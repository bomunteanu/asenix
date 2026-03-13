use serde_json::json;
use std::path::PathBuf;
use mote::config::Config;

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_config_parsing_valid_complete() {
        let config_path = PathBuf::from("tests/test_config.toml");
        let config = Config::load_from_file(&config_path).unwrap();
        
        assert_eq!(config.hub.name, "test-hub");
        assert_eq!(config.hub.domain, "test.mote");
        assert_eq!(config.hub.embedding_dimension, 768);
        assert_eq!(config.hub.structured_vector_reserved_dims, 10);
        assert_eq!(config.hub.dims_per_numeric_key, 2);
        assert_eq!(config.hub.dims_per_categorical_key, 1);
        assert_eq!(config.pheromone.decay_half_life_hours, 24);
        assert_eq!(config.trust.reliability_threshold, 0.7);
        assert_eq!(config.workers.embedding_pool_size, 4);
        assert_eq!(config.acceptance.required_provenance_fields, vec!["agent_id", "timestamp"]);
    }

    #[tokio::test]
    async fn test_config_parsing_missing_required_field() {
        let toml = r#"
[hub]
name = "test-hub"
# Missing domain field
listen_address = "127.0.0.1:3000"
"#;

        let config_path = PathBuf::from("test_missing_field.toml");
        std::fs::write(&config_path, toml).unwrap();
        
        let result = Config::load_from_file(&config_path);
        assert!(result.is_err(), "Should fail when required field is missing");

        std::fs::remove_file(&config_path).unwrap();
    }

    #[tokio::test]
    async fn test_config_parsing_invalid_value_type() {
        let toml = r#"
[hub]
name = "test-hub"
domain = "test"
listen_address = "127.0.0.1:3000"
embedding_dimension = "not_a_number"  # Should be integer
"#;

        let config_path = PathBuf::from("test_invalid_type.toml");
        std::fs::write(&config_path, toml).unwrap();
        
        let result = Config::load_from_file(&config_path);
        assert!(result.is_err(), "Should fail with invalid value type");

        std::fs::remove_file(&config_path).unwrap();
    }

    #[tokio::test]
    async fn test_config_validation_invalid_values() {
        let toml = r#"
[hub]
name = "test-hub"
domain = "test"
listen_address = "127.0.0.1:3000"
embedding_endpoint = "http://localhost:8080/embed"
embedding_model = "text-embedding-ada-002"
embedding_dimension = 0  # Invalid: must be > 0
structured_vector_reserved_dims = 256
dims_per_numeric_key = 4
dims_per_categorical_key = 16
neighbourhood_radius = 0.3

[pheromone]
decay_half_life_hours = 168
attraction_cap = 100.0
novelty_radius = 0.3
disagreement_threshold = 0.5

[trust]
reliability_threshold = 1.5  # Invalid: must be between 0.0 and 1.0
independence_ancestry_depth = 5
probation_atom_count = 10
max_atoms_per_hour = 1000

[workers]
embedding_pool_size = 64  # Invalid: must be <= 32
decay_interval_minutes = 30
claim_ttl_hours = 72
staleness_check_interval_minutes = 15

[acceptance]
required_provenance_fields = ["agent_id", "timestamp"]
"#;

        let config_path = PathBuf::from("test_invalid_values.toml");
        std::fs::write(&config_path, toml).unwrap();
        
        let config = Config::load_from_file(&config_path).unwrap();
        let result = config.validate();
        assert!(result.is_err(), "Should fail validation with invalid values");

        std::fs::remove_file(&config_path).unwrap();
    }

    #[tokio::test]
    async fn test_config_total_embedding_dimension() {
        let config_path = PathBuf::from("tests/test_config.toml");
        let config = Config::load_from_file(&config_path).unwrap();
        
        let total = config.total_embedding_dimension();
        assert_eq!(total, 778); // 768 + 10
    }

    #[test]
    fn test_condition_equivalence() {
        let conditions1 = json!({
            "temperature": 25.0,
            "pressure": 101.3
        });

        let conditions2 = json!({
            "temperature": 25.0,
            "pressure": 101.3
        });

        assert_eq!(conditions1, conditions2);
    }

    #[test]
    fn test_condition_tolerance() {
        let conditions1 = json!({
            "measurement_error": 1e10,
            "other_field": "value"
        });

        let conditions2 = json!({
            "measurement_error": 1e10 + 1.0,
            "other_field": "value"
        });

        assert_ne!(conditions1, conditions2);
    }
}
