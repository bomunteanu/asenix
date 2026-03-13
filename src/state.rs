use crate::config::Config;
use crate::db::graph_cache::GraphCache;
use crate::api::handlers::Metrics;
use sqlx::PgPool;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::sync::{broadcast, mpsc};
use tokio::time::Instant;

#[derive(Clone)]
pub struct RateLimiter {
    // agent_id -> (count, window_start)
    inner: Arc<Mutex<HashMap<String, (usize, Instant)>>>,
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

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub graph_cache: Arc<tokio::sync::RwLock<GraphCache>>,
    pub condition_registry: Arc<tokio::sync::RwLock<crate::domain::condition::ConditionRegistry>>,
    pub embedding_queue_tx: mpsc::Sender<String>, // atom_id
    pub sse_broadcast_tx: broadcast::Sender<SseEvent>,
    pub rate_limiter: RateLimiter,
    pub config: Arc<Config>,
    pub metrics: Arc<Metrics>,
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
        embedding_queue_tx: mpsc::Sender<String>,
        sse_broadcast_tx: broadcast::Sender<SseEvent>,
    ) -> Result<Self, crate::error::MoteError> {
        // Initialize graph cache
        let graph_cache = Arc::new(tokio::sync::RwLock::new(
            GraphCache::new()
        ));

        // Initialize condition registry
        let condition_registry = Arc::new(tokio::sync::RwLock::new(
            crate::domain::condition::ConditionRegistry::new()
        ));

        Ok(Self {
            pool,
            graph_cache,
            condition_registry,
            embedding_queue_tx,
            sse_broadcast_tx,
            rate_limiter: RateLimiter::new(),
            config,
            metrics: Arc::new(Metrics::default()),
        })
    }
}
