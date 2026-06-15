mod access;
mod config;
mod proxy;
mod streaming;

use std::{net::SocketAddr, str::FromStr, sync::Arc, time::Duration};

use alloy_primitives::Address;
use arc_x402::{
    server::{
        build_buy_requirements, build_payment_required_header, build_payment_response_header,
        handle_payment,
    },
    GatewayApiClient,
};
use axum::{
    body::Body,
    extract::{Path, Query, State},
    http::{HeaderMap, Request, StatusCode},
    response::{IntoResponse, Json, Response},
    routing::{get, post},
    Router,
};
use media_access::{item_id_to_content_id, MediaAccessClient};
use serde::{Deserialize, Serialize};

use access::AccessManager;
use config::Config;
use streaming::StreamingManager;

// ---------------------------------------------------------------------------
// App state
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct AppState {
    cfg: Arc<Config>,
    access: Arc<AccessManager>,
    streaming: Arc<StreamingManager>,
    gateway: Arc<GatewayApiClient>,
    http: reqwest::Client,
}

// ---------------------------------------------------------------------------
// Request / response types
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct CreateSessionRequest {
    wallet: String,
    item_id: String,
}

#[derive(Serialize)]
struct CreateSessionResponse {
    session_id: String,
    chunk_price_atomic: u64,
    chunk_secs: u64,
    rate_per_sec_atomic: u64,
    /// Seller address — returned so the buyer CLI can reconstruct chunk PaymentRequirements.
    pay_to: String,
}

#[derive(Deserialize)]
struct StreamQuery {
    session_id: Option<String>,
    wallet: Option<String>,
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// POST /api/stream/session
/// Creates a per-second billing session for a given Jellyfin item.
async fn create_session(
    State(s): State<AppState>,
    Json(req): Json<CreateSessionRequest>,
) -> impl IntoResponse {
    let content_id = item_id_to_content_id(&req.item_id);
    let content_hex = format!("0x{}", hex::encode(content_id.as_slice()));

    let session_id = s.streaming.create_session(
        req.wallet.clone(),
        content_hex.clone(),
        &req.item_id,
        &s.cfg,
    );

    let _ = s
        .access
        .log_streaming_session(
            &session_id,
            &req.wallet,
            &content_hex,
            s.cfg.stream_rate_per_sec_atomic,
            s.cfg.stream_chunk_secs,
        )
        .await;

    Json(CreateSessionResponse {
        session_id,
        chunk_price_atomic: s.cfg.chunk_price_atomic(),
        chunk_secs: s.cfg.stream_chunk_secs,
        rate_per_sec_atomic: s.cfg.stream_rate_per_sec_atomic,
        pay_to: s.cfg.seller_address.clone(),
    })
}

/// GET /Videos/{item_id}/stream?[session_id=...][&wallet=...]
///
/// Two modes:
///   - `session_id` present → per-second streaming billing (chunk payment required)
///   - `session_id` absent  → buy-to-access (one payment grants permanent on-chain access)
async fn video_stream(
    State(s): State<AppState>,
    Path(item_id): Path<String>,
    Query(q): Query<StreamQuery>,
    req: Request<Body>,
) -> Response {
    let headers = req.headers().clone();

    if let Some(session_id) = q.session_id {
        stream_chunk_handler(s, item_id, session_id, headers).await
    } else {
        buy_access_handler(s, item_id, q.wallet, headers, req).await
    }
}

/// Per-second billing: the client provides a signed chunk auth in `payment-signature`.
/// After settlement the video streams for `chunk_secs` then closes.
async fn stream_chunk_handler(
    s: AppState,
    item_id: String,
    session_id: String,
    headers: HeaderMap,
) -> Response {
    let session_guard = match s.streaming.sessions.get(&session_id) {
        Some(g) => g,
        None => return (StatusCode::NOT_FOUND, "session not found").into_response(),
    };

    let chunk_requirements = session_guard.chunk_requirements.clone();
    let wallet = session_guard.wallet.clone();
    drop(session_guard);

    let resource = arc_x402::types::ResourceInfo {
        url: format!("/Videos/{}/stream", item_id),
        description: "Stream chunk".to_string(),
        mime_type: "video/*".to_string(),
    };

    let sig_header = headers
        .get("payment-signature")
        .and_then(|v| v.to_str().ok());

    match handle_payment(sig_header, &chunk_requirements, &resource, &s.gateway).await {
        Ok(settled) => {
            tracing::info!(session = %session_id, payer = %wallet, "chunk settled");

            let pay_resp = build_payment_response_header(&settled).unwrap_or_default();

            let mut response = proxy::proxy_stream_chunk(
                &s.http,
                &s.cfg.jellyfin_url,
                &item_id,
                &headers,
                s.cfg.stream_chunk_secs,
            )
            .await;

            if let Ok(hv) = pay_resp.parse() {
                response.headers_mut().insert("payment-response", hv);
            }

            response
        }
        Err(arc_x402::X402Error::PaymentRequired(_)) => {
            let pay_req = build_payment_required_header(&chunk_requirements, &resource)
                .unwrap_or_default();
            let mut resp = Response::builder()
                .status(402)
                .body(Body::empty())
                .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response());
            if let Ok(hv) = pay_req.parse() {
                resp.headers_mut().insert("payment-required", hv);
            }
            resp
        }
        Err(e) => {
            tracing::warn!("chunk payment error: {e}");
            (StatusCode::PAYMENT_REQUIRED, e.to_string()).into_response()
        }
    }
}

