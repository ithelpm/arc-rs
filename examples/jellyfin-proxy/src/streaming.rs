use std::time::{SystemTime, UNIX_EPOCH};

use arc_x402::{server::build_chunk_requirements, types::PaymentRequirements};
use dashmap::DashMap;
use uuid::Uuid;

use crate::config::Config;

/// Active per-second billing session.
/// Stored in memory; audited to SQLite via `AccessManager::log_streaming_session`.
#[allow(dead_code)]
pub struct StreamingSession {
    pub session_id: String,
    pub wallet: String,
    /// hex-encoded keccak256 of the Jellyfin item ID
    pub content_id: String,
    /// Payment requirements for a single chunk (pre-computed at session creation)
    pub chunk_requirements: PaymentRequirements,
    pub created_at: u64,
}

/// Thread-safe in-memory map of active streaming sessions.
pub struct StreamingManager {
    pub sessions: DashMap<String, StreamingSession>,
}

impl StreamingManager {
    pub fn new() -> Self {
        Self {
            sessions: DashMap::new(),
        }
    }

    /// Creates a new session and returns the session ID.
    ///
    /// Panics if `cfg.seller_address` is invalid — this is caught at startup by
    /// `Address::from_str` before the server begins accepting requests.
    pub fn create_session(
        &self,
        wallet: String,
        content_id: String,
        item_id: &str,
        cfg: &Config,
    ) -> String {
        let session_id = Uuid::new_v4().to_string();
        let resource_url = format!("/Videos/{}/stream", item_id);

        let (chunk_requirements, _) =
            build_chunk_requirements(cfg.chunk_price_atomic(), &cfg.seller_address, &resource_url)
                .expect("build_chunk_requirements failed — seller address validated at startup");

        let session = StreamingSession {
            session_id: session_id.clone(),
            wallet,
            content_id,
            chunk_requirements,
            created_at: now_unix(),
        };

        self.sessions.insert(session_id.clone(), session);
        session_id
    }

    /// Removes sessions older than `max_age_secs`. Call periodically from a background task.
    pub fn cleanup_expired(&self, max_age_secs: u64) {
        let now = now_unix();
        self.sessions
            .retain(|_, v| now.saturating_sub(v.created_at) < max_age_secs);
    }

    #[allow(dead_code)]
    pub fn remove(&self, session_id: &str) {
        self.sessions.remove(session_id);
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
