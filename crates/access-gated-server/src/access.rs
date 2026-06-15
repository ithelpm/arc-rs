use std::{
    str::FromStr,
    time::{SystemTime, UNIX_EPOCH},
};

use alloy_primitives::{Address, TxHash, B256};
use anyhow::Context;
use media_access::MediaAccessClient;
use sqlx::SqlitePool;

// ─── Public data types ────────────────────────────────────────────────────────

#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize)]
pub struct ItemRow {
    pub item_id: String,
    pub seller: String,
    pub title: String,
    pub description: String,
    pub buy_price_atomic: i64,
    pub chunk_price_atomic: i64,
    pub created_at: i64,
}

#[derive(Debug, serde::Serialize)]
pub struct ItemStats {
    pub item_id: String,
    pub title: String,
    pub payments: i64,
    pub volume_atomic: i64,
}

#[derive(Debug, serde::Serialize)]
pub struct Stats {
    pub total_payments: i64,
    pub total_volume_atomic: i64,
    pub unique_buyers: i64,
    pub unique_sellers: i64,
    pub items: Vec<ItemStats>,
}

// ─── AccessManager ────────────────────────────────────────────────────────────

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

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS items (
                item_id            TEXT PRIMARY KEY,
                seller             TEXT NOT NULL,
                title              TEXT NOT NULL,
                description        TEXT NOT NULL DEFAULT '',
                buy_price_atomic   INTEGER NOT NULL,
                chunk_price_atomic INTEGER NOT NULL,
                created_at         INTEGER NOT NULL
            )",
        )
        .execute(&pool)
        .await?;

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS payments (
                id              INTEGER PRIMARY KEY AUTOINCREMENT,
                item_id         TEXT NOT NULL,
                seller          TEXT NOT NULL,
                buyer           TEXT NOT NULL,
                amount_atomic   INTEGER NOT NULL,
                mode            TEXT NOT NULL,
                gateway_ref     TEXT,
                settled_at      INTEGER NOT NULL
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

    // ─── Item catalog (DB-backed) ─────────────────────────────────────────────

    pub async fn get_items(&self) -> anyhow::Result<Vec<ItemRow>> {
        Ok(sqlx::query_as::<_, ItemRow>(
            "SELECT * FROM items ORDER BY created_at DESC",
        )
        .fetch_all(&self.pool)
        .await?)
    }

    pub async fn get_item(&self, item_id: &str) -> anyhow::Result<Option<ItemRow>> {
        Ok(sqlx::query_as::<_, ItemRow>(
            "SELECT * FROM items WHERE item_id = ?",
        )
        .bind(item_id)
        .fetch_optional(&self.pool)
        .await?)
    }

    pub async fn upsert_item(&self, item: &ItemRow) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT OR REPLACE INTO items \
             (item_id, seller, title, description, buy_price_atomic, chunk_price_atomic, created_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&item.item_id)
        .bind(&item.seller)
        .bind(&item.title)
        .bind(&item.description)
        .bind(item.buy_price_atomic)
        .bind(item.chunk_price_atomic)
        .bind(item.created_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    // ─── Payment log ──────────────────────────────────────────────────────────

    pub async fn log_payment(
        &self,
        item_id: &str,
        seller: &str,
        buyer: &str,
        amount_atomic: i64,
        mode: &str,
        gateway_ref: Option<&str>,
    ) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO payments \
             (item_id, seller, buyer, amount_atomic, mode, gateway_ref, settled_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(item_id)
        .bind(seller)
        .bind(buyer)
        .bind(amount_atomic)
        .bind(mode)
        .bind(gateway_ref)
        .bind(now_unix() as i64)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn get_stats(&self) -> anyhow::Result<Stats> {
        let total_payments: i64 = sqlx::query_as::<_, (i64,)>("SELECT COUNT(*) FROM payments")
            .fetch_one(&self.pool)
            .await
            .map(|(c,)| c)
            .unwrap_or(0);

        let total_volume_atomic: i64 =
            sqlx::query_as::<_, (Option<i64>,)>("SELECT SUM(amount_atomic) FROM payments")
                .fetch_one(&self.pool)
                .await
                .map(|(s,)| s.unwrap_or(0))
                .unwrap_or(0);

        let unique_buyers: i64 =
            sqlx::query_as::<_, (i64,)>("SELECT COUNT(DISTINCT buyer) FROM payments")
                .fetch_one(&self.pool)
                .await
                .map(|(c,)| c)
                .unwrap_or(0);

        let unique_sellers: i64 =
            sqlx::query_as::<_, (i64,)>("SELECT COUNT(DISTINCT seller) FROM payments")
                .fetch_one(&self.pool)
                .await
                .map(|(c,)| c)
                .unwrap_or(0);

        #[derive(sqlx::FromRow)]
        struct ItemStatsRow {
            item_id: String,
            title: String,
            payments: i64,
            volume_atomic: i64,
        }

        let item_rows = sqlx::query_as::<_, ItemStatsRow>(
            "SELECT p.item_id, COALESCE(i.title, p.item_id) as title, \
             COUNT(*) as payments, SUM(p.amount_atomic) as volume_atomic \
             FROM payments p LEFT JOIN items i ON p.item_id = i.item_id \
             GROUP BY p.item_id",
        )
        .fetch_all(&self.pool)
        .await
        .unwrap_or_default();

        let items = item_rows
            .into_iter()
            .map(|r| ItemStats {
                item_id: r.item_id,
                title: r.title,
                payments: r.payments,
                volume_atomic: r.volume_atomic,
            })
            .collect();

        Ok(Stats {
            total_payments,
            total_volume_atomic,
            unique_buyers,
            unique_sellers,
            items,
        })
    }
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

fn b256_to_hex(b: B256) -> String {
    format!("0x{}", hex::encode(b.as_slice()))
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before epoch")
        .as_secs()
}