/// Buy-to-access: one x402 payment → on-chain `grantAccess` → stream full video.
/// On subsequent requests with the same wallet, the fast-path cache check bypasses payment.
async fn buy_access_handler(
    s: AppState,
    item_id: String,
    wallet_param: Option<String>,
    headers: HeaderMap,
    req: Request<Body>,
) -> Response {
    // Prefer X-Wallet-Address header, fall back to query param
    let wallet = headers
        .get("x-wallet-address")
        .and_then(|v| v.to_str().ok())
        .map(String::from)
        .or(wallet_param);

    // Validate wallet address format early if provided
    if let Some(ref w) = wallet {
        if Address::from_str(w).is_err() {
            tracing::warn!("invalid wallet address in request: {w}");
            return (StatusCode::BAD_REQUEST, "invalid wallet address format").into_response();
        }
    }

    let content_id = item_id_to_content_id(&item_id);
    let resource_url = format!("/Videos/{}/stream", item_id);

    // Fast path: wallet already has on-chain (or cached) access
    if let Some(ref w) = wallet {
        if let Ok(true) = s.access.check_access(w, content_id).await {
            tracing::debug!(wallet = %w, item = %item_id, "access cache hit — bypassing payment");
            return proxy::proxy_to_jellyfin(&s.http, &s.cfg.jellyfin_url, req).await;
        }
    }

    let (buy_requirements, resource) =
        match build_buy_requirements(s.cfg.buy_price_atomic, &s.cfg.seller_address, &resource_url)
        {
            Ok(r) => r,
            Err(e) => {
                tracing::error!("build_buy_requirements failed: {e}");
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            }
        };

    let sig_header = headers
        .get("payment-signature")
        .and_then(|v| v.to_str().ok());

    match handle_payment(sig_header, &buy_requirements, &resource, &s.gateway).await {
        Ok(settled) => {
            // Resolve payer identity; if unknown after settlement, stream without grant
            let payer = match settled
                .payer
                .clone()
                .or_else(|| wallet.clone())
                .filter(|p| !p.is_empty())
            {
                Some(p) => p,
                None => {
                    tracing::error!(
                        item = %item_id,
                        "payer identity unknown after settlement — streaming without on-chain grant"
                    );
                    let pay_resp = build_payment_response_header(&settled).unwrap_or_default();
                    let mut response =
                        proxy::proxy_to_jellyfin(&s.http, &s.cfg.jellyfin_url, req).await;
                    if let Ok(hv) = pay_resp.parse() {
                        response.headers_mut().insert("payment-response", hv);
                    }
                    return response;
                }
            };

            tracing::info!(payer = %payer, item = %item_id, "buy payment settled");

            // Fire-and-forget: grant on-chain access without blocking the HTTP response
            let access_clone = Arc::clone(&s.access);
            let payer_clone = payer.clone();
            tokio::spawn(async move {
                match access_clone.grant_and_record(&payer_clone, content_id).await {
                    Ok(tx) => {
                        tracing::info!(payer = %payer_clone, tx = %tx, "grantAccess confirmed")
                    }
                    Err(e) => tracing::error!("grantAccess background task failed: {e}"),
                }
            });

            let pay_resp = build_payment_response_header(&settled).unwrap_or_default();
            let mut response =
                proxy::proxy_to_jellyfin(&s.http, &s.cfg.jellyfin_url, req).await;
            if let Ok(hv) = pay_resp.parse() {
                response.headers_mut().insert("payment-response", hv);
            }
            response
        }
        Err(arc_x402::X402Error::PaymentRequired(_)) => {
            let pay_req = build_payment_required_header(&buy_requirements, &resource)
                .unwrap_or_default();
            let mut resp = Response::builder()
                .status(402)
                .body(Body::empty())
                .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response());
            if let Ok(hv) = pay_req.parse() {
                resp.headers_mut().insert("payment-required", hv);
            }
            resp
        }
        Err(e) => {
            tracing::warn!("buy payment error: {e}");
            (StatusCode::PAYMENT_REQUIRED, e.to_string()).into_response()
        }
    }
}

/// Catch-all: passes every other request through to Jellyfin unchanged.
async fn catch_all(State(s): State<AppState>, req: Request<Body>) -> Response {
    proxy::proxy_to_jellyfin(&s.http, &s.cfg.jellyfin_url, req).await
}

// ---------------------------------------------------------------------------
// Bootstrap
// ---------------------------------------------------------------------------

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("jellyfin_proxy=info".parse().unwrap()),
        )
        .init();

    let cfg = Arc::new(Config::from_env()?);

    let contract_address = Address::from_str(&cfg.media_access_contract)
        .map_err(|e| anyhow::anyhow!("invalid MEDIA_ACCESS_CONTRACT: {e}"))?;

    let chain_client = MediaAccessClient::new(&cfg.arc_rpc_url, contract_address)
        .with_signer(&cfg.seller_private_key);

    let access = Arc::new(AccessManager::new(&cfg.database_url, chain_client).await?);
    let gateway = Arc::new(GatewayApiClient::testnet());
    let http = reqwest::Client::builder()
        .user_agent("jellyfin-proxy/0.1.0")
        .build()?;

    let streaming = Arc::new(StreamingManager::new());

    let state = AppState {
        cfg: cfg.clone(),
        access,
        streaming: Arc::clone(&streaming),
        gateway,
        http,
    };

    // Background task: clean up streaming sessions older than 24 hours
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(3600));
        loop {
            interval.tick().await;
            streaming.cleanup_expired(86400);
            tracing::debug!("streaming session GC complete");
        }
    });

    let app = Router::new()
        .route("/api/stream/session", post(create_session))
        .route("/Videos/{item_id}/stream", get(video_stream))
        .fallback(catch_all)
        .with_state(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], cfg.port));
    tracing::info!("jellyfin-proxy listening on {addr}");
    tracing::info!("upstream Jellyfin: {}", cfg.jellyfin_url);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
