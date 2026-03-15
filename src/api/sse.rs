use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::{sse::Event, IntoResponse, Sse, Response},
};
use futures::stream::Stream;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::convert::Infallible;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::timeout;
use tracing::{error, info, debug, warn};

use crate::state::AppState;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum TypedSseEvent {
    #[serde(rename = "atom_published")]
    AtomPublished {
        atom_id: String,
        domain: String,
        atom_type: String,
        embedding: Option<Vec<f64>>,
    },
    #[serde(rename = "contradiction_detected")]
    ContradictionDetected {
        atom_id: String,
        contradicting_atom_id: String,
        domain: String,
    },
    #[serde(rename = "synthesis_needed")]
    SynthesisNeeded {
        cluster_center: Vec<f64>,
        atom_count: usize,
        domain: String,
    },
    #[serde(rename = "pheromone_shift")]
    PheromoneShift {
        atom_id: String,
        field: String,
        old_value: f64,
        new_value: f64,
    },
}

#[derive(Debug, Deserialize)]
pub struct SseQueryParams {
    pub region: Option<String>,   // comma-separated float vector; absent = no spatial filter
    pub radius: Option<f64>,      // spatial radius for filtering; ignored when region is absent
    pub types: Option<String>,    // comma-separated event type strings; absent = all types
}

pub async fn sse_events(
    State(state): State<Arc<AppState>>,
    Query(params): Query<SseQueryParams>,
) -> std::result::Result<Response, (StatusCode, String)> {
    // region and radius must be provided together or both absent
    let spatial_filter: Option<(Vec<f64>, f64)> = match (params.region.as_deref(), params.radius) {
        (Some(r), Some(radius)) => {
            if radius <= 0.0 || radius > 1.0 {
                return Err((StatusCode::BAD_REQUEST, "Radius must be between 0.0 and 1.0".to_string()));
            }
            let region: Vec<f64> = r.split(',')
                .filter_map(|s| s.trim().parse().ok())
                .collect();
            if region.is_empty() {
                return Err((StatusCode::BAD_REQUEST, "Invalid region parameter".to_string()));
            }
            Some((region, radius))
        }
        (None, None) => None,
        _ => return Err((StatusCode::BAD_REQUEST, "Provide both region and radius, or neither".to_string())),
    };

    // Parse event types — absent means subscribe to all
    let requested_types: Option<std::collections::HashSet<String>> =
        params.types.as_deref().map(|t| {
            t.split(',').map(|s| s.trim().to_string()).collect()
        });

    info!(
        "SSE subscription: spatial_filter={}, types={:?}",
        spatial_filter.is_some(),
        requested_types
    );

    let stream = create_sse_stream(state, spatial_filter, requested_types);
    Ok(Sse::new(stream).into_response())
}

fn create_sse_stream(
    state: Arc<AppState>,
    spatial_filter: Option<(Vec<f64>, f64)>,
    requested_types: Option<std::collections::HashSet<String>>,
) -> impl Stream<Item = Result<Event, Infallible>> {
    let mut rx = state.sse_broadcast_tx.subscribe();
    let mut event_counter: u64 = 0;
    let mut embedding_cache: HashMap<String, Vec<f64>> = HashMap::new();

    async_stream::stream! {
        loop {
            match timeout(Duration::from_secs(3), rx.recv()).await {
                Ok(Ok(sse_event)) => {
                    // Check if event type is requested (None = all types pass)
                    if let Some(ref types) = requested_types {
                        if !types.contains(&sse_event.event_type) {
                            continue;
                        }
                    }

                    // Parse the typed event
                    match parse_typed_event(&sse_event.data) {
                        Ok(Some(typed_event)) => {
                            // Apply spatial filter only when region+radius were provided
                            let passes_spatial = match spatial_filter.as_ref() {
                                None => true,
                                Some((region, radius)) => {
                                    if let Some(embedding) = get_event_embedding(&typed_event, &state, &mut embedding_cache).await {
                                        let distance = cosine_distance(region, &embedding);
                                        if distance > *radius {
                                            debug!("Event filtered by distance: {} > {}", distance, radius);
                                            false
                                        } else {
                                            true
                                        }
                                    } else {
                                        debug!("Event filtered - no embedding available");
                                        false
                                    }
                                }
                            };

                            if passes_spatial {
                                event_counter += 1;
                                let sse_frame = Event::default()
                                    .event(&sse_event.event_type)
                                    .id(event_counter.to_string())
                                    .data(serde_json::to_string(&typed_event).unwrap_or_else(|e| {
                                        error!("Failed to serialize event: {}", e);
                                        "{}".to_string()
                                    }));
                                yield Ok(sse_frame);
                            }
                        }
                        Ok(None) => {
                            debug!("Event filtered - unsupported type");
                        }
                        Err(e) => {
                            error!("Failed to parse event: {}", e);
                        }
                    }
                }
                Ok(Err(tokio::sync::broadcast::error::RecvError::Lagged(n))) => {
                    warn!("SSE subscriber lagged, {} events dropped", n);
                    let lag_event = Event::default()
                        .comment(format!("lagged, {} events dropped", n));
                    yield Ok(lag_event);
                }
                Ok(Err(tokio::sync::broadcast::error::RecvError::Closed)) => {
                    info!("SSE broadcast channel closed");
                    break;
                }
                Err(_) => {
                    // Timeout - send keepalive comment
                    yield Ok(Event::default().comment("keepalive"));
                }
            }
        }
    }
}

