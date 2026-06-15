use reqwest::Client;
use serde_json::json;

use crate::{
    constants::GATEWAY_API_TESTNET,
    error::X402Error,
    types::{BalancesResponse, PaymentPayload, PaymentRequirements, SettleResult, VerifyResult},
};

/// Thin async HTTP client for the Circle Gateway REST API.
pub struct GatewayApiClient {
    http: Client,
    pub base_url: String,
}

impl GatewayApiClient {
    /// Creates a client pointing at the given base URL (no trailing slash).
    pub fn new(base_url: &str) -> Self {
        Self {
            http: Client::new(),
            base_url: base_url.trim_end_matches('/').to_string(),
        }
    }

    /// Creates a client pre-configured for the Arc testnet Gateway.
    pub fn testnet() -> Self {
        Self::new(GATEWAY_API_TESTNET)
    }

    /// Submits a signed payment payload for settlement.
    ///
    /// Circle recommends calling `settle` directly rather than `verify` + `settle`
    /// because settle already validates the payload and guarantees low-latency settlement.
    pub async fn settle(
        &self,
        payload: &PaymentPayload,
        requirements: &PaymentRequirements,
    ) -> Result<SettleResult, X402Error> {
        let body = json!({
            "payment": payload,
            "paymentRequirements": requirements,
        });

        let response = self
            .http
            .post(format!("{}/v1/payments/settle", self.base_url))
            .json(&body)
            .send()
            .await
            .map_err(|e| X402Error::GatewayApiError(format!("settle request failed: {e}")))?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(X402Error::GatewayApiError(format!(
                "settle returned HTTP {status}: {text}"
            )));
        }

        let result: SettleResult = response.json().await.map_err(|e| {
            X402Error::GatewayApiError(format!("failed to parse settle response: {e}"))
        })?;

        Ok(result)
    }

    /// Verifies a signed payment payload without settling it.
    ///
    /// Note: prefer `settle()` in production — it verifies and settles atomically.
    /// Use `verify()` only when you need a dry-run check.
    pub async fn verify(
        &self,
        payload: &PaymentPayload,
        requirements: &PaymentRequirements,
    ) -> Result<VerifyResult, X402Error> {
        let body = json!({
            "payment": payload,
            "paymentRequirements": requirements,
        });

        let response = self
            .http
            .post(format!("{}/v1/payments/verify", self.base_url))
            .json(&body)
            .send()
            .await
            .map_err(|e| X402Error::GatewayApiError(format!("verify request failed: {e}")))?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(X402Error::GatewayApiError(format!(
                "verify returned HTTP {status}: {text}"
            )));
        }

        let result: VerifyResult = response.json().await.map_err(|e| {
            X402Error::GatewayApiError(format!("failed to parse verify response: {e}"))
        })?;

        Ok(result)
    }

    /// Queries Gateway USDC balances for a depositor on a specific domain (chain).
    pub async fn get_balances(
        &self,
        depositor: &str,
        domain: u32,
    ) -> Result<BalancesResponse, X402Error> {
        let body = json!({
            "token": "USDC",
            "sources": [{ "domain": domain, "depositor": depositor }]
        });

        let response = self
            .http
            .post(format!("{}/v1/balances", self.base_url))
            .json(&body)
            .send()
            .await
            .map_err(|e| X402Error::GatewayApiError(format!("balances request failed: {e}")))?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(X402Error::GatewayApiError(format!(
                "balances returned HTTP {status}: {text}"
            )));
        }

        let result: BalancesResponse = response.json().await.map_err(|e| {
            X402Error::GatewayApiError(format!("failed to parse balances response: {e}"))
        })?;

        Ok(result)
    }
}
