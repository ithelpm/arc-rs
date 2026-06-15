use std::{
    str::FromStr,
    time::{SystemTime, UNIX_EPOCH},
};

use alloy_primitives::{Address, TxHash, B256};
use anyhow::Context;
use media_access::MediaAccessClient;
use sqlx::SqlitePool;

/// Manages on-chain access grants with a local SQLite cache.
///
/// The cache avoids hitting the chain on every request; the chain remains the
/// source of truth and is re-checked when the cache TTL expires.
pub struct AccessManager {
    pool: SqlitePool,
    chain: MediaAccessClient,
    cache_ttl_secs: u64,
}

impl AccessManager {
    /// Opens the database, runs schema migrations, and returns a ready manager.
    pub async fn new(database_url: &str, chain: MediaAccessClient) -> anyhow::Result<Self> {
        // Create parent directory if using a file-based path
        if let Some(path) = database_url.strip_prefix("sqlite:") {
            if let Some(parent) = std::path::Path::new(path.trim_start_matches('/')).parent() {
                if !parent.as_os_str().is_empty() {
                    tokio::fs::create_dir_all(parent).await.ok();
                }
            }
        }

        let pool = SqlitePool::connect(database_url)
            .await
            .context("failed to open SQLite database")?;

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS access_cache (
                wallet      TEXT NOT NULL,
                content_id  TEXT NOT NULL,
                granted_at  INTEGER NOT NULL,
                verified_at INTEGER NOT NULL,
                PRIMARY KEY (wallet, content_id)
            )",
        )
        .execute(&pool)
        .await?;

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS streaming_sessions_log (
                session_id          TEXT PRIMARY KEY,
                wallet              TEXT NOT NULL,
                content_id          TEXT NOT NULL,
                rate_per_sec_atomic INTEGER NOT NULL,
                chunk_duration_secs INTEGER NOT NULL,
                created_at          INTEGER NOT NULL
            )",
        )
        .execute(&pool)
        .await?;

        Ok(Self {
            pool,
            chain,
            cache_ttl_secs: 3600, // re-verify against chain every hour
        })
    }

    /// Returns true if `wallet` has access to `content_id`.
    ///
    /// Checks the local cache first; falls back to the chain when the cache is
    /// cold or stale.
    pub async fn check_access(&self, wallet: &str, content_id: B256) -> anyhow::Result<bool> {
        let content_hex = b256_to_hex(content_id);
        let now = now_unix();

        // Fast path: cached access within TTL
        let row = sqlx::query_as::<_, (i64,)>(
            "SELECT verified_at FROM access_cache WHERE wallet = ?1 AND content_id = ?2",
        )
        .bind(wallet)
        .bind(&content_hex)
        .fetch_optional(&self.pool)
        .await?;

        if let Some((verified_at,)) = row {
            if now.saturating_sub(verified_at as u64) < self.cache_ttl_secs {
                return Ok(true);
            }
        }

        // Slow path: ask the chain
        let addr = Address::from_str(wallet).context("invalid wallet address")?;
        let has = self.chain.has_access(addr, content_id).await?;
        if has {
            self.write_cache(wallet, &content_hex, now).await?;
        }
        Ok(has)
    }

    /// Calls `grantAccess` on-chain and records the result in the local cache.
    pub async fn grant_and_record(&self, wallet: &str, content_id: B256) -> anyhow::Result<TxHash> {
        let addr = Address::from_str(wallet).context("invalid wallet address")?;
        let tx = self.chain.grant_access(addr, content_id).await?;
        let content_hex = b256_to_hex(content_id);
        self.write_cache(wallet, &content_hex, now_unix()).await?;
        Ok(tx)
    }

    /// Persists a streaming session to the audit log.
    pub async fn log_streaming_session(
        &self,
        session_id: &str,
        wallet: &str,
        content_id: &str,
        rate_per_sec: u64,
        chunk_secs: u64,
    ) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT OR IGNORE INTO streaming_sessions_log
             (session_id, wallet, content_id, rate_per_sec_atomic, chunk_duration_secs, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        )
        .bind(session_id)
        .bind(wallet)
        .bind(content_id)
        .bind(rate_per_sec as i64)
        .bind(chunk_secs as i64)
        .bind(now_unix() as i64)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn write_cache(&self, wallet: &str, content_hex: &str, now: u64) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO access_cache (wallet, content_id, granted_at, verified_at)
             VALUES (?1, ?2, ?3, ?3)
             ON CONFLICT(wallet, content_id) DO UPDATE SET verified_at = ?3",
        )
        .bind(wallet)
        .bind(content_hex)
        .bind(now as i64)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}

fn b256_to_hex(b: B256) -> String {
    format!("0x{}", hex::encode(b.as_slice()))
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before epoch")
        .as_secs()
}
