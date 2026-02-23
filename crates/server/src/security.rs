use crate::error::{AppError, AppResult};
use argon2::Argon2;
use argon2::password_hash::rand_core::OsRng;
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use rand::RngCore;
use sha2::{Digest, Sha256};

pub fn generate_api_key() -> String {
    let mut bytes = [0_u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    format!("rsk_{}", URL_SAFE_NO_PAD.encode(bytes))
}

pub fn fingerprint_api_key(api_key: &str) -> String {
    let digest = Sha256::digest(api_key.as_bytes());
    hex::encode(digest)
}

pub fn hash_api_key(api_key: &str) -> AppResult<String> {
    let salt = SaltString::generate(&mut OsRng);
    let hash = Argon2::default()
        .hash_password(api_key.as_bytes(), &salt)?
        .to_string();
    Ok(hash)
}

pub fn verify_api_key(api_key: &str, password_hash: &str) -> AppResult<bool> {
    let parsed_hash = PasswordHash::new(password_hash)?;
    Ok(Argon2::default()
        .verify_password(api_key.as_bytes(), &parsed_hash)
        .is_ok())
}

pub fn decode_base64_field(input: &str, field_name: &str) -> AppResult<Vec<u8>> {
    URL_SAFE_NO_PAD
        .decode(input)
        .or_else(|_| base64::engine::general_purpose::STANDARD.decode(input))
        .map_err(|_| AppError::BadRequest(format!("{field_name} must be valid base64")))
}
