mod access;
mod config;
mod html;
mod streaming;

use std::{collections::HashMap, net::SocketAddr, str::FromStr, sync::Arc, time::Duration};

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
    extract::{Path, Query, Request, State},
    http::{HeaderMap, StatusCode},
    response::{Html, IntoResponse, Json, Response},
    routing::{get, post},
    Router,
};
use media_access::{item_id_to_content_id, MediaAccessClient};
use serde::{Deserialize, Serialize};

use access::{AccessManager, ItemRow};
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

#[derive(Serialize)]
struct BuyContent {
    item_id: String,
    access: &'static str,
    content: String,
}

#[derive(Serialize)]
struct ChunkContent {
    item_id: String,
    session_id: String,
    chunk: u64,
    content: String,
}

// ─── HTML page handlers ───────────────────────────────────────────────────────

async fn serve_storefront() -> impl IntoResponse {
    Html(html::storefront())
}

async fn serve_stats_page() -> impl IntoResponse {
    Html(html::stats_page())
}

// ─── Public API handlers ──────────────────────────────────────────────────────

async fn list_items(State(s): State<AppState>) -> impl IntoResponse {
    match s.access.get_items().await {
        Ok(items) => Json(items).into_response(),
        Err(e) => {
            tracing::error!("get_items failed: {e}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

async fn api_stats(State(s): State<AppState>) -> impl IntoResponse {
    match s.access.get_stats().await {
        Ok(stats) => Json(stats).into_response(),
        Err(e) => {
            tracing::error!("get_stats failed: {e}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

// ─── Content handlers ─────────────────────────────────────────────────────────

async fn create_session(
    State(s): State<AppState>,
    Json(req): Json<CreateSessionRequest>,
) -> impl IntoResponse {
    let content_id = item_id_to_content_id(&req.item_id);
    let content_hex = format!("0x{}", hex::encode(content_id.as_slice()));

    let chunk_price = match s.access.get_item(&req.item_id).await {
        Ok(Some(item)) => item.chunk_price_atomic as u64,
        _ => s.cfg.chunk_price_atomic,
    };

    let session_id = s.streaming.create_session(
        req.wallet.clone(),
        content_hex.clone(),
        &req.item_id,
        chunk_price,
        &s.cfg.seller_address,
    );

    let _ = s
        .access
        .log_streaming_session(&session_id, &req.wallet, &content_hex, chunk_price, 1)
        .await;

    Json(CreateSessionResponse {
        session_id,
        chunk_price_atomic: chunk_price,
        pay_to: s.cfg.seller_address.clone(),
    })
}

async fn get_content(
    State(s): State<AppState>,
    Path(item_id): Path<String>,
    Query(q): Query<ContentQuery>,
    headers: HeaderMap,
    _req: Request,
) -> Response {
    if let Some(session_id) = q.session_id {
        chunk_handler(s, item_id, session_id, headers).await
    } else {
        buy_handler(s, item_id, q.wallet, headers).await
    }
}

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

    let sig_header = headers
        .get("payment-signature")
        .and_then(|v| v.to_str().ok());

    match handle_payment(sig_header, &chunk_requirements, &resource, &s.gateway).await {
        Ok(settled) => {
            tracing::info!(session = %session_id, chunk = chunk_num, "chunk payment settled");
            let pay_resp = build_payment_response_header(&settled).unwrap_or_default();

            if let Some(ref payer) = settled.payer {
                let chunk_price: i64 =
                    chunk_requirements.amount.parse().unwrap_or(0);
                let _ = s
                    .access
                    .log_payment(
                        &item_id,
                        &s.cfg.seller_address,
                        payer,
                        chunk_price,
                        "chunk",
                        settled.transaction.as_deref(),
                    )
                    .await;
            }

            let (title, desc) = match s.access.get_item(&item_id).await {
                Ok(Some(item)) => (item.title, item.description),
                _ => (item_id.clone(), String::new()),
            };

            let body = Json(ChunkContent {
                item_id: item_id.clone(),
                session_id: session_id.clone(),
                chunk: chunk_num,
                content: format!(
                    "[{title}] Chunk {chunk_num}. {desc} \
                     Metered delivery via per-chunk x402 billing, \
                     settled through Circle Gateway on Arc testnet.",
                ),
            });

            let mut response = body.into_response();
            if let Ok(hv) = pay_resp.parse() {
                response.headers_mut().insert("payment-response", hv);
            }
            response
        }
        Err(arc_x402::X402Error::PaymentRequired(_)) => {
            let pay_req =
                build_payment_required_header(&chunk_requirements, &resource).unwrap_or_default();
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

    if let Some(ref w) = wallet {
        if let Ok(true) = s.access.check_access(w, content_id).await {
            tracing::info!(wallet = %w, item = %item_id, "fast-path: access confirmed");

            let (title, desc) = match s.access.get_item(&item_id).await {
                Ok(Some(item)) => (item.title, item.description),
                _ => (item_id.clone(), String::new()),
            };

            return Json(BuyContent {
                item_id: item_id.clone(),
                access: "permanent",
                content: format!(
                    "[{title}] {desc} \
                     Access verified via fast-path (SQLite cache / on-chain hasAccess). \
                     No payment taken.",
                ),
            })
            .into_response();
        }
    }

    let buy_price = match s.access.get_item(&item_id).await {
        Ok(Some(item)) => item.buy_price_atomic as u64,
        _ => s.cfg.buy_price_atomic,
    };

    let (buy_requirements, resource) =
        match build_buy_requirements(buy_price, &s.cfg.seller_address, &resource_url) {
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
            let payer = match settled
                .payer
                .clone()
                .or_else(|| wallet.clone())
                .filter(|p| !p.is_empty())
            {
                Some(p) => p,
                None => {
                    tracing::error!(item = %item_id, "payer unknown after settlement");
                    return StatusCode::INTERNAL_SERVER_ERROR.into_response();
                }
            };

            tracing::info!(payer = %payer, item = %item_id, "buy payment settled");

            let _ = s
                .access
                .log_payment(
                    &item_id,
                    &s.cfg.seller_address,
                    &payer,
                    buy_price as i64,
                    "buy",
                    settled.transaction.as_deref(),
                )
                .await;

            let access_clone = Arc::clone(&s.access);
            let payer_clone = payer.clone();
            tokio::spawn(async move {
                match access_clone.grant_and_record(&payer_clone, content_id).await {
                    Ok(tx) => tracing::info!(payer = %payer_clone, tx = %tx, "grantAccess confirmed"),
                    Err(e) => tracing::error!("grantAccess background task failed: {e}"),
                }
            });

            let pay_resp = build_payment_response_header(&settled).unwrap_or_default();

            let (title, desc) = match s.access.get_item(&item_id).await {
                Ok(Some(item)) => (item.title, item.description),
                _ => (item_id.clone(), String::new()),
            };

            let body = Json(BuyContent {
                item_id: item_id.clone(),
                access: "permanent",
                content: format!(
                    "[{title}] {desc} \
                     Payment settled. Permanent soulbound access is being recorded \
                     on Arc testnet (grantAccess submitted in background).",
                ),
            });

            let mut response = body.into_response();
            if let Ok(hv) = pay_resp.parse() {
                response.headers_mut().insert("payment-response", hv);
            }
            response
        }
        Err(arc_x402::X402Error::PaymentRequired(_)) => {
            let pay_req =
                build_payment_required_header(&buy_requirements, &resource).unwrap_or_default();
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

// ─── items.json seed ──────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct SeedItem {
    title: String,
    #[serde(default)]
    description: String,
    buy_price_atomic: Option<u64>,
    chunk_price_atomic: Option<u64>,
}

async fn seed_from_json(
    access: &AccessManager,
    path: &str,
    seller: &str,
    default_buy: u64,
    default_chunk: u64,
) {
    let Ok(data) = std::fs::read_to_string(path) else {
        tracing::info!("{path} not found — no items seeded");
        return;
    };
    let Ok(map) = serde_json::from_str::<HashMap<String, SeedItem>>(&data) else {
        tracing::warn!("{path} parse failed — skipping seed");
        return;
    };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    for (item_id, item) in map {
        let row = ItemRow {
            item_id: item_id.clone(),
            seller: seller.to_string(),
            title: item.title,
            description: item.description,
            buy_price_atomic: item.buy_price_atomic.unwrap_or(default_buy) as i64,
            chunk_price_atomic: item.chunk_price_atomic.unwrap_or(default_chunk) as i64,
            created_at: now,
        };
        match access.upsert_item(&row).await {
            Ok(()) => tracing::info!(item_id, "seeded item"),
            Err(e) => tracing::warn!(item_id, "seed failed: {e}"),
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

    // Seed items from items.json on first startup
    let existing = access.get_items().await.unwrap_or_default();
    if existing.is_empty() {
        seed_from_json(
            &access,
            &cfg.items_json_path,
            &cfg.seller_address,
            cfg.buy_price_atomic,
            cfg.chunk_price_atomic,
        )
        .await;
    }

    let state = AppState {
        cfg: cfg.clone(),
        access,
        streaming: Arc::clone(&streaming),
        gateway,
    };

    // Background session GC (hourly, expire after 24h)
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(3600));
        loop {
            interval.tick().await;
            streaming.cleanup_expired(86400);
            tracing::debug!("session GC complete");
        }
    });

    let app = Router::new()
        .route("/", get(serve_storefront))
        .route("/stats", get(serve_stats_page))
        .route("/items", get(list_items))
        .route("/api/stats", get(api_stats))
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
