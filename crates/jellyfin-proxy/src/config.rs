use anyhow::Context;

/// Runtime configuration sourced entirely from environment variables.
#[derive(Clone)]
pub struct Config {
    /// Base URL of the upstream Jellyfin server (e.g. "http://localhost:8096")
    pub jellyfin_url: String,
    /// Seller's Ethereum address — receives x402 payments
    pub seller_address: String,
    /// Seller's private key — used to call `grantAccess` on-chain
    pub seller_private_key: String,
    /// Arc testnet (or mainnet) JSON-RPC endpoint
    pub arc_rpc_url: String,
    /// Deployed MediaAccess contract address (0x-prefixed hex)
    pub media_access_contract: String,
    /// SQLite database URL, e.g. "sqlite://./data/access.db"
    pub database_url: String,
    /// TCP port for this proxy to listen on
    pub port: u16,
    /// Per-second stream price in atomic USDC units (1 atomic = $0.000001)
    pub stream_rate_per_sec_atomic: u64,
    /// Billing chunk duration in seconds; one payment covers this many seconds
    pub stream_chunk_secs: u64,
    /// One-time purchase price for permanent access (atomic USDC)
    pub buy_price_atomic: u64,
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        Ok(Config {
            jellyfin_url: std::env::var("JELLYFIN_URL")
                .unwrap_or_else(|_| "http://localhost:8096".to_string()),
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
                .unwrap_or_else(|_| "8080".to_string())
                .parse()
                .context("PORT must be a valid port number")?,
            stream_rate_per_sec_atomic: std::env::var("STREAM_RATE_PER_SEC")
                .unwrap_or_else(|_| "100".to_string())
                .parse()
                .context("STREAM_RATE_PER_SEC must be a u64")?,
            stream_chunk_secs: std::env::var("STREAM_CHUNK_SECS")
                .unwrap_or_else(|_| "60".to_string())
                .parse()
                .context("STREAM_CHUNK_SECS must be a u64")?,
            buy_price_atomic: std::env::var("BUY_PRICE_ATOMIC")
                .unwrap_or_else(|_| "10000".to_string())
                .parse()
                .context("BUY_PRICE_ATOMIC must be a u64")?,
        })
    }

    pub fn chunk_price_atomic(&self) -> u64 {
        self.stream_rate_per_sec_atomic * self.stream_chunk_secs
    }
}
