use base64::{engine::general_purpose::STANDARD as B64, Engine};

use crate::{
    constants::{
        ARC_TESTNET_CAIP2, ARC_TESTNET_GATEWAY_WALLET, ARC_TESTNET_USDC,
        DEFAULT_MAX_TIMEOUT_SECONDS, GATEWAY_DOMAIN_NAME, GATEWAY_DOMAIN_VERSION, X402_VERSION,
    },
    error::X402Error,
    gateway::GatewayApiClient,
    types::{
        GatewayExtra, PaymentRequired, PaymentRequirements, PaymentResponse, ResourceInfo,
        SettleResult,
    },
    validate::{price_to_atomic, validate_address, validate_payment_signature,
               validate_payload_vs_requirements},
};

/// Encodes a `PaymentRequired` struct as the base64 value for the `PAYMENT-REQUIRED` header.
pub fn build_payment_required_header(
    requirements: &PaymentRequirements,
    resource: &ResourceInfo,
) -> Result<String, X402Error> {
    let payment_required = PaymentRequired {
        x402_version: X402_VERSION,
        resource: resource.clone(),
        accepts: vec![requirements.clone()],
    };
    let json = serde_json::to_vec(&payment_required)?;
    Ok(B64.encode(json))
}

/// Encodes a `SettleResult` as the base64 value for the `PAYMENT-RESPONSE` header.
pub fn build_payment_response_header(settle: &SettleResult) -> Result<String, X402Error> {
    let response = PaymentResponse {
        success: settle.success,
        transaction: settle.transaction.clone().unwrap_or_default(),
        network: settle.network.clone().unwrap_or_else(|| ARC_TESTNET_CAIP2.to_string()),
        payer: settle.payer.clone().unwrap_or_default(),
    };
    let json = serde_json::to_vec(&response)?;
    Ok(B64.encode(json))
}

/// Builds a `PaymentRequirements` struct from a dollar-price string and seller address.
///
/// Uses Arc testnet defaults for network, asset, and gateway wallet.
pub fn build_requirements(
    price_usd: &str,
    pay_to: &str,
    endpoint: &str,
) -> Result<(PaymentRequirements, ResourceInfo), X402Error> {
    validate_address(pay_to)?;

    let atomic = price_to_atomic(price_usd)?;

    let requirements = PaymentRequirements {
        scheme: "exact".to_string(),
        network: ARC_TESTNET_CAIP2.to_string(),
        asset: ARC_TESTNET_USDC.to_string(),
        amount: atomic.to_string(),
        pay_to: pay_to.to_string(),
        max_timeout_seconds: DEFAULT_MAX_TIMEOUT_SECONDS,
        extra: GatewayExtra {
            name: GATEWAY_DOMAIN_NAME.to_string(),
            version: GATEWAY_DOMAIN_VERSION.to_string(),
            verifying_contract: ARC_TESTNET_GATEWAY_WALLET.to_string(),
        },
    };

    let resource = ResourceInfo {
        url: endpoint.to_string(),
        description: format!("Paid resource ({price_usd} USDC)"),
        mime_type: "application/json".to_string(),
    };

    Ok((requirements, resource))
}

/// Builds `PaymentRequirements` for a one-time buy-access payment.
///
/// Price is given in atomic USDC units (1 USDC = 1_000_000 atomic).
/// Uses Arc testnet defaults for network, asset, and gateway wallet.
pub fn build_buy_requirements(
    price_atomic: u64,
    pay_to: &str,
    resource_url: &str,
) -> Result<(PaymentRequirements, ResourceInfo), X402Error> {
    validate_address(pay_to)?;
    let requirements = PaymentRequirements {
        scheme: "exact".to_string(),
        network: ARC_TESTNET_CAIP2.to_string(),
        asset: ARC_TESTNET_USDC.to_string(),
        amount: price_atomic.to_string(),
        pay_to: pay_to.to_string(),
        max_timeout_seconds: DEFAULT_MAX_TIMEOUT_SECONDS,
        extra: GatewayExtra {
            name: GATEWAY_DOMAIN_NAME.to_string(),
            version: GATEWAY_DOMAIN_VERSION.to_string(),
            verifying_contract: ARC_TESTNET_GATEWAY_WALLET.to_string(),
        },
    };
    let resource = ResourceInfo {
        url: resource_url.to_string(),
        description: "Buy access".to_string(),
        mime_type: "video/*".to_string(),
    };
    Ok((requirements, resource))
}

