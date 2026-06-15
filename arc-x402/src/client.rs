use std::time::Instant;

use base64::{engine::general_purpose::STANDARD as B64, Engine};
use reqwest::{Client, Method};
use serde::{Deserialize, Serialize};

use crate::{
    constants::ARC_TESTNET_DOMAIN_ID,
    error::X402Error,
    gateway::GatewayApiClient,
    signer::{address_from_private_key, build_payment_payload, encode_payload},
    types::{PaymentRequired, PaymentRequirements, PaymentResponse, PayResult, ResourceInfo},
    validate::validate_private_key,
};

/// Response from creating a per-second streaming session on the proxy server.
#[derive(Debug, Deserialize)]
pub struct CreateSessionResponse {
    pub session_id: String,
    pub chunk_price_atomic: u64,
    pub chunk_secs: u64,
    pub rate_per_sec_atomic: u64,
    /// Seller address — needed to reconstruct `PaymentRequirements` for chunk signing.
    pub pay_to: String,
}

/// HTTP client for the buyer side of the x402 protocol.
///
/// Handles the full payment flow: initial request → 402 response →
/// sign EIP-3009 authorization → retry with PAYMENT-SIGNATURE header.
pub struct BuyerClient {
    /// k256 signing key stored as raw bytes (not as a plain string).
    key_bytes: Vec<u8>,
    address: String,
    http: Client,
}

impl BuyerClient {
    /// Creates a new `BuyerClient` from a 0x-prefixed hex private key.
    /// Validates the key format immediately — fails fast rather than at pay time.
    pub fn new(private_key_hex: &str) -> Result<Self, X402Error> {
        validate_private_key(private_key_hex)?;

        let address = address_from_private_key(private_key_hex)?;
        let key_bytes = hex::decode(private_key_hex.trim_start_matches("0x"))
            .map_err(|e| X402Error::Signing(format!("failed to decode private key: {e}")))?;

        Ok(Self {
            key_bytes,
            address,
            http: Client::new(),
        })
    }

    /// Returns the Ethereum address derived from this client's private key.
    pub fn address(&self) -> &str {
        &self.address
    }

    fn private_key_hex(&self) -> String {
        format!("0x{}", hex::encode(&self.key_bytes))
    }

    /// Performs the full x402 payment flow for the given URL:
    /// 1. Sends the initial request (no payment).
    /// 2. If 402 received, parses `PAYMENT-REQUIRED` header.
    /// 3. Signs an EIP-3009 authorization.
    /// 4. Retries the request with the `PAYMENT-SIGNATURE` header.
    /// 5. Returns the response body and payment metadata.
    pub async fn pay(
        &self,
        url: &str,
        method: &str,
        body: Option<&[u8]>,
    ) -> Result<PayResult, X402Error> {
        let start = Instant::now();
        let http_method = method.parse::<Method>().map_err(|e| {
            X402Error::Validation(format!("invalid HTTP method '{method}': {e}"))
        })?;

        // ── Step 1: Initial request (no payment header) ──
        let mut req_builder = self.http.request(http_method.clone(), url);
        if let Some(b) = body {
            req_builder = req_builder
                .header("content-type", "application/json")
                .body(b.to_vec());
        }

        let initial_response = req_builder.send().await.map_err(|e| {
            X402Error::Http(e)
        })?;

        // ── Step 2: If not 402, return the response directly ──
        if initial_response.status() != reqwest::StatusCode::PAYMENT_REQUIRED {
            let status = initial_response.status();
            let response_body = initial_response.bytes().await.map_err(X402Error::Http)?;
            if status.is_success() {
                return Ok(PayResult {
                    body: response_body.to_vec(),
                    payment_response: None,
                    amount_paid_usdc: "0".to_string(),
                    elapsed_ms: start.elapsed().as_millis() as u64,
                });
            }
            return Err(X402Error::GatewayApiError(format!(
                "unexpected HTTP {status}"
            )));
        }

        // ── Step 3: Parse PAYMENT-REQUIRED header ──
        let payment_required_b64 = initial_response
            .headers()
            .get("payment-required")
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| {
                X402Error::InvalidSignature(
                    "server returned 402 but no PAYMENT-REQUIRED header".to_string(),
                )
            })?
            .to_owned();

        let pr_bytes = B64.decode(payment_required_b64.trim()).map_err(|e| {
            X402Error::InvalidSignature(format!("failed to decode PAYMENT-REQUIRED: {e}"))
        })?;
        let payment_required: PaymentRequired = serde_json::from_slice(&pr_bytes)?;

        // Pick the first accepted payment option
        let requirements = payment_required
            .accepts
            .into_iter()
            .next()
            .ok_or_else(|| {
                X402Error::InvalidSignature("PAYMENT-REQUIRED has no accepts entries".to_string())
            })?;

        // ── Step 4: Sign EIP-3009 authorization ──
        let payload = build_payment_payload(
            &self.private_key_hex(),
            &requirements,
            &payment_required.resource,
        )?;
        let signature_header = encode_payload(&payload)?;

        // ── Step 5: Retry with PAYMENT-SIGNATURE header ──
        let mut retry_builder = self
            .http
            .request(http_method, url)
            .header("payment-signature", &signature_header);
        if let Some(b) = body {
            retry_builder = retry_builder
                .header("content-type", "application/json")
                .body(b.to_vec());
        }

        let paid_response = retry_builder.send().await.map_err(X402Error::Http)?;
        let status = paid_response.status();

