use std::time::{SystemTime, UNIX_EPOCH};

use base64::{engine::general_purpose::STANDARD as B64, Engine};
use k256::ecdsa::{SigningKey, VerifyingKey};
use rand::RngExt;

use crate::{
    constants::{
        ARC_TESTNET_CHAIN_ID, GATEWAY_DOMAIN_NAME, GATEWAY_DOMAIN_VERSION,
        MIN_VALID_DURATION_SECS, X402_VERSION,
    },
    eip712::eip712_digest,
    error::X402Error,
    types::{Eip3009Authorization, ExactPayload, PaymentPayload, PaymentRequirements, ResourceInfo},
    validate::validate_private_key,
};

/// Derives the Ethereum address from a 0x-prefixed hex private key.
pub fn address_from_private_key(private_key_hex: &str) -> Result<String, X402Error> {
    validate_private_key(private_key_hex)?;

    let key_bytes = hex::decode(private_key_hex.trim_start_matches("0x")).map_err(|e| {
        X402Error::Signing(format!("failed to decode private key: {e}"))
    })?;

    let signing_key = SigningKey::from_bytes(key_bytes.as_slice().into()).map_err(|e| {
        X402Error::Signing(format!("invalid private key: {e}"))
    })?;

    let verifying_key = VerifyingKey::from(&signing_key);
    let point = verifying_key.to_encoded_point(false);
    let public_key_bytes = &point.as_bytes()[1..]; // strip 0x04 prefix

    let hash = tiny_keccak_hash(public_key_bytes);
    let address_bytes = &hash[12..]; // last 20 bytes

    Ok(format!("0x{}", hex::encode(address_bytes)))
}

fn tiny_keccak_hash(data: &[u8]) -> [u8; 32] {
    use tiny_keccak::{Hasher, Keccak};
    let mut output = [0u8; 32];
    let mut hasher = Keccak::v256();
    hasher.update(data);
    hasher.finalize(&mut output);
    output
}

/// Signs an EIP-712 digest with a private key.
/// Returns a 65-byte signature in Ethereum format: `r || s || v` (v = recovery_id + 27),
/// encoded as a 0x-prefixed hex string.
fn sign_digest(private_key_hex: &str, digest: &[u8; 32]) -> Result<String, X402Error> {
    let key_bytes = hex::decode(private_key_hex.trim_start_matches("0x")).map_err(|e| {
        X402Error::Signing(format!("failed to decode private key: {e}"))
    })?;

    let signing_key = SigningKey::from_bytes(key_bytes.as_slice().into()).map_err(|e| {
        X402Error::Signing(format!("invalid signing key: {e}"))
    })?;

    let (signature, recovery_id) =
        signing_key.sign_prehash_recoverable(digest).map_err(|e| {
            X402Error::Signing(format!("signing failed: {e}"))
        })?;

    let sig_bytes = signature.to_bytes();
    let v: u8 = recovery_id.to_byte() + 27;

    let mut result = Vec::with_capacity(65);
    result.extend_from_slice(&sig_bytes);
    result.push(v);

    Ok(format!("0x{}", hex::encode(result)))
}

/// Generates a random 32-byte nonce as a 0x-prefixed hex string.
fn generate_nonce() -> String {
    let mut bytes = [0u8; 32];
    rand::rng().fill(&mut bytes);
    format!("0x{}", hex::encode(bytes))
}

