use crate::error::CoreError;
use aes_gcm::{
    Aes256Gcm, Key, Nonce,
    aead::{Aead, AeadCore, KeyInit},
};
use rand::{RngCore, thread_rng};
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::{ErrorKind, Read};
use std::path::Path;

/// Generates a random 32-byte key for AES-256-GCM.
pub fn generate_key() -> [u8; 32] {
    let mut key = [0u8; 32];
    thread_rng().fill_bytes(&mut key);
    key
}

/// Encrypts a buffer using AES-256-GCM.
/// Returns a Vec<u8> containing the 12-byte nonce followed by the ciphertext.
pub fn encrypt(data: &[u8], key: &[u8; 32]) -> Result<Vec<u8>, CoreError> {
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key));
    let nonce = Aes256Gcm::generate_nonce(&mut thread_rng());

    let ciphertext = cipher
        .encrypt(&nonce, data)
        .map_err(|e| CoreError::Crypto(e.to_string()))?;

    let mut result = Vec::with_capacity(nonce.len() + ciphertext.len());
    result.extend_from_slice(&nonce);
    result.extend_from_slice(&ciphertext);
    Ok(result)
}

/// Decrypts a buffer using AES-256-GCM.
/// Expects the first 12 bytes to be the nonce.
pub fn decrypt(encrypted_data: &[u8], key: &[u8; 32]) -> Result<Vec<u8>, CoreError> {
    if encrypted_data.len() < 12 {
        return Err(CoreError::Crypto("Encrypted data too short".to_string()));
    }

    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key));
    let (nonce_bytes, ciphertext) = encrypted_data.split_at(12);
    let nonce = Nonce::from_slice(nonce_bytes);

    cipher
        .decrypt(nonce, ciphertext)
        .map_err(|e| CoreError::Crypto(e.to_string()))
}

/// Calculates the SHA-256 checksum of a file.
pub fn calculate_checksum(path: &Path) -> Result<String, CoreError> {
    let path_string = path.to_string_lossy().into_owned();
    let mut file = File::open(path).map_err(|source| match source.kind() {
        ErrorKind::NotFound => CoreError::FileNotFound {
            path: path_string.clone(),
        },
        _ => CoreError::Io {
            path: path_string.clone(),
            source,
        },
    })?;

    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 8192];

    loop {
        let count = file.read(&mut buffer).map_err(|source| CoreError::Io {
            path: path_string.clone(),
            source,
        })?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }

    Ok(format!("{:x}", hasher.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Write;
    use uuid::Uuid;

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        let key = generate_key();
        let data = b"Secret message";

        let encrypted = encrypt(data, &key).expect("Encryption failed");
        let decrypted = decrypt(&encrypted, &key).expect("Decryption failed");

        assert_eq!(data, &decrypted[..]);
    }

    #[test]
    fn test_checksum_deterministic() {
        let path = std::env::temp_dir().join(format!("rustsync-checksum-{}.txt", Uuid::new_v4()));
        let mut file = fs::File::create(&path).unwrap();
        writeln!(file, "Hello RustSync").unwrap();
        drop(file);

        let checksum1 = calculate_checksum(&path).unwrap();
        let checksum2 = calculate_checksum(&path).unwrap();

        assert_eq!(checksum1, checksum2);
        assert_eq!(checksum1.len(), 64); // SHA-256 hex length

        fs::remove_file(path).unwrap();
    }

    #[test]
    fn test_checksum_missing_file_returns_not_found() {
        let path = std::env::temp_dir().join(format!("rustsync-missing-{}.txt", Uuid::new_v4()));
        let err = calculate_checksum(&path).unwrap_err();

        assert!(matches!(err, CoreError::FileNotFound { .. }));
    }
}
