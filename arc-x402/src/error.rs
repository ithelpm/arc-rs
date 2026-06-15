use crate::types::PaymentRequired;

#[derive(Debug, thiserror::Error)]
pub enum X402Error {
    #[error("payment required")]
    PaymentRequired(PaymentRequired),

    #[error("invalid payment signature: {0}")]
    InvalidSignature(String),

    #[error("settlement failed: {0}")]
    SettlementFailed(String),

    #[error("gateway API error: {0}")]
    GatewayApiError(String),

    #[error("validation error: {0}")]
    Validation(String),

    #[error("encoding error: {0}")]
    Encoding(String),

    #[error("signing error: {0}")]
    Signing(String),

    #[error("http error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
}
