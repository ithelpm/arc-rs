use serde::{Deserialize, Serialize};

/// The `extra` field inside a payment requirement, specifying the EIP-712 domain.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GatewayExtra {
    pub name: String,
    pub version: String,
    #[serde(rename = "verifyingContract")]
    pub verifying_contract: String,
}

/// One acceptable payment method declared in a 402 response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaymentRequirements {
    pub scheme: String,
    pub network: String,
    pub asset: String,
    /// Amount in USDC atomic units (6 decimals), as a decimal string.
    pub amount: String,
    #[serde(rename = "payTo")]
    pub pay_to: String,
    #[serde(rename = "maxTimeoutSeconds")]
    pub max_timeout_seconds: u64,
    pub extra: GatewayExtra,
}

/// Metadata about the resource being sold, embedded in payment headers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceInfo {
    pub url: String,
    pub description: String,
    #[serde(rename = "mimeType")]
    pub mime_type: String,
}

/// Full JSON content of the `PAYMENT-REQUIRED` response header (base64-decoded).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaymentRequired {
    #[serde(rename = "x402Version")]
    pub x402_version: u32,
    pub resource: ResourceInfo,
    pub accepts: Vec<PaymentRequirements>,
}

/// EIP-3009 TransferWithAuthorization fields + the resulting signature.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Eip3009Authorization {
    pub from: String,
    pub to: String,
    /// Payment amount in atomic units, as a decimal string.
    pub value: String,
    /// Unix timestamp (seconds) after which the auth is valid. Typically 0.
    #[serde(rename = "validAfter")]
    pub valid_after: String,
    /// Unix timestamp (seconds) before which the auth is valid. Must be >= now + 7 days.
    #[serde(rename = "validBefore")]
    pub valid_before: String,
    /// 32-byte random nonce, 0x-prefixed hex. Must never be reused.
    pub nonce: String,
    /// 65-byte ECDSA signature (r || s || v), 0x-prefixed hex. v = recovery_id + 27.
    pub signature: String,
}

/// The scheme-specific payload for the "exact" nanopayment scheme.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExactPayload {
    #[serde(rename = "eip3009Auth")]
    pub eip3009_auth: Eip3009Authorization,
}

/// Full JSON content of the `PAYMENT-SIGNATURE` request header (base64-decoded).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaymentPayload {
    #[serde(rename = "x402Version")]
    pub x402_version: u32,
    pub resource: ResourceInfo,
    pub accepted: PaymentRequirements,
    pub payload: ExactPayload,
}

/// Full JSON content of the `PAYMENT-RESPONSE` response header (base64-decoded).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaymentResponse {
    pub success: bool,
    pub transaction: String,
    pub network: String,
    pub payer: String,
}

/// Response from the Circle Gateway settle endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SettleResult {
    pub success: bool,
    #[serde(rename = "errorReason")]
    pub error_reason: Option<String>,
    pub transaction: Option<String>,
    pub network: Option<String>,
    pub payer: Option<String>,
}

/// Response from the Circle Gateway verify endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerifyResult {
    #[serde(rename = "isValid")]
    pub is_valid: bool,
    #[serde(rename = "invalidReason")]
    pub invalid_reason: Option<String>,
    pub payer: Option<String>,
}

/// One chain's balance entry from the Gateway balances API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GatewayBalance {
    pub domain: u32,
    pub depositor: Option<String>,
    pub balance: String,
    pub withdrawing: Option<String>,
    pub withdrawable: Option<String>,
}

/// Response from the Circle Gateway balances endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BalancesResponse {
    pub token: Option<String>,
    pub balances: Vec<GatewayBalance>,
}

/// Result returned by `BuyerClient::pay()`.
pub struct PayResult {
    pub body: Vec<u8>,
    pub payment_response: Option<PaymentResponse>,
    /// Formatted USDC amount paid (e.g. "0.001").
    pub amount_paid_usdc: String,
    pub elapsed_ms: u64,
}
