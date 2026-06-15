use std::sync::Arc;

use axum::{
    extract::Extension,
    middleware,
    response::Json,
    routing::{get, post},
    Router,
};
use serde_json::{json, Value};
use tracing::info;
use tracing_subscriber::EnvFilter;

use arc_x402::{
    payment_middleware, require_payment, GatewayApiClient, PayerAddress, PaymentAmount,
};

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("seller=info".parse().unwrap()))
        .init();

    let seller_address = std::env::var("SELLER_ADDRESS")
        .expect("SELLER_ADDRESS environment variable is required");
    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(3000);

    let gateway = Arc::new(GatewayApiClient::testnet());

    // Build payment gates for each route
    let quote_gate = require_payment("$0.001", &seller_address, "/api/premium/quote", gateway.clone())
        .expect("failed to build quote payment gate");
    let dataset_gate = require_payment("$0.01", &seller_address, "/api/premium/dataset", gateway.clone())
        .expect("failed to build dataset payment gate");
    let compute_gate = require_payment("$0.0003", &seller_address, "/api/premium/compute", gateway.clone())
        .expect("failed to build compute payment gate");
    let task_gate = require_payment("$0.03", &seller_address, "/api/premium/agent-task", gateway.clone())
        .expect("failed to build agent-task payment gate");

    let app = Router::new()
        .route(
            "/api/premium/quote",
            get(quote_handler)
                .layer(Extension(quote_gate))
                .layer(middleware::from_fn(payment_middleware)),
        )
        .route(
            "/api/premium/dataset",
            get(dataset_handler)
                .layer(Extension(dataset_gate))
                .layer(middleware::from_fn(payment_middleware)),
        )
        .route(
            "/api/premium/compute",
            post(compute_handler)
                .layer(Extension(compute_gate))
                .layer(middleware::from_fn(payment_middleware)),
        )
        .route(
            "/api/premium/agent-task",
            get(task_handler)
                .layer(Extension(task_gate))
                .layer(middleware::from_fn(payment_middleware)),
        );

    let addr = format!("0.0.0.0:{port}");
    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();

    info!("──────────────────────────────────────────────");
    info!("  arc-x402 Seller Server");
    info!("  Listening on http://{addr}");
    info!("  Seller: {seller_address}");
    info!("──────────────────────────────────────────────");
    info!("  GET  /api/premium/quote       $0.001 USDC");
    info!("  GET  /api/premium/dataset     $0.01  USDC");
    info!("  POST /api/premium/compute     $0.0003 USDC");
    info!("  GET  /api/premium/agent-task  $0.03  USDC");
    info!("──────────────────────────────────────────────");

    axum::serve(listener, app).await.unwrap();
}

async fn quote_handler(
    Extension(payer): Extension<PayerAddress>,
    Extension(amount): Extension<PaymentAmount>,
) -> Json<Value> {
    info!("quote paid by {} ({}  USDC)", payer.0, amount.0);
    Json(json!({
        "quote": "The best way to predict the future is to invent it.",
        "author": "Alan Kay",
        "category": "technology"
    }))
}

async fn dataset_handler(
    Extension(payer): Extension<PayerAddress>,
    Extension(amount): Extension<PaymentAmount>,
) -> Json<Value> {
    info!("dataset paid by {} ({} USDC)", payer.0, amount.0);
    Json(json!({
        "dataset": [
            { "id": 1, "metric": "daily_active_users", "value": 14200, "unit": "users" },
            { "id": 2, "metric": "revenue_usd",        "value": 8450,  "unit": "USD"   },
            { "id": 3, "metric": "api_calls",          "value": 92100, "unit": "calls" },
            { "id": 4, "metric": "avg_latency_ms",     "value": 42,    "unit": "ms"    }
        ]
    }))
}

async fn compute_handler(
    Extension(payer): Extension<PayerAddress>,
    Extension(amount): Extension<PaymentAmount>,
    body: axum::body::Bytes,
) -> Json<Value> {
    info!("compute paid by {} ({} USDC)", payer.0, amount.0);

    let text = String::from_utf8_lossy(&body).into_owned();

    // Simple text analysis
    let text_to_analyze = if text.is_empty() {
        let parsed: serde_json::Value = serde_json::from_str(&text).unwrap_or_default();
        parsed
            .get("text")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string()
    } else {
        text
    };

    let words: Vec<&str> = text_to_analyze.split_whitespace().collect();
    let word_count = words.len();
    let char_count = text_to_analyze.chars().count();
    let sentence_count = text_to_analyze.chars().filter(|&c| c == '.' || c == '!' || c == '?').count().max(1);

    Json(json!({
        "word_count": word_count,
        "char_count": char_count,
        "sentence_count": sentence_count,
        "summary": format!("Input contains {word_count} words across {sentence_count} sentence(s).")
    }))
}

async fn task_handler(
    Extension(payer): Extension<PayerAddress>,
    Extension(amount): Extension<PaymentAmount>,
) -> Json<Value> {
    info!("agent-task paid by {} ({} USDC)", payer.0, amount.0);
    Json(json!({
        "clue": "The treasure is hidden where the sun meets the ocean at the edge of the Arc.",
        "step": 1,
        "total_steps": 5,
        "hint": "Look for the gateway where USDC flows without gas."
    }))
}
