pub mod client;
pub mod constants;
pub mod error;
pub mod eip712;
pub mod gateway;
pub mod server;
pub mod signer;
pub mod types;
pub mod validate;

// Convenient top-level re-exports
pub use client::{BuyerClient, CreateSessionResponse};
pub use server::{build_buy_requirements, build_chunk_requirements};
pub use error::X402Error;
pub use gateway::GatewayApiClient;
pub use types::{
    BalancesResponse, Eip3009Authorization, ExactPayload, GatewayBalance, GatewayExtra,
    PaymentPayload, PaymentRequired, PaymentRequirements, PaymentResponse, PayResult,
    ResourceInfo, SettleResult, VerifyResult,
};
pub use validate::price_to_atomic;

#[cfg(feature = "axum-middleware")]
pub use server::{payment_middleware, require_payment, PayerAddress, PaymentAmount, PaymentGateway};
