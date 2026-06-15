use anyhow::Context;

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
    /// Per-chunk billing rate in atomic USDC units (1 USDC = 1_000_000 atomic)
    pub chunk_price_atomic: u64,
    /// One-time purchase price for permanent access (atomic USDC)
    pub buy_price_atomic: u64,
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
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
        })
    }
}