        // Parse PAYMENT-RESPONSE header if present
        let payment_response: Option<PaymentResponse> = paid_response
            .headers()
            .get("payment-response")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| B64.decode(v.trim()).ok())
            .and_then(|bytes| serde_json::from_slice(&bytes).ok());

        let response_body = paid_response.bytes().await.map_err(X402Error::Http)?;

        if !status.is_success() {
            return Err(X402Error::SettlementFailed(format!(
                "paid request returned HTTP {status}: {}",
                String::from_utf8_lossy(&response_body)
            )));
        }

        // Format the amount for display
        let amount_atomic: u64 = requirements.amount.parse().unwrap_or(0);
        let amount_paid_usdc = format!("{:.6}", amount_atomic as f64 / 1_000_000.0)
            .trim_end_matches('0')
            .trim_end_matches('.')
            .to_string();
        // Ensure at least one decimal place
        let amount_paid_usdc = if amount_paid_usdc.contains('.') {
            amount_paid_usdc
        } else {
            format!("{amount_paid_usdc}.0")
        };

        Ok(PayResult {
            body: response_body.to_vec(),
            payment_response,
            amount_paid_usdc,
            elapsed_ms: start.elapsed().as_millis() as u64,
        })
    }

    /// Probes whether a URL returns a Gateway-compatible 402 response.
    /// Returns `true` if the URL supports x402 with Circle Gateway nanopayments.
    pub async fn supports(&self, url: &str) -> Result<bool, X402Error> {
        let response = self
            .http
            .get(url)
            .send()
            .await
            .map_err(X402Error::Http)?;

        if response.status() != reqwest::StatusCode::PAYMENT_REQUIRED {
            return Ok(false);
        }

        let has_payment_required = response.headers().contains_key("payment-required");
        if !has_payment_required {
            return Ok(false);
        }

        // Try to parse the header and check for "exact" scheme
        if let Some(header_val) = response.headers().get("payment-required")
            .and_then(|v| v.to_str().ok())
        {
            if let Ok(bytes) = B64.decode(header_val.trim()) {
                if let Ok(pr) = serde_json::from_slice::<PaymentRequired>(&bytes) {
                    return Ok(pr.accepts.iter().any(|a| a.scheme == "exact"));
                }
            }
        }

        Ok(false)
    }

    /// Queries this wallet's Gateway balance on the Arc testnet.
    pub async fn get_gateway_balance(
        &self,
        gateway: &GatewayApiClient,
    ) -> Result<String, X402Error> {
        let balances = gateway
            .get_balances(&self.address, ARC_TESTNET_DOMAIN_ID)
            .await?;

        let balance = balances
            .balances
            .first()
            .map(|b| b.balance.clone())
            .unwrap_or_else(|| "0".to_string());

        Ok(balance)
    }

    /// Signs a single streaming chunk payment and returns the base64 `payment-signature` value.
    ///
    /// Call once per chunk before each `stream_chunk` request.
    pub fn sign_chunk(
        &self,
        requirements: &PaymentRequirements,
        resource: &ResourceInfo,
    ) -> Result<String, X402Error> {
        let payload = build_payment_payload(&self.private_key_hex(), requirements, resource)?;
        encode_payload(&payload)
    }

    /// Creates a per-second billing session on the proxy server.
    ///
    /// Returns session metadata including the per-chunk price and the seller's address
    /// needed to reconstruct `PaymentRequirements` for chunk signing.
    pub async fn create_stream_session(
        &self,
        base_url: &str,
        item_id: &str,
    ) -> Result<CreateSessionResponse, X402Error> {
        #[derive(Serialize)]
        struct Req<'a> {
            wallet: &'a str,
            item_id: &'a str,
        }

        let url = format!("{}/api/session", base_url.trim_end_matches('/'));
        let resp = self
            .http
            .post(&url)
            .json(&Req { wallet: &self.address, item_id })
            .send()
            .await
            .map_err(X402Error::Http)?;

        if !resp.status().is_success() {
            return Err(X402Error::GatewayApiError(format!(
                "create_stream_session returned HTTP {}",
                resp.status()
            )));
        }

        resp.json::<CreateSessionResponse>().await.map_err(|e| {
            X402Error::GatewayApiError(format!("failed to parse CreateSessionResponse: {e}"))
        })
    }

    /// Buys permanent access to a Jellyfin item via the x402 buy-to-access flow.
    ///
    /// Passes `wallet` as a query parameter so the proxy's fast-path cache check fires
    /// on both the initial probe and the signed retry — avoiding a double charge on
    /// subsequent calls once access is already granted.
    pub async fn buy_access(
        &self,
        base_url: &str,
        item_id: &str,
    ) -> Result<PayResult, X402Error> {
        let url = format!(
            "{}/content/{}?wallet={}",
            base_url.trim_end_matches('/'),
            item_id,
            self.address
        );
        self.pay(&url, "GET", None).await
    }

    /// Fetches a single timed streaming chunk, paying with `requirements` inline.
    ///
    /// Signs the chunk, sends the request with the `payment-signature` header,
    /// and returns the raw video bytes for that segment.
    pub async fn stream_chunk(
        &self,
        base_url: &str,
        item_id: &str,
        session_id: &str,
        requirements: &PaymentRequirements,
        resource: &ResourceInfo,
    ) -> Result<Vec<u8>, X402Error> {
        let signature = self.sign_chunk(requirements, resource)?;
        let url = format!(
            "{}/content/{}?session_id={}",
            base_url.trim_end_matches('/'),
            item_id,
            session_id
        );
        let resp = self
            .http
            .get(&url)
            .header("payment-signature", &signature)
            .send()
            .await
            .map_err(X402Error::Http)?;

        if !resp.status().is_success() {
            return Err(X402Error::SettlementFailed(format!(
                "stream_chunk HTTP {}",
                resp.status()
            )));
        }

        resp.bytes().await.map_err(X402Error::Http).map(|b| b.to_vec())
    }
}
