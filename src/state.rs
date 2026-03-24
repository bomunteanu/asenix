use crate::config::Config;
use crate::db::graph_cache::GraphCache;
use crate::api::handlers::Metrics;
use sqlx::PgPool;
use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::{Arc, Mutex};
use tokio::sync::broadcast;
use tokio::sync::mpsc;
use tokio::time::Instant;

#[derive(Clone)]
pub struct RateLimiter {
    // agent_id -> (count, window_start)
    inner: Arc<Mutex<HashMap<String, (usize, Instant)>>>,
}

impl Default for RateLimiter {
    fn default() -> Self {
        Self::new()
    }
}

impl RateLimiter {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn check_rate_limit(&self, agent_id: &str, max_per_hour: usize) -> bool {
        // Zero limit should always reject
        if max_per_hour == 0 {
            return false;
        }

        let mut limiter = self.inner.lock().unwrap();
        let now = Instant::now();
        let window_duration = std::time::Duration::from_secs(3600); // 1 hour

        match limiter.get_mut(agent_id) {
            Some((count, window_start)) => {
                if now.duration_since(*window_start) >= window_duration {
                    // Reset window
                    *count = 1;
                    *window_start = now;
                    true
                } else if *count >= max_per_hour {
                    false
                } else {
                    *count += 1;
                    true
                }
            }
            None => {
                limiter.insert(agent_id.to_string(), (1, now));
                true
            }
        }
    }
}

/// Per-IP rate limiter.  Used for two independent limits:
///   - general unauthenticated requests (60/min)
///   - self-registration (5/hour)
#[derive(Clone)]
pub struct IpRateLimiter {
    // ip -> (count, window_start)
    inner: Arc<Mutex<HashMap<IpAddr, (usize, Instant)>>>,
}

impl Default for IpRateLimiter {
    fn default() -> Self { Self::new() }
}

impl IpRateLimiter {
    pub fn new() -> Self {
        Self { inner: Arc::new(Mutex::new(HashMap::new())) }
    }

    /// Returns `true` if the request is allowed, `false` if the limit is exceeded.
    /// `window_secs` is the width of the sliding window; `max` is the cap.
    pub fn check(&self, ip: IpAddr, max: usize, window_secs: u64) -> bool {
        if max == 0 { return false; }
        let mut map = self.inner.lock().unwrap();
        let now = Instant::now();
        let window = std::time::Duration::from_secs(window_secs);
        match map.get_mut(&ip) {
            Some((count, start)) => {
                if now.duration_since(*start) >= window {
                    *count = 1;
                    *start = now;
                    true
                } else if *count >= max {
                    false
                } else {
                    *count += 1;
                    true
                }
            }
            None => {
                map.insert(ip, (1, now));
                true
            }
        }
    }
}

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub graph_cache: Arc<tokio::sync::RwLock<GraphCache>>,
    pub condition_registry: Arc<tokio::sync::RwLock<crate::domain::condition::ConditionRegistry>>,
    pub sse_broadcast_tx: broadcast::Sender<SseEvent>,
    pub embedding_tx: mpsc::Sender<String>,
    pub rate_limiter: RateLimiter,
    pub ip_rate_limiter: IpRateLimiter,     // 60 req/min per IP (unauthenticated)
    pub reg_rate_limiter: IpRateLimiter,    // 5 registrations/hour per IP
    pub config: Arc<Config>,
    pub metrics: Arc<Metrics>,
    pub storage: Arc<crate::storage::LocalStorage>,
    pub session_store: Arc<crate::api::mcp_session::SessionStore>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SseEvent {
    pub event_type: String,
    pub data: serde_json::Value,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

impl AppState {
    pub async fn new(
        pool: PgPool,
        config: Arc<Config>,
        sse_broadcast_tx: broadcast::Sender<SseEvent>,
        storage: Arc<crate::storage::LocalStorage>,
        embedding_tx: mpsc::Sender<String>,
    ) -> Result<Self, crate::error::MoteError> {
        // Initialize graph cache from DB so it survives restarts
        let graph_cache = Arc::new(tokio::sync::RwLock::new(
            GraphCache::load_from_database(&pool).await
                .map_err(|e| crate::error::MoteError::Internal(
                    format!("Cannot start: failed to load graph cache: {}", e)
                ))?
        ));

        // Initialize condition registry
        let condition_registry = Arc::new(tokio::sync::RwLock::new(
            crate::domain::condition::ConditionRegistry::new()
        ));

        Ok(Self {
            pool,
            graph_cache,
            condition_registry,
            sse_broadcast_tx,
            embedding_tx,
            rate_limiter: RateLimiter::new(),
            ip_rate_limiter: IpRateLimiter::new(),
            reg_rate_limiter: IpRateLimiter::new(),
            config,
            metrics: Arc::new(Metrics::default()),
            storage,
            session_store: Arc::new(crate::api::mcp_session::SessionStore::new()),
        })
    }
}
