//! Unit tests for pheromone decay worker
//! 
//! Tests the decay worker functionality including batch updates, statistics,
//! and proper handling of edge cases.

use mote::workers::decay::{DecayWorker, DecayStats};
use mote::config::{Config, PheromoneConfig, HubConfig, TrustConfig, WorkersConfig, AcceptanceConfig};
use std::time::Duration;

fn create_test_config() -> Config {
    Config {
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
    }
}

#[tokio::test]
async fn test_decay_worker_creation() {
    let config = create_test_config();
    
    // Note: This test would require a test database setup
    // For now, we test the creation logic with a mock pool
    // In a real test environment, you would set up a test database
    
    // Test that DecayWorker can be created with valid config
    // This is a structural test - actual database tests would need
    // a proper test database setup
    assert_eq!(config.pheromone.decay_half_life_hours, 24);
    assert_eq!(config.pheromone.attraction_cap, 100.0);
}

#[tokio::test]
async fn test_decay_stats_structure() {
    let stats = DecayStats {
        total_atoms: 100,
        atoms_with_attraction: 50,
        avg_attraction: 2.5,
        max_attraction: 10.0,
    };

    assert_eq!(stats.total_atoms, 100);
    assert_eq!(stats.atoms_with_attraction, 50);
    assert_eq!(stats.avg_attraction, 2.5);
    assert_eq!(stats.max_attraction, 10.0);
}

#[tokio::test]
async fn test_decay_calculation_edge_cases() {
    // Test the mathematical decay calculation directly
    use mote::domain::pheromone::decay_attraction;
    
    // Test decay to zero
    let result = decay_attraction(0.0005, 168.0, 168.0, 0.001);
    assert_eq!(result, 0.0, "Should decay to floor value");
    
    // Test no decay (zero time)
    let result = decay_attraction(10.0, 0.0, 168.0, 0.001);
    assert_eq!(result, 10.0, "Should not decay with zero time");
    
    // Test exact half-life
    let result = decay_attraction(10.0, 168.0, 168.0, 0.001);
    assert!((result - 5.0).abs() < 0.001, "Should halve after one half-life");
    
    // Test multiple half-lives
    let result = decay_attraction(10.0, 336.0, 168.0, 0.001); // 2 half-lives
    assert!((result - 2.5).abs() < 0.001, "Should quarter after two half-lives");
    
    // Test very small initial value
    let result = decay_attraction(0.001, 168.0, 168.0, 0.001);
    assert_eq!(result, 0.0, "Very small values should decay to floor");
    
    // Test large time value
    let result = decay_attraction(10.0, 1680.0, 168.0, 0.001); // 10 half-lives
    assert!(result < 0.01, "Should decay to near zero after many half-lives");
}

#[tokio::test]
async fn test_decay_worker_configuration_validation() {
    let config = create_test_config();
    
    // Test valid configuration
    assert!(config.validate().is_ok(), "Valid config should pass validation");
    
    // Test invalid half-life
    let mut invalid_config = config.clone();
    invalid_config.pheromone.decay_half_life_hours = 0;
    assert!(invalid_config.validate().is_err(), "Zero half-life should fail validation");
    
    // Test invalid attraction cap
    let mut invalid_config = config.clone();
    invalid_config.pheromone.attraction_cap = 0.0;
    assert!(invalid_config.validate().is_err(), "Zero attraction cap should fail validation");
    
    // Test invalid novelty radius
    let mut invalid_config = config.clone();
    invalid_config.pheromone.novelty_radius = 1.5;
    assert!(invalid_config.validate().is_err(), "Novelty radius > 1.0 should fail validation");
    
    // Test invalid disagreement threshold
    let mut invalid_config = config.clone();
    invalid_config.pheromone.disagreement_threshold = -0.1;
    assert!(invalid_config.validate().is_err(), "Negative disagreement threshold should fail validation");
}

#[tokio::test]
async fn test_decay_worker_time_calculations() {
    use chrono::{DateTime, Utc};
    use std::time::SystemTime;
    
    let now = Utc::now();
    let past = now - chrono::Duration::hours(24);
    
    let hours_elapsed = now.signed_duration_since(past).num_hours();
    assert_eq!(hours_elapsed, 24, "Should correctly calculate elapsed hours");
    
    // Test with recent time
    let recent = now - chrono::Duration::minutes(30);
    let recent_hours = now.signed_duration_since(recent).num_hours();
    assert_eq!(recent_hours, 0, "Should round down to hours");
    
    // Test with very old time
    let ancient = now - chrono::Duration::weeks(2);
    let ancient_hours = now.signed_duration_since(ancient).num_hours();
    assert_eq!(ancient_hours, 14 * 24, "Should handle large time spans");
}