/// Builds a fully-signed `PaymentPayload` for the given requirements.
///
/// This is the main entry point for the buyer side. It:
/// 1. Validates the private key
/// 2. Generates a random nonce
/// 3. Sets validAfter=0 and validBefore=now+MIN_VALID_DURATION_SECS
/// 4. Computes the EIP-712 digest
/// 5. Signs it with k256 (RFC-6979 deterministic)
/// 6. Returns a complete `PaymentPayload` ready for base64 encoding
pub fn build_payment_payload(
    private_key_hex: &str,
    requirements: &PaymentRequirements,
    resource: &ResourceInfo,
) -> Result<PaymentPayload, X402Error> {
    validate_private_key(private_key_hex)?;

    let from_address = address_from_private_key(private_key_hex)?;

    let nonce = generate_nonce();

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before epoch")
        .as_secs();

    let valid_after = "0".to_string();
    let valid_before = (now + MIN_VALID_DURATION_SECS).to_string();

    let auth = Eip3009Authorization {
        from: from_address,
        to: requirements.pay_to.clone(),
        value: requirements.amount.clone(),
        valid_after,
        valid_before,
        nonce,
        signature: String::new(), // filled in after signing
    };

    let verifying_contract = &requirements.extra.verifying_contract;
    let digest = eip712_digest(
        GATEWAY_DOMAIN_NAME,
        GATEWAY_DOMAIN_VERSION,
        ARC_TESTNET_CHAIN_ID,
        verifying_contract,
        &auth,
    );

    let signature = sign_digest(private_key_hex, &digest)?;

    let signed_auth = Eip3009Authorization {
        signature,
        ..auth
    };

    Ok(PaymentPayload {
        x402_version: X402_VERSION,
        resource: resource.clone(),
        accepted: requirements.clone(),
        payload: ExactPayload {
            eip3009_auth: signed_auth,
        },
    })
}

/// Encodes a `PaymentPayload` as a base64 string for use in the `PAYMENT-SIGNATURE` header.
pub fn encode_payload(payload: &PaymentPayload) -> Result<String, X402Error> {
    let json = serde_json::to_vec(payload)?;
    Ok(B64.encode(json))
}

/// Decodes the `PAYMENT-SIGNATURE` header value back into a `PaymentPayload`.
pub fn decode_payload(header: &str) -> Result<PaymentPayload, X402Error> {
    let bytes = B64.decode(header.trim()).map_err(|e| {
        X402Error::Encoding(format!("base64 decode failed: {e}"))
    })?;
    let payload = serde_json::from_slice(&bytes)?;
    Ok(payload)
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_PRIVATE_KEY: &str =
        "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80";

    #[test]
    fn test_address_from_private_key() {
        // Known address for the Hardhat account #0 private key
        let addr = address_from_private_key(TEST_PRIVATE_KEY).unwrap();
        assert!(addr.starts_with("0x"));
        assert_eq!(addr.len(), 42);
    }

    #[test]
    fn test_generate_nonce_format() {
        let nonce = generate_nonce();
        assert!(nonce.starts_with("0x"));
        assert_eq!(nonce.len(), 66); // "0x" + 64 hex chars
    }

    #[test]
    fn test_sign_digest_format() {
        let digest = [0x42u8; 32];
        let sig = sign_digest(TEST_PRIVATE_KEY, &digest).unwrap();
        assert!(sig.starts_with("0x"));
        assert_eq!(sig.len(), 132); // "0x" + 130 hex chars (65 bytes)
    }

    #[test]
    fn test_encode_decode_roundtrip() {
        use crate::{
            constants::ARC_TESTNET_GATEWAY_WALLET,
            types::{GatewayExtra, PaymentRequirements, ResourceInfo},
        };

        let requirements = PaymentRequirements {
            scheme: "exact".to_string(),
            network: "eip155:5042002".to_string(),
            asset: "0x3600000000000000000000000000000000000000".to_string(),
            amount: "1000".to_string(),
            pay_to: "0xd8dA6BF26964aF9D7eEd9e03E53415D37aA96045".to_string(),
            max_timeout_seconds: 604900,
            extra: GatewayExtra {
                name: "GatewayWalletBatched".to_string(),
                version: "1".to_string(),
                verifying_contract: ARC_TESTNET_GATEWAY_WALLET.to_string(),
            },
        };

        let resource = ResourceInfo {
            url: "/api/premium/quote".to_string(),
            description: "Test resource".to_string(),
            mime_type: "application/json".to_string(),
        };

        let payload = build_payment_payload(TEST_PRIVATE_KEY, &requirements, &resource).unwrap();
        let encoded = encode_payload(&payload).unwrap();
        let decoded = decode_payload(&encoded).unwrap();

        assert_eq!(decoded.x402_version, payload.x402_version);
        assert_eq!(
            decoded.payload.eip3009_auth.from,
            payload.payload.eip3009_auth.from
        );
        assert_eq!(
            decoded.payload.eip3009_auth.signature,
            payload.payload.eip3009_auth.signature
        );
    }
}
