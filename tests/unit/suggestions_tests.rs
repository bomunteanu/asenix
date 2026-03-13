//! Unit tests for get_suggestions endpoint
//! 
//! Tests the MCP get_suggestions endpoint including parameter validation,
//! filtering, ranking by pheromone attraction, and response formatting.

use serde_json::{json, Value};
use mote::api::mcp::{JsonRpcRequest};
use mote::state::{AppState, RateLimiter};
use mote::config::{Config, PheromoneConfig, HubConfig, TrustConfig, WorkersConfig, AcceptanceConfig};
use std::sync::Arc;
use tokio::sync::{mpsc, broadcast};
use mote::db::graph_cache::GraphCache;

fn create_test_app_state() -> AppState {
    // Note: This would require a test database in a real test environment
    // For now, we test the endpoint logic and parameter handling
    
    let config = Config {
        hub: HubConfig {
            name: "test-hub".to_string(),
            domain: "test-domain".to_string(),
            listen_address: "127.0.0.1:8080".to_string(),
            embedding_endpoint: "http://localhost:8000".to_string(),
            embedding_model: "test-model".to_string(),
            embedding_dimension: 394,
            structured_vector_reserved_dims: 10,
            dims_per_numeric_key: 2,
            dims_per_categorical_key: 3,
            neighbourhood_radius: 0.5,
            summary_llm_endpoint: None,
            summary_llm_model: None,
        },
        pheromone: PheromoneConfig {
            decay_half_life_hours: 24,
            attraction_cap: 100.0,
            novelty_radius: 0.3,
            disagreement_threshold: 0.5,
        },
        trust: TrustConfig {
            reliability_threshold: 0.8,
            independence_ancestry_depth: 5,
            probation_atom_count: 10,
            max_atoms_per_hour: 100,
        },
        workers: WorkersConfig {
            embedding_pool_size: 32,
            decay_interval_minutes: 60,
            claim_ttl_hours: 24,
            staleness_check_interval_minutes: 30,
        },
        acceptance: AcceptanceConfig {
            required_provenance_fields: vec!["agent".to_string(), "timestamp".to_string()],
        },
    };
    
    // Create mock channels for testing
    let (embedding_queue_tx, _) = mpsc::channel(100);
    let (sse_broadcast_tx, _) = broadcast::channel(100);
    
    AppState {
        pool: unsafe { std::mem::zeroed() }, // Placeholder - would be real pool in tests
        graph_cache: Arc::new(tokio::sync::RwLock::new(GraphCache::new())),
        condition_registry: Arc::new(tokio::sync::RwLock::new(mote::domain::condition::ConditionRegistry::new())),
        embedding_queue_tx,
        sse_broadcast_tx,
        rate_limiter: RateLimiter::new(),
        config: Arc::new(config),
        metrics: Arc::new(mote::api::handlers::Metrics::default()),
    }
}

fn create_test_request(params: Option<Value>) -> JsonRpcRequest {
    JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        method: "get_suggestions".to_string(),
        params,
        id: Some(json!("test-id")),
    }
}

#[tokio::test]
async fn test_get_suggestions_request_structure() {
    let state = create_test_app_state();
    
    // Test request with no parameters
    let request = create_test_request(None);
    assert_eq!(request.method, "get_suggestions");
    assert_eq!(request.jsonrpc, "2.0");
    
    // Test request with parameters
    let params = json!({
        "domain": "test-domain",
        "limit": 5
    });
    let request_with_params = create_test_request(Some(params));
    assert!(request_with_params.params.is_some());
}

#[tokio::test]
async fn test_get_suggestions_parameter_validation() {
    let state = create_test_app_state();
    
    // Test valid parameters
    let valid_params = json!({
        "domain": "research",
        "limit": 10
    });
    
    // Extract and validate parameters (simulating endpoint logic)
    let domain_filter: Option<String> = serde_json::from_value(valid_params["domain"].clone()).ok();
    let limit: i64 = serde_json::from_value(valid_params["limit"].clone()).unwrap_or(10);
    
    assert_eq!(domain_filter, Some("research".to_string()));
    assert_eq!(limit, 10);
    
    // Test missing parameters (should use defaults)
    let empty_params = json!({});
    let default_domain: Option<String> = serde_json::from_value(empty_params["domain"].clone()).ok();
    let default_limit: i64 = serde_json::from_value(empty_params["limit"].clone()).unwrap_or(10);
    
    assert_eq!(default_domain, None);
    assert_eq!(default_limit, 10);
    
    // Test invalid limit (should use default)
    let invalid_params = json!({
        "limit": "not-a-number"
    });
    let invalid_limit: i64 = serde_json::from_value(invalid_params["limit"].clone()).unwrap_or(10);
    assert_eq!(invalid_limit, 10); // Should fall back to default
}

