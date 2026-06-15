use std::time::Duration;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use tracing::{info, warn};
use tracing_subscriber::EnvFilter;

use arc_x402::{build_chunk_requirements, BuyerClient, GatewayApiClient};

#[derive(Parser)]
#[command(name = "buyer", about = "arc-x402 payment CLI — buy or stream Jellyfin content")]
struct Cli {
    #[command(subcommand)]
    command: Option<Cmd>,

    /// Base URL of the seller / proxy server
    #[arg(long, env = "BASE_URL", default_value = "http://localhost:3000", global = true)]
    base_url: String,
}

#[derive(Subcommand)]
enum Cmd {
    /// Cycle through paid demo endpoints until the spend limit is reached.
    Pay {
        /// Stop after spending this many USDC (e.g. 0.5)
        #[arg(long)]
        limit: Option<f64>,
    },
    /// Buy permanent access to a Jellyfin item (one-time payment).
    Buy {
        /// Jellyfin item ID
        #[arg(long)]
        item_id: String,
    },
    /// Stream a Jellyfin item using per-second chunk billing.
    Stream {
        /// Jellyfin item ID
        #[arg(long)]
        item_id: String,
        /// Number of chunks to stream
        #[arg(long, default_value_t = 1)]
        chunks: u32,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("buyer=info".parse().unwrap()))
        .init();

    let cli = Cli::parse();

    let private_key = std::env::var("BUYER_PRIVATE_KEY")
        .context("BUYER_PRIVATE_KEY environment variable is required")?;

    let client = BuyerClient::new(&private_key).context("failed to create buyer client")?;
    let gateway = GatewayApiClient::testnet();

    info!("──────────────────────────────────────────────");
    info!("  arc-x402 Buyer CLI");
    info!("  Wallet:   {}", client.address());
    info!("  Target:   {}", cli.base_url);
    info!("──────────────────────────────────────────────");

    match client.get_gateway_balance(&gateway).await {
        Ok(balance) => info!("  Gateway balance: {balance} USDC"),
        Err(e) => warn!("  Could not fetch gateway balance: {e}"),
    }
    info!("──────────────────────────────────────────────");

    match cli.command.unwrap_or(Cmd::Pay { limit: None }) {
        Cmd::Pay { limit } => cmd_pay(&client, &cli.base_url, limit).await,
        Cmd::Buy { item_id } => cmd_buy(&client, &cli.base_url, &item_id).await,
        Cmd::Stream { item_id, chunks } => cmd_stream(&client, &cli.base_url, &item_id, chunks).await,
    }
}

async fn cmd_pay(client: &BuyerClient, base_url: &str, limit: Option<f64>) -> Result<()> {
    if let Some(l) = limit {
        info!("  Limit: {l:.6} USDC");
        info!("──────────────────────────────────────────────");
    }

    let endpoints = vec![
        ("GET",  "/api/premium/quote",      None),
        ("GET",  "/api/premium/dataset",    None),
        ("POST", "/api/premium/compute",    Some(br#"{"text":"The quick brown fox jumps over the lazy dog."}"#.as_ref())),
        ("GET",  "/api/premium/agent-task", None),
    ];

    let mut request_count: u64 = 0;
    let mut total_spent: f64 = 0.0;

    loop {
        for (method, path, body) in &endpoints {
            if let Some(lim) = limit {
                if total_spent >= lim {
                    info!("Spend limit {lim:.6} USDC reached after {request_count} requests.");
                    return Ok(());
                }
            }

            request_count += 1;
            let url = format!("{base_url}{path}");

            match client.pay(&url, method, body.map(|b: &[u8]| b)).await {
                Ok(result) => {
                    let amount: f64 = result.amount_paid_usdc.parse().unwrap_or(0.0);
                    total_spent += amount;
                    info!(
                        "#{request_count} {method} {path} → {} USDC ({}ms) [total: {total_spent:.6}]",
                        result.amount_paid_usdc,
                        result.elapsed_ms
                    );
                }
                Err(e) => warn!("#{request_count} {method} {path} → ERROR: {e}"),
            }

            tokio::time::sleep(Duration::from_millis(1000)).await;
        }
    }
}

async fn cmd_buy(client: &BuyerClient, base_url: &str, item_id: &str) -> Result<()> {
    info!("Buying access to item: {item_id}");
    let result = client
        .buy_access(base_url, item_id)
        .await
        .context("buy_access failed")?;

    if let Some(pr) = &result.payment_response {
        info!("  Payment settled — tx: {}", pr.transaction);
        info!("  Payer:  {}", pr.payer);
        info!("  Amount: {} USDC ({}ms)", result.amount_paid_usdc, result.elapsed_ms);
    } else {
        info!("  Access granted (fast-path — already purchased). {}ms", result.elapsed_ms);
    }
    Ok(())
}

async fn cmd_stream(client: &BuyerClient, base_url: &str, item_id: &str, chunks: u32) -> Result<()> {
    info!("Creating streaming session for item: {item_id}");
    let session = client
        .create_stream_session(base_url, item_id)
        .await
        .context("create_stream_session failed")?;

    let rate_usdc = session.rate_per_sec_atomic as f64 / 1_000_000.0;
    info!(
        "  Session: {}  chunk={}s  rate={:.6} USDC/s",
        session.session_id, session.chunk_secs, rate_usdc
    );

    let resource_url = format!("/content/{}", item_id);
    let (requirements, resource) = build_chunk_requirements(
        session.chunk_price_atomic,
        &session.pay_to,
        &resource_url,
    )
    .context("failed to build chunk requirements")?;

    let mut total_bytes: usize = 0;
    let mut total_atomic: u64 = 0;

    for i in 1..=chunks {
        let data = client
            .stream_chunk(base_url, item_id, &session.session_id, &requirements, &resource)
            .await
            .with_context(|| format!("chunk {i} failed"))?;

        total_bytes += data.len();
        total_atomic += session.chunk_price_atomic;

        let chunk_usdc = session.chunk_price_atomic as f64 / 1_000_000.0;
        info!(
            "  Chunk {i}/{chunks} — {} bytes  paid {chunk_usdc:.6} USDC",
            data.len()
        );
    }

    let total_usdc = total_atomic as f64 / 1_000_000.0;
    info!("──────────────────────────────────────────────");
    info!("  Streamed {chunks} chunks  {total_bytes} bytes  {total_usdc:.6} USDC total");
    Ok(())
}