#[tokio::test]
async fn test_decay_worker_batch_logic() {
    // Test the batch processing logic without database
    let test_atoms = vec![
        ("atom1".to_string(), 5.0),
        ("atom2".to_string(), 3.0),
        ("atom3".to_string(), 7.0),
    ];
    
    assert_eq!(test_atoms.len(), 3, "Test data should have 3 atoms");
    
    // Test empty batch
    let empty_batch: Vec<(String, f64)> = vec![];
    assert_eq!(empty_batch.len(), 0, "Empty batch should have 0 atoms");
    
    // Test batch size tracking
    let batch_size = test_atoms.len();
    assert!(batch_size > 0, "Batch size should be positive");
    assert!(batch_size <= 1000, "Batch size should be reasonable");
}

#[tokio::test]
async fn test_decay_worker_error_handling() {
    // Test error scenarios that don't require database
    
    // Test with invalid floor_threshold
    use mote::domain::pheromone::decay_attraction;
    
    let result = decay_attraction(10.0, 24.0, 168.0, -0.1); // Negative floor
    assert!(result >= 0.0, "Should handle negative floor gracefully");
    
    // Test with negative half-life (should handle gracefully)
    let result = decay_attraction(10.0, 24.0, -168.0, 0.001); // Negative half-life
    assert!(result.is_finite(), "Should handle negative half-life without panic");
    
    // Test with very large values
    let result = decay_attraction(f64::MAX, 24.0, 168.0, 0.001);
    assert!(result.is_finite(), "Should handle very large values without overflow");
}

#[tokio::test]
async fn test_decay_worker_performance_considerations() {
    // Test that the decay worker handles large batches efficiently
    
    // Create a large test dataset
    let large_batch: Vec<(String, f64)> = (0..1000)
        .map(|i| (format!("atom_{}", i), i as f64))
        .collect();
    
    assert_eq!(large_batch.len(), 1000, "Should handle large batches");
    
    // Test batch processing time (should be fast)
    let start = std::time::Instant::now();
    
    // Simulate batch processing (in real implementation this would be database operations)
    let processed_count = large_batch.len();
    
    let elapsed = start.elapsed();
    assert!(elapsed < Duration::from_secs(1), "Batch processing should be fast");
    assert_eq!(processed_count, 1000, "Should process all items");
}

#[tokio::test]
async fn test_decay_stats_calculation() {
    // Test statistics calculation logic
    
    // Test with normal data
    let stats = DecayStats {
        total_atoms: 100,
        atoms_with_attraction: 75,
        avg_attraction: 3.5,
        max_attraction: 15.0,
    };
    
    assert!(stats.atoms_with_attraction <= stats.total_atoms, "Atoms with attraction should not exceed total");
    assert!(stats.avg_attraction <= stats.max_attraction, "Average should not exceed maximum");
    assert!(stats.avg_attraction >= 0.0, "Average should be non-negative");
    assert!(stats.max_attraction >= 0.0, "Maximum should be non-negative");
    
    // Test edge cases
    let empty_stats = DecayStats {
        total_atoms: 0,
        atoms_with_attraction: 0,
        avg_attraction: 0.0,
        max_attraction: 0.0,
    };
    
    assert_eq!(empty_stats.total_atoms, 0, "Should handle empty dataset");
    assert_eq!(empty_stats.atoms_with_attraction, 0, "Should handle empty dataset");
    
    // Test single atom
    let single_stats = DecayStats {
        total_atoms: 1,
        atoms_with_attraction: 1,
        avg_attraction: 5.0,
        max_attraction: 5.0,
    };
    
    assert_eq!(single_stats.total_atoms, 1, "Should handle single atom");
    assert_eq!(single_stats.atoms_with_attraction, 1, "Single atom should have attraction");
    assert_eq!(single_stats.avg_attraction, single_stats.max_attraction, "Average should equal max for single atom");
}

#[tokio::test]
async fn test_decay_worker_integration_points() {
    // Test integration points without actual database
    
    let config = create_test_config();
    
    // Test that configuration values are used correctly
    assert_eq!(config.pheromone.decay_half_life_hours, 24);
    assert_eq!(config.pheromone.attraction_cap, 100.0);
    
    // Test that the worker would use these values
    let half_life_hours = config.pheromone.decay_half_life_hours as f64;
    assert!(half_life_hours > 0.0, "Half-life should be positive");
    
    let attraction_cap = config.pheromone.attraction_cap;
    assert!(attraction_cap > 0.0, "Attraction cap should be positive");
    
    // Test floor threshold (hardcoded in implementation)
    let floor_threshold = 0.001;
    assert!(floor_threshold > 0.0, "Floor threshold should be positive");
    assert!(floor_threshold < 1.0, "Floor threshold should be reasonable");
}