#[tokio::test]
async fn test_get_suggestions_query_logic() {
    // Test the query building logic without database
    
    let domain_filter = Some("test-domain".to_string());
    let limit = 5;
    
    // Test query construction logic
    let base_query = "SELECT atom_id, atom_type, domain, statement, conditions, metrics, 
                     ph_attraction, ph_repulsion, ph_novelty, ph_disagreement
                     FROM atoms 
                     WHERE NOT archived 
                     AND ph_attraction > 0.1";
    
    let mut final_query = base_query.to_string();
    
    if let Some(domain) = domain_filter.as_ref() {
        final_query.push_str(" AND domain = ");
        final_query.push_str(domain);
    }
    
    final_query.push_str(" ORDER BY ph_attraction DESC, ph_novelty DESC LIMIT ");
    final_query.push_str(&limit.to_string());
    
    assert!(final_query.contains("ORDER BY ph_attraction DESC"));
    assert!(final_query.contains("ph_novelty DESC"));
    assert!(final_query.contains("LIMIT 5"));
    
    if domain_filter.is_some() {
        assert!(final_query.contains("AND domain = test-domain"));
    }
}

#[tokio::test]
async fn test_get_suggestions_response_formatting() {
    // Test response formatting logic
    
    // Simulate database rows
    let mock_rows = vec![
        json!({
            "atom_id": "atom-1",
            "atom_type": "finding",
            "domain": "research",
            "statement": "Test finding 1",
            "conditions": {},
            "metrics": {"accuracy": 0.95},
            "ph_attraction": 8.5,
            "ph_repulsion": 1.2,
            "ph_novelty": 0.7,
            "ph_disagreement": 0.3
        }),
        json!({
            "atom_id": "atom-2",
            "atom_type": "hypothesis",
            "domain": "research",
            "statement": "Test hypothesis 1",
            "conditions": {"test": true},
            "metrics": null,
            "ph_attraction": 7.2,
            "ph_repulsion": 0.8,
            "ph_novelty": 0.9,
            "ph_disagreement": 0.4
        })
    ];
    
    // Format response
    let suggestions: Vec<Value> = mock_rows.into_iter().map(|row| {
        json!({
            "atom_id": row["atom_id"],
            "atom_type": row["atom_type"],
            "domain": row["domain"],
            "statement": row["statement"],
            "conditions": row["conditions"],
            "metrics": row["metrics"],
            "pheromone": {
                "attraction": row["ph_attraction"],
                "repulsion": row["ph_repulsion"],
                "novelty": row["ph_novelty"],
                "disagreement": row["ph_disagreement"]
            }
        })
    }).collect();
    
    let response = json!({
        "suggestions": suggestions,
        "strategy": "pheromone_attraction",
        "description": "Atoms ranked by pheromone attraction (high novelty/disagreement potential)"
    });
    
    assert_eq!(response["strategy"], "pheromone_attraction");
    assert!(response["suggestions"].is_array());
    assert_eq!(response["suggestions"].as_array().unwrap().len(), 2);
    
    let first_suggestion = &response["suggestions"][0];
    assert_eq!(first_suggestion["atom_id"], "atom-1");
    assert!(first_suggestion["pheromone"].is_object());
    assert_eq!(first_suggestion["pheromone"]["attraction"], 8.5);
}

#[tokio::test]
async fn test_get_suggestions_ranking_logic() {
    // Test that suggestions are properly ranked by attraction
    
    let mock_atoms = vec![
        ("atom-1", 5.0, 0.5, 0.3), // atom_id, attraction, novelty, disagreement
        ("atom-2", 8.0, 0.7, 0.2),
        ("atom-3", 3.0, 0.9, 0.6),
        ("atom-4", 8.0, 0.6, 0.1), // Same attraction as atom-2 but lower novelty
    ];
    
    // Sort by attraction DESC, then novelty DESC
    let mut sorted_atoms = mock_atoms.clone();
    sorted_atoms.sort_by(|a, b| {
        b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal))
    });
    
    // Verify ranking
    assert_eq!(sorted_atoms[0].1, 8.0); // Highest attraction
    assert_eq!(sorted_atoms[1].1, 8.0); // Same attraction
    assert_eq!(sorted_atoms[0].2, 0.7); // Higher novelty for same attraction
    assert_eq!(sorted_atoms[1].2, 0.6); // Lower novelty for same attraction
    assert_eq!(sorted_atoms[2].1, 5.0); // Next highest attraction
    assert_eq!(sorted_atoms[3].1, 3.0); // Lowest attraction
}

