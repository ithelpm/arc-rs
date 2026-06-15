mod access;
mod config;
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

// ─── App state ────────────────────────────────────────────────────────────────

#[derive(Clone)]
struct AppState {
    cfg: Arc<Config>,
    access: Arc<AccessManager>,
    streaming: Arc<StreamingManager>,
    gateway: Arc<GatewayApiClient>,
}

// ─── Request / response types ─────────────────────────────────────────────────

#[derive(Deserialize)]
struct CreateSessionRequest {
    wallet: String,
    item_id: String,
}

#[derive(Serialize)]
struct CreateSessionResponse {
    session_id: String,
    chunk_price_atomic: u64,
    pay_to: String,
}

#[derive(Deserialize)]
struct ContentQuery {
    session_id: Option<String>,
    wallet: Option<String>,
}

/// Demo content returned after a successful one-time purchase.
#[derive(Serialize)]
struct BuyContent {
    item_id: String,
    access: &'static str,
    content: String,
}

/// Demo content returned per paid chunk in a metered session.
#[derive(Serialize)]
struct ChunkContent {
    item_id: String,
    session_id: String,
    chunk: u64,
    content: String,
}

// ─── Handlers ─────────────────────────────────────────────────────────────────

/// POST /api/session — create a per-chunk billing session.
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

    let _ = s.access.log_streaming_session(
        &session_id, &req.wallet, &content_hex,
        s.cfg.chunk_price_atomic, 1,
    ).await;

    Json(CreateSessionResponse {
        session_id,
        chunk_price_atomic: s.cfg.chunk_price_atomic,
        pay_to: s.cfg.seller_address.clone(),
    })
}

/// GET /content/{item_id}[?session_id=...][&wallet=...]
///
/// Two modes:
///   - `session_id` present → per-chunk metered access (pay per request)
///   - `session_id` absent  → buy-to-access (one payment → permanent soulbound record)
async fn get_content(
    State(s): State<AppState>,
    Path(item_id): Path<String>,
    Query(q): Query<ContentQuery>,
    headers: HeaderMap,
    _req: Request<Body>,
) -> Response {
    if let Some(session_id) = q.session_id {
        chunk_handler(s, item_id, session_id, headers).await
    } else {
        buy_handler(s, item_id, q.wallet, headers).await
    }
}

