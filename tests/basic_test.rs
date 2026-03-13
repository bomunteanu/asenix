#[cfg(test)]
mod tests {
    #[test]
    fn test_basic_functionality() {
        // Test basic JSON functionality
        let data = serde_json::json!({
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
        
        let decoded = hex::decode(&hex_string).unwrap();
        assert_eq!(decoded, data);
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
    fn test_ed25519_basic() {
        use ed25519_dalek::{SigningKey, VerifyingKey, Signer, Verifier};
        
        
        let signing_key = SigningKey::from_bytes(&[0u8; 32]);
        let public_key = VerifyingKey::from(&signing_key);
        
        let message = b"test message";
        let signature = signing_key.sign(message);
        
        assert!(public_key.verify(message, &signature).is_ok());
    }
}
