use tiny_keccak::{Hasher, Keccak};

use crate::types::Eip3009Authorization;

fn keccak256(data: &[u8]) -> [u8; 32] {
    let mut output = [0u8; 32];
    let mut hasher = Keccak::v256();
    hasher.update(data);
    hasher.finalize(&mut output);
    output
}

/// EIP-712 type string for TransferWithAuthorization.
/// Must match the Solidity definition exactly — any whitespace difference changes the hash.
const TRANSFER_WITH_AUTHORIZATION_TYPEHASH_STR: &str =
    "TransferWithAuthorization(address from,address to,uint256 value,uint256 validAfter,uint256 validBefore,bytes32 nonce)";

/// EIP-712 type string for the domain separator (GatewayWalletBatched).
const DOMAIN_TYPEHASH_STR: &str =
    "EIP712Domain(string name,string version,uint256 chainId,address verifyingContract)";

lazy_static::lazy_static! {
    static ref TRANSFER_TYPEHASH: [u8; 32] =
        keccak256(TRANSFER_WITH_AUTHORIZATION_TYPEHASH_STR.as_bytes());

    static ref DOMAIN_TYPEHASH: [u8; 32] =
        keccak256(DOMAIN_TYPEHASH_STR.as_bytes());
}

/// Parses a 0x-prefixed hex address and returns it as a 32-byte word (left-padded with zeros).
fn address_to_word(addr: &str) -> [u8; 32] {
    let hex = addr.trim_start_matches("0x").trim_start_matches("0X");
    let bytes = hex::decode(hex).unwrap_or_default();
    let mut word = [0u8; 32];
    // Address is 20 bytes, placed in the rightmost 20 bytes of the 32-byte word
    let offset = 32 - bytes.len().min(20);
    word[offset..offset + bytes.len().min(20)].copy_from_slice(&bytes[bytes.len().saturating_sub(20)..]);
    word
}

/// Parses a decimal string integer and returns it as a 32-byte big-endian word.
fn uint256_to_word(value: &str) -> [u8; 32] {
    let n: u128 = value.parse().unwrap_or(0);
    let mut word = [0u8; 32];
    word[16..].copy_from_slice(&n.to_be_bytes());
    word
}

/// Parses a 0x-prefixed hex bytes32 and returns it as 32 bytes.
fn bytes32_to_word(hex_str: &str) -> [u8; 32] {
    let hex = hex_str.trim_start_matches("0x").trim_start_matches("0X");
    let bytes = hex::decode(hex).unwrap_or_default();
    let mut word = [0u8; 32];
    let len = bytes.len().min(32);
    word[..len].copy_from_slice(&bytes[..len]);
    word
}

/// Computes the EIP-712 domain separator for GatewayWalletBatched on a given chain.
///
/// Domain: { name: "GatewayWalletBatched", version: "1", chainId: X, verifyingContract: Y }
pub fn gateway_domain_separator(
    name: &str,
    version: &str,
    chain_id: u64,
    verifying_contract: &str,
) -> [u8; 32] {
    let name_hash = keccak256(name.as_bytes());
    let version_hash = keccak256(version.as_bytes());

    let mut chain_id_word = [0u8; 32];
    chain_id_word[24..].copy_from_slice(&chain_id.to_be_bytes());

    let contract_word = address_to_word(verifying_contract);

    // abi.encode(DOMAIN_TYPEHASH, name_hash, version_hash, chainId, verifyingContract)
    let mut encoded = Vec::with_capacity(5 * 32);
    encoded.extend_from_slice(&*DOMAIN_TYPEHASH);
    encoded.extend_from_slice(&name_hash);
    encoded.extend_from_slice(&version_hash);
    encoded.extend_from_slice(&chain_id_word);
    encoded.extend_from_slice(&contract_word);

    keccak256(&encoded)
}

/// Computes the EIP-712 struct hash for a TransferWithAuthorization.
pub fn transfer_with_authorization_hash(auth: &Eip3009Authorization) -> [u8; 32] {
    let from_word = address_to_word(&auth.from);
    let to_word = address_to_word(&auth.to);
    let value_word = uint256_to_word(&auth.value);
    let valid_after_word = uint256_to_word(&auth.valid_after);
    let valid_before_word = uint256_to_word(&auth.valid_before);
    let nonce_word = bytes32_to_word(&auth.nonce);

    // abi.encode(TYPEHASH, from, to, value, validAfter, validBefore, nonce)
    let mut encoded = Vec::with_capacity(7 * 32);
    encoded.extend_from_slice(&*TRANSFER_TYPEHASH);
    encoded.extend_from_slice(&from_word);
    encoded.extend_from_slice(&to_word);
    encoded.extend_from_slice(&value_word);
    encoded.extend_from_slice(&valid_after_word);
    encoded.extend_from_slice(&valid_before_word);
    encoded.extend_from_slice(&nonce_word);

    keccak256(&encoded)
}

/// Computes the final EIP-712 digest:
/// keccak256("\x19\x01" || domainSeparator || structHash)
pub fn eip712_digest(
    name: &str,
    version: &str,
    chain_id: u64,
    verifying_contract: &str,
    auth: &Eip3009Authorization,
) -> [u8; 32] {
    let domain_sep = gateway_domain_separator(name, version, chain_id, verifying_contract);
    let struct_hash = transfer_with_authorization_hash(auth);

    let mut data = Vec::with_capacity(2 + 32 + 32);
    data.extend_from_slice(&[0x19, 0x01]);
    data.extend_from_slice(&domain_sep);
    data.extend_from_slice(&struct_hash);

    keccak256(&data)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_typehash_is_deterministic() {
        let h1 = keccak256(TRANSFER_WITH_AUTHORIZATION_TYPEHASH_STR.as_bytes());
        let h2 = keccak256(TRANSFER_WITH_AUTHORIZATION_TYPEHASH_STR.as_bytes());
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_address_to_word_padding() {
        let word = address_to_word("0x0077777d7EBA4688BDeF3E311b846F25870A19B9");
        // First 12 bytes must be zero
        assert_eq!(&word[..12], &[0u8; 12]);
        // Last 20 bytes must be the address
        assert_ne!(&word[12..], &[0u8; 20]);
    }

    #[test]
    fn test_uint256_to_word() {
        let word = uint256_to_word("1000");
        // Value 1000 = 0x3E8 in big-endian
        assert_eq!(word[31], 0xe8);
        assert_eq!(word[30], 0x03);
        assert_eq!(&word[..30], &[0u8; 30]);
    }

    #[test]
    fn test_domain_separator_deterministic() {
        let sep1 = gateway_domain_separator(
            "GatewayWalletBatched",
            "1",
            5042002,
            "0x0077777d7EBA4688BDeF3E311b846F25870A19B9",
        );
        let sep2 = gateway_domain_separator(
            "GatewayWalletBatched",
            "1",
            5042002,
            "0x0077777d7EBA4688BDeF3E311b846F25870A19B9",
        );
        assert_eq!(sep1, sep2);
    }

    #[test]
    fn test_domain_separator_changes_with_chain_id() {
        let sep1 = gateway_domain_separator(
            "GatewayWalletBatched",
            "1",
            5042002,
            "0x0077777d7EBA4688BDeF3E311b846F25870A19B9",
        );
        let sep2 = gateway_domain_separator(
            "GatewayWalletBatched",
            "1",
            1,
            "0x0077777d7EBA4688BDeF3E311b846F25870A19B9",
        );
        assert_ne!(sep1, sep2);
    }
}