/// Per-chunk metered access: each call requires a fresh payment-signature.
async fn chunk_handler(
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
    let chunk_num = session_guard.next_chunk();
    drop(session_guard);

    let resource = arc_x402::types::ResourceInfo {
        url: format!("/content/{}", item_id),
        description: "Metered content chunk".to_string(),
        mime_type: "application/json".to_string(),
    };

    let sig_header = headers.get("payment-signature").and_then(|v| v.to_str().ok());

    match handle_payment(sig_header, &chunk_requirements, &resource, &s.gateway).await {
        Ok(settled) => {
            tracing::info!(session = %session_id, chunk = chunk_num, "chunk payment settled");
            let pay_resp = build_payment_response_header(&settled).unwrap_or_default();

            let body = Json(ChunkContent {
                item_id: item_id.clone(),
                session_id: session_id.clone(),
                chunk: chunk_num,
                content: format!(
                    "Chunk {} of '{}'. Metered content delivered via per-chunk x402 billing. \
                     Payment settled through Circle Gateway on Arc testnet.",
                    chunk_num, item_id
                ),
            });

            let mut response = body.into_response();
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

/// Buy-to-access: one payment grants permanent access; subsequent requests use fast-path.
async fn buy_handler(
    s: AppState,
    item_id: String,
    wallet_param: Option<String>,
    headers: HeaderMap,
) -> Response {
    let wallet = headers
        .get("x-wallet-address")
        .and_then(|v| v.to_str().ok())
        .map(String::from)
        .or(wallet_param);

    if let Some(ref w) = wallet {
        if Address::from_str(w).is_err() {
            return (StatusCode::BAD_REQUEST, "invalid wallet address format").into_response();
        }
    }

    let content_id = item_id_to_content_id(&item_id);
    let resource_url = format!("/content/{}", item_id);

    // Fast path: SQLite cache hit or on-chain hasAccess == true
    if let Some(ref w) = wallet {
        if let Ok(true) = s.access.check_access(w, content_id).await {
            tracing::info!(wallet = %w, item = %item_id, "fast-path: access confirmed, no payment");
            return Json(BuyContent {
                item_id: item_id.clone(),
                access: "permanent",
                content: format!(
                    "Full premium content for '{}'. \
                     Access verified via fast-path (SQLite cache / on-chain hasAccess). \
                     No payment taken.",
                    item_id
                ),
            }).into_response();
        }
    }

    let (buy_requirements, resource) =
        match build_buy_requirements(s.cfg.buy_price_atomic, &s.cfg.seller_address, &resource_url) {
            Ok(r) => r,
            Err(e) => {
                tracing::error!("build_buy_requirements failed: {e}");
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            }
        };

    let sig_header = headers.get("payment-signature").and_then(|v| v.to_str().ok());

    match handle_payment(sig_header, &buy_requirements, &resource, &s.gateway).await {
        Ok(settled) => {
            let payer = match settled.payer.clone().or_else(|| wallet.clone()).filter(|p| !p.is_empty()) {
                Some(p) => p,
                None => {
                    tracing::error!(item = %item_id, "payer unknown after settlement");
                    return StatusCode::INTERNAL_SERVER_ERROR.into_response();
                }
            };

            tracing::info!(payer = %payer, item = %item_id, "buy payment settled");

            // Fire-and-forget: record soulbound access on-chain without blocking response
            let access_clone = Arc::clone(&s.access);
            let payer_clone = payer.clone();
            tokio::spawn(async move {
                match access_clone.grant_and_record(&payer_clone, content_id).await {
                    Ok(tx) => tracing::info!(payer = %payer_clone, tx = %tx, "grantAccess confirmed"),
                    Err(e) => tracing::error!("grantAccess background task failed: {e}"),
                }
            });

            let pay_resp = build_payment_response_header(&settled).unwrap_or_default();
            let body = Json(BuyContent {
                item_id: item_id.clone(),
                access: "permanent",
                content: format!(
                    "Full premium content for '{}'. \
                     Payment settled. Permanent soulbound access is being recorded \
                     on Arc testnet (grantAccess submitted in background).",
                    item_id
                ),
            });

            let mut response = body.into_response();
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

// ─── Bootstrap ────────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("access_gated_server=info".parse().unwrap()),
        )
        .init();

    let cfg = Arc::new(Config::from_env()?);

    let contract_address = Address::from_str(&cfg.media_access_contract)
        .map_err(|e| anyhow::anyhow!("invalid MEDIA_ACCESS_CONTRACT: {e}"))?;

    let chain_client = MediaAccessClient::new(&cfg.arc_rpc_url, contract_address)
        .with_signer(&cfg.seller_private_key);

    let access = Arc::new(AccessManager::new(&cfg.database_url, chain_client).await?);
    let streaming = Arc::new(StreamingManager::new());
    let gateway = Arc::new(GatewayApiClient::testnet());

    let state = AppState {
        cfg: cfg.clone(),
        access,
        streaming: Arc::clone(&streaming),
        gateway,
    };

    // Background GC: evict sessions older than 24 hours
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(3600));
        loop {
            interval.tick().await;
            streaming.cleanup_expired(86400);
            tracing::debug!("session GC complete");
        }
    });

    let app = Router::new()
        .route("/api/session", post(create_session))
        .route("/content/{item_id}", get(get_content))
        .with_state(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], cfg.port));
    tracing::info!("access-gated-server listening on {addr}");
    tracing::info!(
        buy_price = cfg.buy_price_atomic,
        chunk_price = cfg.chunk_price_atomic,
        "payment config"
    );

    axum::serve(tokio::net::TcpListener::bind(addr).await?, app).await?;
    Ok(())
}
