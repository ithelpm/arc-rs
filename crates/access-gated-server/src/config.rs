use std::collections::HashMap;

use anyhow::Context;
use serde::Deserialize;

/// Per-item pricing and metadata entry in the catalog.
#[derive(Debug, Clone, Deserialize)]
pub struct ItemConfig {
    /// Display title shown in content responses.
    pub title: String,
    /// Short description of what the buyer gets.
    #[serde(default)]
    pub description: String,
    /// One-time buy price in atomic USDC (1 USDC = 1_000_000). Falls back to global default.
    pub buy_price_atomic: Option<u64>,
    /// Per-chunk streaming price in atomic USDC. Falls back to global default.
    pub chunk_price_atomic: Option<u64>,
}

/// Map of item_id → ItemConfig loaded from items.json.
#[derive(Debug, Clone, Default)]
pub struct ItemCatalog(HashMap<String, ItemConfig>);

impl ItemCatalog {
    pub fn buy_price(&self, item_id: &str, global_default: u64) -> u64 {
        self.0
            .get(item_id)
            .and_then(|c| c.buy_price_atomic)
            .unwrap_or(global_default)
    }

    pub fn chunk_price(&self, item_id: &str, global_default: u64) -> u64 {
        self.0
            .get(item_id)
            .and_then(|c| c.chunk_price_atomic)
            .unwrap_or(global_default)
    }

    pub fn title(&self, item_id: &str) -> String {
        self.0
            .get(item_id)
            .map(|c| c.title.clone())
            .unwrap_or_else(|| item_id.to_string())
    }

    pub fn description(&self, item_id: &str) -> String {
        self.0
            .get(item_id)
            .map(|c| c.description.clone())
            .unwrap_or_default()
    }
}

/// Runtime configuration sourced entirely from environment variables.
#[derive(Clone)]
pub struct Config {
    /// Seller's Ethereum address — receives x402 payments
    pub seller_address: String,
    /// Seller's private key — used to call `grantAccess` on-chain
    pub seller_private_key: String,
    /// Arc testnet JSON-RPC endpoint
    pub arc_rpc_url: String,
    /// Deployed MediaAccess contract address (0x-prefixed hex)
    pub media_access_contract: String,
    /// SQLite database URL, e.g. "sqlite:./data/access.db"
    pub database_url: String,
    /// TCP port for this server to listen on
    pub port: u16,
    /// Global fallback per-chunk billing rate (atomic USDC)
    pub chunk_price_atomic: u64,
    /// Global fallback one-time purchase price (atomic USDC)
    pub buy_price_atomic: u64,
    /// Per-item pricing catalog loaded from items.json (optional)
    pub catalog: ItemCatalog,
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        let catalog_path = std::env::var("ITEMS_CATALOG_PATH")
            .unwrap_or_else(|_| "items.json".to_string());

        let catalog = if std::path::Path::new(&catalog_path).exists() {
            let raw = std::fs::read_to_string(&catalog_path)
                .with_context(|| format!("failed to read {catalog_path}"))?;
            let map: HashMap<String, ItemConfig> = serde_json::from_str(&raw)
                .with_context(|| format!("failed to parse {catalog_path}"))?;
            tracing::info!(path = %catalog_path, items = map.len(), "loaded item catalog");
            ItemCatalog(map)
        } else {
            tracing::info!("no items.json found — using global price defaults for all items");
            ItemCatalog::default()
        };

        Ok(Config {
            seller_address: std::env::var("SELLER_ADDRESS")
                .context("SELLER_ADDRESS env var required")?,
            seller_private_key: std::env::var("SELLER_PRIVATE_KEY")
                .context("SELLER_PRIVATE_KEY env var required")?,
            arc_rpc_url: std::env::var("ARC_RPC_URL")
                .unwrap_or_else(|_| "https://rpc.testnet.arc.network".to_string()),
            media_access_contract: std::env::var("MEDIA_ACCESS_CONTRACT")
                .context("MEDIA_ACCESS_CONTRACT env var required")?,
            database_url: std::env::var("DATABASE_URL")
                .unwrap_or_else(|_| "sqlite:./data/access.db".to_string()),
            port: std::env::var("PORT")
                .unwrap_or_else(|_| "3001".to_string())
                .parse()
                .context("PORT must be a valid port number")?,
            chunk_price_atomic: std::env::var("CHUNK_PRICE_ATOMIC")
                .unwrap_or_else(|_| "1000".to_string())
                .parse()
                .context("CHUNK_PRICE_ATOMIC must be a u64")?,
            buy_price_atomic: std::env::var("BUY_PRICE_ATOMIC")
                .unwrap_or_else(|_| "5000000".to_string())
                .parse()
                .context("BUY_PRICE_ATOMIC must be a u64")?,
            catalog,
        })
    }
}
