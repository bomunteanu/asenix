use crate::error::{MoteError, Result};
use ed25519_dalek::{VerifyingKey, Signature, Verifier};

pub fn verify_signature(public_key: &[u8], message: &[u8], signature: &[u8]) -> Result<bool> {
    let public_key = VerifyingKey::from_bytes(public_key.try_into().unwrap())
        .map_err(|e| MoteError::Cryptography(format!("Invalid public key: {}", e)))?;
    
    if signature.len() != 64 {
        return Err(MoteError::Cryptography("Invalid signature length".to_string()));
    }
    
    let mut sig_bytes = [0u8; 64];
    sig_bytes.copy_from_slice(signature);
    let signature = Signature::from_bytes(&sig_bytes);
    
    public_key.verify(message, &signature)
        .map_err(|e| MoteError::Cryptography(format!("Signature verification failed: {}", e)))?;
    
    Ok(true)
}

pub fn generate_challenge() -> Vec<u8> {
    use rand::RngCore;
    let mut challenge = [0u8; 32];
    rand::rng().fill_bytes(&mut challenge);
    challenge.to_vec()
}

pub fn hex_to_bytes(hex: &str) -> Result<Vec<u8>> {
    hex::decode(hex).map_err(|e| MoteError::Cryptography(format!("Invalid hex: {}", e)))
}

pub fn bytes_to_hex(bytes: &[u8]) -> String {
    hex::encode(bytes)
}
