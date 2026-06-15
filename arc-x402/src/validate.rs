use std::time::{SystemTime, UNIX_EPOCH};

use base64::{engine::general_purpose::STANDARD as B64, Engine};

use crate::{
    constants::{MAX_PRICE_ATOMIC, MIN_VALID_DURATION_SECS},
    error::X402Error,
    types::{PaymentPayload, PaymentRequirements},
};

/// Validates an Ethereum address: must start with "0x" and be exactly 42 hex chars.
pub fn validate_address(addr: &str) -> Result<(), X402Error> {
    let stripped = addr
        .strip_prefix("0x")
        .or_else(|| addr.strip_prefix("0X"))
        .ok_or_else(|| X402Error::Validation(format!("address must start with 0x: {addr}")))?;

    if stripped.len() != 40 {
        return Err(X402Error::Validation(format!(
            "address must be 20 bytes (40 hex chars), got {}: {addr}",
            stripped.len()
        )));
    }

    if !stripped.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(X402Error::Validation(format!(
            "address contains non-hex characters: {addr}"
        )));
    }

    Ok(())
}

/// Validates a private key: must start with "0x" and be exactly 64 hex chars, not all-zero.
pub fn validate_private_key(key: &str) -> Result<(), X402Error> {
    let stripped = key
        .strip_prefix("0x")
        .or_else(|| key.strip_prefix("0X"))
        .ok_or_else(|| X402Error::Validation("private key must start with 0x".to_string()))?;

    if stripped.len() != 64 {
        return Err(X402Error::Validation(format!(
            "private key must be 32 bytes (64 hex chars), got {}",
            stripped.len()
        )));
    }

    if !stripped.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(X402Error::Validation(
            "private key contains non-hex characters".to_string(),
        ));
    }

    if stripped.chars().all(|c| c == '0') {
        return Err(X402Error::Validation(
            "private key must not be zero".to_string(),
        ));
    }

    Ok(())
}

/// Validates a CAIP-2 network identifier (e.g. "eip155:5042002").
pub fn validate_network(network: &str) -> Result<(), X402Error> {
    let parts: Vec<&str> = network.splitn(2, ':').collect();
    if parts.len() != 2 || parts[0].is_empty() || parts[1].is_empty() {
        return Err(X402Error::Validation(format!(
            "network must be CAIP-2 format (e.g. eip155:5042002), got: {network}"
        )));
    }

    // The chain ID part must be numeric
    if parts[1].parse::<u64>().is_err() {
        return Err(X402Error::Validation(format!(
            "network chain ID must be numeric, got: {}",
            parts[1]
        )));
    }

    Ok(())
}

/// Parses a USD price string (e.g. "$0.001") and converts to USDC atomic units (6 decimals).
/// Uses integer arithmetic exclusively to avoid floating-point precision issues.
///
/// Examples: "$0.001" → 1000, "$0.0003" → 300, "$1.00" → 1_000_000
pub fn price_to_atomic(price_usd: &str) -> Result<u64, X402Error> {
    let s = price_usd.trim().trim_start_matches('$');

    if s.is_empty() {
        return Err(X402Error::Validation("price string is empty".to_string()));
    }

    let (integer_part, decimal_part) = if let Some(dot) = s.find('.') {
        (&s[..dot], &s[dot + 1..])
    } else {
        (s, "")
    };

    let int_val: u64 = if integer_part.is_empty() {
        0
    } else {
        integer_part
            .parse()
            .map_err(|_| X402Error::Validation(format!("invalid price integer part: {s}")))?
    };

    // Pad or truncate the decimal part to exactly 6 digits
    let dec_padded = format!("{:0<6}", decimal_part);
    let dec_str = &dec_padded[..6];
    let dec_val: u64 = dec_str
        .parse()
        .map_err(|_| X402Error::Validation(format!("invalid price decimal part: {s}")))?;

    let atomic = int_val
        .checked_mul(1_000_000)
        .and_then(|v| v.checked_add(dec_val))
        .ok_or_else(|| X402Error::Validation(format!("price overflow: {s}")))?;

    if atomic == 0 {
        return Err(X402Error::Validation("price must be greater than zero".to_string()));
    }

    if atomic > MAX_PRICE_ATOMIC {
        return Err(X402Error::Validation(format!(
            "price {atomic} atomic units exceeds maximum {MAX_PRICE_ATOMIC}"
        )));
    }

    Ok(atomic)
}