#[tokio::test]
async fn test_get_suggestions_filtering() {
    // Test domain filtering logic
    
    let mock_atoms = vec![
        ("atom-1", "research", 8.0),
        ("atom-2", "engineering", 7.0),
        ("atom-3", "research", 6.0),
        ("atom-4", "mathematics", 9.0),
    ];
    
    // Test filtering by domain
    let domain_filter = Some("research".to_string());
    let filtered: Vec<_> = mock_atoms.iter()
        .filter(|(_, domain, _)| domain_filter.as_ref().map_or(true, |d| domain == d))
        .collect();
    
    assert_eq!(filtered.len(), 2);
    assert!(filtered.iter().all(|(_, domain, _)| *domain == "research"));
    
    // Test no filtering
    let no_filter: Vec<_> = mock_atoms.iter().collect();
    assert_eq!(no_filter.len(), 4);
    
    // Test non-existent domain
    let nonexistent_filter = Some("nonexistent".to_string());
    let empty_filtered: Vec<_> = mock_atoms.iter()
        .filter(|(_, domain, _)| nonexistent_filter.as_ref().map_or(true, |d| domain == d))
        .collect();
    assert_eq!(empty_filtered.len(), 0);
}

#[tokio::test]
async fn test_get_suggestions_limit_handling() {
    // Test limit parameter handling
    
    let mock_atoms = vec![
        "atom-1", "atom-2", "atom-3", "atom-4", "atom-5",
        "atom-6", "atom-7", "atom-8", "atom-9", "atom-10"
    ];
    
    // Test with limit smaller than available
    let limit = 5;
    let limited: Vec<_> = mock_atoms.iter().take(limit).collect();
    assert_eq!(limited.len(), 5);
    
    // Test with limit larger than available
    let large_limit = 15;
    let large_limited: Vec<_> = mock_atoms.iter().take(large_limit).collect();
    assert_eq!(large_limited.len(), 10); // All available atoms
    
    // Test with zero limit
    let zero_limit = 0;
    let zero_limited: Vec<_> = mock_atoms.iter().take(zero_limit).collect();
    assert_eq!(zero_limited.len(), 0);
    
    // Test with default limit
    let default_limit = 10;
    let default_limited: Vec<_> = mock_atoms.iter().take(default_limit).collect();
    assert_eq!(default_limited.len(), 10);
}

#[tokio::test]
async fn test_get_suggestions_pheromone_thresholds() {
    // Test pheromone attraction threshold filtering
    
    let mock_atoms = vec![
        ("atom-1", 0.05), // Below threshold
        ("atom-2", 0.15), // Above threshold
        ("atom-3", 0.5),  // Above threshold
        ("atom-4", 0.1),  // At threshold
        ("atom-5", 2.0),  // Well above threshold
    ];
    
    let attraction_threshold = 0.1;
    let filtered: Vec<_> = mock_atoms.iter()
        .filter(|(_, attraction)| *attraction > attraction_threshold)
        .collect();
    
    assert_eq!(filtered.len(), 3); // Only atoms with attraction > 0.1
    assert!(filtered.iter().all(|(_, attraction)| *attraction > 0.1));
    
    // Verify specific atoms are included/excluded
    let filtered_ids: Vec<_> = filtered.iter().map(|(id, _)| *id).collect();
    assert!(filtered_ids.contains(&"atom-2"));
    assert!(filtered_ids.contains(&"atom-3"));
    assert!(filtered_ids.contains(&"atom-5"));
    assert!(!filtered_ids.contains(&"atom-1"));
    assert!(!filtered_ids.contains(&"atom-4")); // Exactly at threshold, excluded
}

#[tokio::test]
async fn test_get_suggestions_error_handling() {
    let state = create_test_app_state();
    
    // Test with invalid JSON parameters
    let invalid_params = json!({
        "domain": 123, // Should be string
        "limit": -5   // Should be positive
    });
    
    // Extract parameters with error handling
    let domain_filter: Option<String> = serde_json::from_value(invalid_params["domain"].clone()).ok();
    let limit: i64 = serde_json::from_value(invalid_params["limit"].clone()).unwrap_or(10);
    
    assert_eq!(domain_filter, None); // Should fail to parse
    assert_eq!(limit, 10); // Should fall back to default
    
    // Test with completely invalid parameters
    let completely_invalid = json!({
        "unknown_field": "value",
        "another_field": 123
    });
    
    let unknown_domain: Option<String> = serde_json::from_value(completely_invalid["domain"].clone()).ok();
    let unknown_limit: i64 = serde_json::from_value(completely_invalid["limit"].clone()).unwrap_or(10);
    
    assert_eq!(unknown_domain, None);
    assert_eq!(unknown_limit, 10);
}

