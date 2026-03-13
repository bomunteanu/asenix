use serde_json::json;
use std::path::PathBuf;
use mote::config::Config;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_json_parsing() {
        let data = json!({
            "name": "test",
            "value": 42
        });
        
        assert_eq!(data["name"], "test");
        assert_eq!(data["value"], 42);
    }

    #[test]
    fn test_hex_encoding() {
        let data = vec![0x12, 0x34, 0x56];
        let hex_string = hex::encode(&data);
        assert_eq!(hex_string, "123456");
    }

    #[test]
    fn test_ed25519_basic() {
        use ed25519_dalek::{SigningKey, VerifyingKey, Signer, Verifier};
        
        let signing_key = SigningKey::from_bytes(&[0u8; 32]);
        let public_key = VerifyingKey::from(&signing_key);
        
        let message = b"test message";
        let signature = signing_key.sign(message);
        
        assert!(public_key.verify(message, &signature).is_ok());
    }

    #[test]
    fn test_blake3_hashing() {
        use blake3::Hasher;
        
        let mut hasher = Hasher::new();
        hasher.update(b"test message");
        let hash = hasher.finalize();
        
        assert_eq!(hash.as_bytes().len(), 32);
    }

    #[test]
    fn test_config_parsing_valid_complete() {
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

    #[test]
    fn test_config_parsing_invalid_value_type() {
        let config_path = PathBuf::from("tests/invalid_config.toml");
        let config = Config::load_from_file(&config_path).unwrap();
        
        // Should load but validation should fail
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_config_validation_invalid_values() {
        let config_path = PathBuf::from("tests/invalid_config.toml");
        let config = Config::load_from_file(&config_path).unwrap();
        
        // Validation should fail because embedding_dimension is 0
        let result = config.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("embedding_dimension must be > 0"));
    }

    #[test]
    fn test_config_total_embedding_dimension() {
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
        let value1: f64 = 25.0;
        let value2: f64 = 25.005;
        let tolerance: f64 = 0.01;
        
        assert!((value1 - value2).abs() <= tolerance);
    }
}