/// Decodes and validates the `PAYMENT-SIGNATURE` header value.
/// Returns the parsed `PaymentPayload` on success.
pub fn validate_payment_signature(header: &str) -> Result<PaymentPayload, X402Error> {
    let bytes = B64.decode(header.trim()).map_err(|e| {
        X402Error::InvalidSignature(format!("base64 decode failed: {e}"))
    })?;

    let payload: PaymentPayload = serde_json::from_slice(&bytes).map_err(|e| {
        X402Error::InvalidSignature(format!("JSON parse failed: {e}"))
    })?;

    // Basic structural checks
    let auth = &payload.payload.eip3009_auth;

    validate_address(&auth.from)
        .map_err(|e| X402Error::InvalidSignature(format!("invalid `from` address: {e}")))?;

    validate_address(&auth.to)
        .map_err(|e| X402Error::InvalidSignature(format!("invalid `to` address: {e}")))?;

    if auth.value.is_empty() {
        return Err(X402Error::InvalidSignature("`value` field is empty".to_string()));
    }

    if !auth.nonce.starts_with("0x") || auth.nonce.len() != 66 {
        return Err(X402Error::InvalidSignature(format!(
            "nonce must be 0x-prefixed 32 bytes hex, got len {}",
            auth.nonce.len()
        )));
    }

    if !auth.signature.starts_with("0x") || auth.signature.len() != 132 {
        return Err(X402Error::InvalidSignature(format!(
            "signature must be 0x-prefixed 65 bytes hex, got len {}",
            auth.signature.len()
        )));
    }

    Ok(payload)
}

/// Checks that a `PaymentPayload` is compatible with the given `PaymentRequirements`.
/// Verifies: amount, payTo address, network, and that validBefore has not expired.
pub fn validate_payload_vs_requirements(
    payload: &PaymentPayload,
    requirements: &PaymentRequirements,
) -> Result<(), X402Error> {
    let auth = &payload.payload.eip3009_auth;

    // Amount must match
    if auth.value != requirements.amount {
        return Err(X402Error::Validation(format!(
            "payment amount mismatch: payload has {}, requirements expect {}",
            auth.value, requirements.amount
        )));
    }

    // Recipient must match seller address (case-insensitive)
    if !auth.to.eq_ignore_ascii_case(&requirements.pay_to) {
        return Err(X402Error::Validation(format!(
            "payment recipient mismatch: payload has {}, requirements expect {}",
            auth.to, requirements.pay_to
        )));
    }

    // Network must match
    if payload.accepted.network != requirements.network {
        return Err(X402Error::Validation(format!(
            "network mismatch: payload has {}, requirements expect {}",
            payload.accepted.network, requirements.network
        )));
    }

    // validBefore must not have already expired
    let valid_before: u64 = auth.valid_before.parse().map_err(|_| {
        X402Error::Validation(format!("invalid validBefore: {}", auth.valid_before))
    })?;

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before epoch")
        .as_secs();

    if valid_before <= now {
        return Err(X402Error::Validation(format!(
            "payment authorization has expired (validBefore={valid_before}, now={now})"
        )));
    }

    // validBefore must be at least MIN_VALID_DURATION_SECS from now (Circle requirement)
    if valid_before < now + MIN_VALID_DURATION_SECS {
        return Err(X402Error::Validation(format!(
            "validBefore must be at least {} seconds from now (Circle requirement)",
            MIN_VALID_DURATION_SECS
        )));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_address_valid() {
        assert!(validate_address("0xd8dA6BF26964aF9D7eEd9e03E53415D37aA96045").is_ok());
        assert!(validate_address("0x0077777d7EBA4688BDeF3E311b846F25870A19B9").is_ok());
    }

    #[test]
    fn test_validate_address_invalid() {
        assert!(validate_address("not-an-address").is_err());
        assert!(validate_address("0x123").is_err()); // too short
        assert!(validate_address("d8dA6BF26964aF9D7eEd9e03E53415D37aA96045").is_err()); // no 0x
    }

    #[test]
    fn test_price_to_atomic() {
        assert_eq!(price_to_atomic("$0.001").unwrap(), 1_000);
        assert_eq!(price_to_atomic("$0.0003").unwrap(), 300);
        assert_eq!(price_to_atomic("$0.01").unwrap(), 10_000);
        assert_eq!(price_to_atomic("$0.03").unwrap(), 30_000);
        assert_eq!(price_to_atomic("$1.00").unwrap(), 1_000_000);
        assert_eq!(price_to_atomic("1.00").unwrap(), 1_000_000); // no $ sign
        assert_eq!(price_to_atomic("$0.000001").unwrap(), 1); // minimum
    }

    #[test]
    fn test_price_to_atomic_invalid() {
        assert!(price_to_atomic("$0").is_err()); // zero
        assert!(price_to_atomic("$-1.0").is_err());
        assert!(price_to_atomic("not-a-price").is_err());
        assert!(price_to_atomic("").is_err());
    }

    #[test]
    fn test_validate_network() {
        assert!(validate_network("eip155:5042002").is_ok());
        assert!(validate_network("eip155:1").is_ok());
        assert!(validate_network("invalid").is_err());
        assert!(validate_network(":5042002").is_err());
        assert!(validate_network("eip155:abc").is_err());
    }

    #[test]
    fn test_validate_private_key_invalid() {
        assert!(validate_private_key("no-prefix").is_err());
        assert!(validate_private_key("0x0000000000000000000000000000000000000000000000000000000000000000").is_err()); // zero
        assert!(validate_private_key("0xshort").is_err());
    }
}