/// Builds `PaymentRequirements` for a single streaming chunk payment.
///
/// Price is given in atomic USDC units. Uses Arc testnet defaults.
pub fn build_chunk_requirements(
    chunk_price_atomic: u64,
    pay_to: &str,
    resource_url: &str,
) -> Result<(PaymentRequirements, ResourceInfo), X402Error> {
    validate_address(pay_to)?;
    let requirements = PaymentRequirements {
        scheme: "exact".to_string(),
        network: ARC_TESTNET_CAIP2.to_string(),
        asset: ARC_TESTNET_USDC.to_string(),
        amount: chunk_price_atomic.to_string(),
        pay_to: pay_to.to_string(),
        max_timeout_seconds: DEFAULT_MAX_TIMEOUT_SECONDS,
        extra: GatewayExtra {
            name: GATEWAY_DOMAIN_NAME.to_string(),
            version: GATEWAY_DOMAIN_VERSION.to_string(),
            verifying_contract: ARC_TESTNET_GATEWAY_WALLET.to_string(),
        },
    };
    let resource = ResourceInfo {
        url: resource_url.to_string(),
        description: "Stream chunk".to_string(),
        mime_type: "video/*".to_string(),
    };
    Ok((requirements, resource))
}

/// Core payment handling logic, framework-agnostic.
///
/// - If `payment_sig_header` is `None`: returns `Err(X402Error::PaymentRequired(...))`.
///   The caller should use `build_payment_required_header` to construct the 402 response.
/// - If `payment_sig_header` is `Some(...)`: validates the payload, settles with Gateway,
///   and returns `Ok(SettleResult)` on success.
pub async fn handle_payment(
    payment_sig_header: Option<&str>,
    requirements: &PaymentRequirements,
    resource: &ResourceInfo,
    gateway: &GatewayApiClient,
) -> Result<SettleResult, X402Error> {
    let header = match payment_sig_header {
        None => {
            let payment_required = PaymentRequired {
                x402_version: X402_VERSION,
                resource: resource.clone(),
                accepts: vec![requirements.clone()],
            };
            return Err(X402Error::PaymentRequired(payment_required));
        }
        Some(h) => h,
    };

    // Validate signature format and structural correctness
    let payload = validate_payment_signature(header)?;

    // Validate payload compatibility with our requirements
    validate_payload_vs_requirements(&payload, requirements)?;

    // Settle with Circle Gateway
    let result = gateway.settle(&payload, requirements).await?;

    if !result.success {
        return Err(X402Error::SettlementFailed(
            result.error_reason.unwrap_or_else(|| "unknown error".to_string()),
        ));
    }

    Ok(result)
}

// ─── Axum middleware ──────────────────────────────────────────────────────────

#[cfg(feature = "axum-middleware")]
pub use axum_middleware::*;

#[cfg(feature = "axum-middleware")]
mod axum_middleware {
    use std::sync::Arc;

    use axum::{
        extract::Request,
        http::{HeaderValue, StatusCode},
        middleware::Next,
        response::{IntoResponse, Response},
        Extension,
    };

    use super::{build_payment_required_header, build_payment_response_header,
                build_requirements, handle_payment};
    use crate::{
        error::X402Error,
        gateway::GatewayApiClient,
        types::{PaymentRequirements, ResourceInfo},
    };

    /// Identifies the payer address, injected into request extensions after successful payment.
    #[derive(Clone, Debug)]
    pub struct PayerAddress(pub String);

    /// The paid amount in formatted USDC (e.g. "0.001"), injected after successful payment.
    #[derive(Clone, Debug)]
    pub struct PaymentAmount(pub String);

    /// Shared state passed through middleware via axum extensions.
    #[derive(Clone)]
    pub struct PaymentGateway {
        pub requirements: PaymentRequirements,
        pub resource: ResourceInfo,
        pub gateway: Arc<GatewayApiClient>,
    }

