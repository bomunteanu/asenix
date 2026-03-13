use ed25519_dalek::{SigningKey, VerifyingKey, Signer, Verifier};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ed25519_sign_and_verify() {
        // Test successful signing and verification
        let signing_key = SigningKey::from_bytes(&[0u8; 32]);
        let public_key = VerifyingKey::from(&signing_key);
        
        let message = b"test message";
        let signature = signing_key.sign(message);
        
        assert!(public_key.verify(message, &signature).is_ok());
    }

    #[test]
    fn test_ed25519_verify_modified_message_fails() {
        // Test that verification fails for modified message
        let signing_key = SigningKey::from_bytes(&[0u8; 32]);
        let public_key = VerifyingKey::from(&signing_key);
        
        let message = b"test message";
        let modified_message = b"modified message";
        let signature = signing_key.sign(message);
        
        assert!(public_key.verify(modified_message, &signature).is_err());
    }

    #[test]
    fn test_ed25519_verify_modified_signature_fails() {
        // Test that verification fails for modified signature
        let signing_key = SigningKey::from_bytes(&[0u8; 32]);
        let public_key = VerifyingKey::from(&signing_key);
        
        let message = b"test message";
        let signature = signing_key.sign(message);
        
        // Modify the signature by creating a new one with different bytes
        let mut signature_bytes = signature.to_bytes();
        signature_bytes[0] ^= 0x01;
        
        // Create a new signature from the modified bytes
        let modified_signature = ed25519_dalek::Signature::from_bytes(&signature_bytes);
        
        assert!(public_key.verify(message, &modified_signature).is_err());
    }

    #[test]
    fn test_ed25519_verify_wrong_key_fails() {
        // Test that verification fails with wrong public key
        let signing_key = SigningKey::from_bytes(&[0u8; 32]);
        let wrong_public_key = VerifyingKey::from_bytes(&[1u8; 32]).unwrap();
        
        let message = b"test message";
        let signature = signing_key.sign(message);
        
        assert!(wrong_public_key.verify(message, &signature).is_err());
    }

    #[test]
    fn test_ed25519_different_keys_different_signatures() {
        // Test that different keys produce different signatures
        let signing_key1 = SigningKey::from_bytes(&[0u8; 32]);
        let signing_key2 = SigningKey::from_bytes(&[1u8; 32]);
        
        let message = b"test message";
        let signature1 = signing_key1.sign(message);
        let signature2 = signing_key2.sign(message);
        
        assert_ne!(signature1.to_bytes(), signature2.to_bytes());
    }

    #[test]
    fn test_ed25519_same_key_same_signature() {
        // Test that same key produces same signature for same message
        let signing_key = SigningKey::from_bytes(&[0u8; 32]);
        
        let message = b"test message";
        let signature1 = signing_key.sign(message);
        let signature2 = signing_key.sign(message);
        
        assert_eq!(signature1.to_bytes(), signature2.to_bytes());
    }

    #[test]
    fn test_ed25519_verify_empty_message() {
        // Test signing and verification of empty message
        let signing_key = SigningKey::from_bytes(&[0u8; 32]);
        let public_key = VerifyingKey::from(&signing_key);
        
        let message = b"";
        let signature = signing_key.sign(message);
        
        assert!(public_key.verify(message, &signature).is_ok());
    }

    #[test]
    fn test_ed25519_verify_long_message() {
        // Test signing and verification of long message
        let signing_key = SigningKey::from_bytes(&[0u8; 32]);
        let public_key = VerifyingKey::from(&signing_key);
        
        let message = vec![0u8; 10000]; // 10KB message
        let signature = signing_key.sign(&message);
        
        assert!(public_key.verify(&message, &signature).is_ok());
    }

    #[test]
    fn test_ed25519_public_key_from_bytes() {
        // Test creating public key from bytes
        let signing_key = SigningKey::from_bytes(&[0u8; 32]);
        let public_key1 = VerifyingKey::from(&signing_key);
        let public_key2 = VerifyingKey::from_bytes(&public_key1.as_bytes()).unwrap();
        
        assert_eq!(public_key1.as_bytes(), public_key2.as_bytes());
    }

    #[test]
    fn test_ed25519_invalid_public_key_bytes() {
        // Test that invalid public key bytes fail
        // Use a point that's not on the curve - this should fail
        let invalid_bytes = [
            0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
            0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
            0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
            0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFE
        ];
        let result = VerifyingKey::from_bytes(&invalid_bytes);
        assert!(result.is_err());
    }
}
