use std::{
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use arc_x402::{server::build_chunk_requirements, types::PaymentRequirements};
use dashmap::DashMap;
use uuid::Uuid;

/// Active per-chunk billing session.
#[allow(dead_code)]
pub struct StreamingSession {
    pub session_id: String,
    pub wallet: String,
    /// hex-encoded keccak256 of the item ID
    pub content_id: String,
    /// Payment requirements for a single chunk (pre-computed at session creation)
    pub chunk_requirements: PaymentRequirements,
    /// Monotonically increasing chunk counter — used to label delivered content pages
    pub chunks_delivered: AtomicU64,
    pub created_at: u64,
}

impl StreamingSession {
    pub fn next_chunk(&self) -> u64 {
        self.chunks_delivered.fetch_add(1, Ordering::Relaxed) + 1
    }
}

/// Thread-safe in-memory map of active billing sessions.
pub struct StreamingManager {
    pub sessions: DashMap<String, StreamingSession>,
}

impl StreamingManager {
    pub fn new() -> Self {
        Self { sessions: DashMap::new() }
    }

    /// Creates a new session and returns the session ID.
    ///
    /// `chunk_price_atomic` is resolved by the caller (may be per-item or global default).
    pub fn create_session(
        &self,
        wallet: String,
        content_id: String,
        item_id: &str,
        chunk_price_atomic: u64,
        seller_address: &str,
    ) -> String {
        let session_id = Uuid::new_v4().to_string();
        let resource_url = format!("/content/{}", item_id);

        let (chunk_requirements, _) =
            build_chunk_requirements(chunk_price_atomic, seller_address, &resource_url)
                .expect("build_chunk_requirements failed — seller address validated at startup");

        self.sessions.insert(session_id.clone(), StreamingSession {
            session_id: session_id.clone(),
            wallet,
            content_id,
            chunk_requirements,
            chunks_delivered: AtomicU64::new(0),
            created_at: now_unix(),
        });
        session_id
    }

    /// Removes sessions older than `max_age_secs`. Called from a background GC task.
    pub fn cleanup_expired(&self, max_age_secs: u64) {
        let now = now_unix();
        self.sessions.retain(|_, v| now.saturating_sub(v.created_at) < max_age_secs);
    }
}

impl Default for StreamingManager {
    fn default() -> Self {
        Self::new()
    }
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before epoch")
        .as_secs()
}