    /// axum middleware function — use with `axum::middleware::from_fn_with_state`.
    ///
    /// On payment absent/invalid: short-circuits with 402.
    /// On payment valid: injects `Extension<PayerAddress>` and `Extension<PaymentAmount>`
    /// into the request, then calls the next handler.
    pub async fn payment_middleware(
        Extension(state): Extension<PaymentGateway>,
        mut req: Request,
        next: Next,
    ) -> Response {
        let sig_header = req
            .headers()
            .get("payment-signature")
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned);

        match handle_payment(
            sig_header.as_deref(),
            &state.requirements,
            &state.resource,
            &state.gateway,
        )
        .await
        {
            Ok(settle) => {
                // Inject payment info into request extensions
                let payer = settle.payer.clone().unwrap_or_default();
                let amount_atomic: u64 = state.requirements.amount.parse().unwrap_or(0);
                let amount_usdc = format!("{:.6}", amount_atomic as f64 / 1_000_000.0);

                req.extensions_mut().insert(PayerAddress(payer));
                req.extensions_mut().insert(PaymentAmount(amount_usdc));

                let mut response = next.run(req).await;

                // Attach PAYMENT-RESPONSE header to the successful response
                if let Ok(header_value) = build_payment_response_header(&settle) {
                    if let Ok(hv) = HeaderValue::from_str(&header_value) {
                        response.headers_mut().insert("payment-response", hv);
                    }
                }

                response
            }
            Err(X402Error::PaymentRequired(_)) => {
                // Build 402 response with PAYMENT-REQUIRED header
                let header_value = match build_payment_required_header(
                    &state.requirements,
                    &state.resource,
                ) {
                    Ok(v) => v,
                    Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
                };

                let mut response = (StatusCode::PAYMENT_REQUIRED, "Payment Required").into_response();
                if let Ok(hv) = HeaderValue::from_str(&header_value) {
                    response.headers_mut().insert("payment-required", hv);
                }
                response
            }
            Err(X402Error::InvalidSignature(msg)) | Err(X402Error::Validation(msg)) => {
                let body = serde_json::json!({ "error": msg });
                (StatusCode::PAYMENT_REQUIRED, axum::Json(body)).into_response()
            }
            Err(X402Error::SettlementFailed(msg)) => {
                let body = serde_json::json!({ "error": format!("settlement failed: {msg}") });
                (StatusCode::PAYMENT_REQUIRED, axum::Json(body)).into_response()
            }
            Err(e) => {
                let body = serde_json::json!({ "error": e.to_string() });
                (StatusCode::INTERNAL_SERVER_ERROR, axum::Json(body)).into_response()
            }
        }
    }

    /// Convenience function: builds a `PaymentGateway` extension value from a price string.
    ///
    /// Usage:
    /// ```ignore
    /// let gw = require_payment("$0.001", &seller_address, "/api/premium/quote", gateway.clone())?;
    /// let app = Router::new()
    ///     .route("/api/premium/quote", get(handler))
    ///     .layer(axum::middleware::from_fn_with_state(gw, payment_middleware));
    /// ```
    pub fn require_payment(
        price_usd: &str,
        pay_to: &str,
        endpoint: &str,
        gateway: Arc<GatewayApiClient>,
    ) -> Result<PaymentGateway, X402Error> {
        let (requirements, resource) = build_requirements(price_usd, pay_to, endpoint)?;
        Ok(PaymentGateway {
            requirements,
            resource,
            gateway,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_requirements() {
        let (req, resource) =
            build_requirements("$0.001", "0xd8dA6BF26964aF9D7eEd9e03E53415D37aA96045", "/api/test")
                .unwrap();

        assert_eq!(req.amount, "1000");
        assert_eq!(req.scheme, "exact");
        assert_eq!(req.network, ARC_TESTNET_CAIP2);
        assert_eq!(resource.url, "/api/test");
    }

    #[test]
    fn test_build_payment_required_header_roundtrip() {
        let (req, resource) =
            build_requirements("$0.01", "0xd8dA6BF26964aF9D7eEd9e03E53415D37aA96045", "/api/data")
                .unwrap();

        let header = build_payment_required_header(&req, &resource).unwrap();
        assert!(!header.is_empty());

        // Decode and verify
        let bytes = base64::engine::general_purpose::STANDARD.decode(&header).unwrap();
        let parsed: PaymentRequired = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(parsed.x402_version, X402_VERSION);
        assert_eq!(parsed.accepts[0].amount, "10000");
    }
}