async fn get_event_embedding(
    event: &TypedSseEvent,
    state: &AppState,
    cache: &mut HashMap<String, Vec<f64>>,
) -> Option<Vec<f64>> {
    match event {
        TypedSseEvent::AtomPublished { embedding, .. } => embedding.clone(),
        TypedSseEvent::ContradictionDetected { atom_id, .. } => {
            get_atom_embedding(atom_id, state, cache).await
        }
        TypedSseEvent::PheromoneShift { atom_id, .. } => {
            get_atom_embedding(atom_id, state, cache).await
        }
        TypedSseEvent::SynthesisNeeded { cluster_center, .. } => {
            Some(cluster_center.clone())
        }
    }
}

async fn get_atom_embedding(
    atom_id: &str,
    state: &AppState,
    cache: &mut HashMap<String, Vec<f64>>,
) -> Option<Vec<f64>> {
    // Check cache first
    if let Some(embedding) = cache.get(atom_id) {
        return Some(embedding.clone());
    }

    // Query database
    match sqlx::query_scalar::<_, Option<Vec<f64>>>("SELECT embedding FROM atoms WHERE atom_id = $1 AND embedding_status = 'ready'")
        .bind(atom_id)
        .fetch_one(&state.pool)
        .await
    {
        Ok(Some(embedding)) => {
            cache.insert(atom_id.to_string(), embedding.clone());
            Some(embedding)
        }
        Ok(None) => None,
        Err(e) => {
            error!("Failed to query embedding for atom {}: {}", atom_id, e);
            None
        }
    }
}

fn parse_typed_event(data: &serde_json::Value) -> Result<Option<TypedSseEvent>, serde_json::Error> {
    serde_json::from_value::<TypedSseEvent>(data.clone()).map(Some)
}

fn cosine_distance(a: &[f64], b: &[f64]) -> f64 {
    if a.len() != b.len() {
        return 1.0; // Maximum distance for different dimensions
    }

    let dot_product: f64 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f64 = a.iter().map(|x| x * x).sum::<f64>().sqrt();
    let norm_b: f64 = b.iter().map(|x| x * x).sum::<f64>().sqrt();

    if norm_a == 0.0 || norm_b == 0.0 {
        return 1.0;
    }

    1.0 - (dot_product / (norm_a * norm_b))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_cosine_distance() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![0.0, 1.0, 0.0];
        assert!((cosine_distance(&a, &b) - 1.0).abs() < f64::EPSILON);

        let a = vec![1.0, 0.0];
        let b = vec![1.0, 0.0];
        assert!((cosine_distance(&a, &b) - 0.0).abs() < f64::EPSILON);

        let a = vec![1.0, 1.0];
        let b = vec![-1.0, -1.0];
        let distance = cosine_distance(&a, &b);
        assert!((distance - 2.0).abs() < 1e-10, "Expected ~2.0, got {}", distance);
    }

    #[test]
    fn test_parse_typed_event() {
        let atom_event = json!({
            "type": "atom_published",
            "atom_id": "test-atom",
            "domain": "test",
            "atom_type": "finding",
            "embedding": [0.1, 0.2, 0.3]
        });

        let parsed = parse_typed_event(&atom_event).unwrap();
        match parsed {
            Some(TypedSseEvent::AtomPublished { atom_id, .. }) => {
                assert_eq!(atom_id, "test-atom");
            }
            _ => panic!("Expected AtomPublished event"),
        }
    }

    #[test]
    fn test_event_serialization() {
        let event = TypedSseEvent::AtomPublished {
            atom_id: "test-atom".to_string(),
            domain: "test".to_string(),
            atom_type: "finding".to_string(),
            embedding: Some(vec![0.1, 0.2, 0.3]),
        };

        let json = serde_json::to_string(&event).unwrap();
        let parsed: TypedSseEvent = serde_json::from_str(&json).unwrap();
        
        match parsed {
            TypedSseEvent::AtomPublished { atom_id, .. } => {
                assert_eq!(atom_id, "test-atom");
            }
            _ => panic!("Expected AtomPublished event"),
        }
    }
}