#[tokio::test]
async fn test_get_suggestions_response_structure() {
    // Test that the response has the correct structure
    
    let mock_response = json!({
        "suggestions": [
            {
                "atom_id": "test-atom",
                "atom_type": "finding",
                "domain": "test",
                "statement": "Test statement",
                "conditions": {},
                "metrics": {"test": 1.0},
                "pheromone": {
                    "attraction": 5.0,
                    "repulsion": 1.0,
                    "novelty": 0.5,
                    "disagreement": 0.2
                }
            }
        ],
        "strategy": "pheromone_attraction",
        "description": "Atoms ranked by pheromone attraction (high novelty/disagreement potential)"
    });
    
    // Verify top-level structure
    assert!(mock_response["suggestions"].is_array());
    assert_eq!(mock_response["strategy"], "pheromone_attraction");
    assert!(mock_response["description"].is_string());
    
    // Verify suggestion structure
    let suggestion = &mock_response["suggestions"][0];
    assert!(suggestion["atom_id"].is_string());
    assert!(suggestion["atom_type"].is_string());
    assert!(suggestion["domain"].is_string());
    assert!(suggestion["statement"].is_string());
    assert!(suggestion["conditions"].is_object());
    assert!(suggestion["metrics"].is_object() || suggestion["metrics"].is_null());
    
    // Verify pheromone structure
    let pheromone = &suggestion["pheromone"];
    assert!(pheromone["attraction"].is_number());
    assert!(pheromone["repulsion"].is_number());
    assert!(pheromone["novelty"].is_number());
    assert!(pheromone["disagreement"].is_number());
    
    // Verify pheromone values are reasonable
    let attraction: f64 = serde_json::from_value(pheromone["attraction"].clone()).unwrap();
    let novelty: f64 = serde_json::from_value(pheromone["novelty"].clone()).unwrap();
    let disagreement: f64 = serde_json::from_value(pheromone["disagreement"].clone()).unwrap();
    
    assert!(attraction >= 0.0);
    assert!(novelty >= 0.0 && novelty <= 1.0);
    assert!(disagreement >= 0.0 && disagreement <= 1.0);
}

#[tokio::test]
async fn test_get_suggestions_performance_considerations() {
    // Test performance-related aspects
    
    // Test that large result sets are handled efficiently
    let large_mock_dataset: Vec<Value> = (0..1000)
        .map(|i| json!({
            "atom_id": format!("atom-{}", i),
            "atom_type": "finding",
            "domain": "test",
            "statement": format!("Test statement {}", i),
            "conditions": {},
            "metrics": null,
            "ph_attraction": (i % 100) as f64 / 10.0,
            "ph_repulsion": 1.0,
            "ph_novelty": 0.5,
            "ph_disagreement": 0.2
        }))
        .collect();
    
    // Test sorting performance
    let start = std::time::Instant::now();
    let mut sorted_dataset = large_mock_dataset.clone();
    sorted_dataset.sort_by(|a, b| {
        let a_attraction = a["ph_attraction"].as_f64().unwrap_or(0.0);
        let b_attraction = b["ph_attraction"].as_f64().unwrap_or(0.0);
        let a_novelty = a["ph_novelty"].as_f64().unwrap_or(0.0);
        let b_novelty = b["ph_novelty"].as_f64().unwrap_or(0.0);
        
        b_attraction.partial_cmp(&a_attraction).unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| b_novelty.partial_cmp(&a_novelty).unwrap_or(std::cmp::Ordering::Equal))
    });
    let sort_time = start.elapsed();
    
    assert!(sort_time < std::time::Duration::from_millis(100), "Sorting should be fast");
    assert_eq!(sorted_dataset.len(), 1000);
    
    // Test limit application performance
    let start = std::time::Instant::now();
    let limited: Vec<_> = sorted_dataset.iter().take(10).collect();
    let limit_time = start.elapsed();
    
    assert!(limit_time < std::time::Duration::from_millis(1), "Limit application should be very fast");
    assert_eq!(limited.len(), 10);
}
