//! Encrypted secret helpers for desktop persistence.
//!
//! Settings and profiles are ordinary JSON documents, but API keys must not be
//! stored as readable JSON values. This module encrypts secret strings in-place
//! before serialization and decrypts them after loading.
//!
//! Refs: I-Shell-Runtime-OnlyIO

use std::num::NonZeroU32;

use base64::Engine;
use ring::aead::{AES_256_GCM, Aad, LessSafeKey, Nonce, UnboundKey};
use ring::pbkdf2::{PBKDF2_HMAC_SHA256, derive};
use ring::rand::{SecureRandom, SystemRandom};

const SECRET_PREFIX: &str = "brioche-secret:v1:";
const PBKDF2_ITERATIONS: u32 = 120_000;
const KEY_LEN: usize = 32;
const NONCE_LEN: usize = 12;
const SALT: &[u8] = b"brioche-shell-persistence-secret-v1";

/// Returns true when a string is an encrypted Brioche secret marker.
///
/// Refs: I-Shell-Runtime-OnlyIO
pub fn is_protected_secret(value: &str) -> bool {
    value.starts_with(SECRET_PREFIX)
}

/// Encrypts a secret for storage in JSON configuration files.
///
/// Empty strings stay empty so unset keys do not produce secret markers. Values
/// already protected by this module are returned unchanged.
///
/// Refs: I-Shell-Runtime-OnlyIO
///
/// # Complexity
/// O(N) where N is secret length. Allocates one ciphertext buffer.
///
/// # Errors
/// Returns an error if key derivation, random nonce generation, or encryption fails.
pub fn protect_secret(value: &str) -> Result<String, String> {
    if value.is_empty() || is_protected_secret(value) {
        return Ok(value.to_string());
    }

    let key = encryption_key()?;
    let rng = SystemRandom::new();
    let mut nonce_bytes = [0u8; NONCE_LEN];
    rng.fill(&mut nonce_bytes)
        .map_err(|_| "failed to generate secret nonce".to_string())?;

    let unbound = UnboundKey::new(&AES_256_GCM, &key)
        .map_err(|_| "failed to initialize secret cipher".to_string())?;
    let sealing_key = LessSafeKey::new(unbound);
    let nonce = Nonce::assume_unique_for_key(nonce_bytes);
    let mut sealed = value.as_bytes().to_vec();
    sealing_key
        .seal_in_place_append_tag(nonce, Aad::empty(), &mut sealed)
        .map_err(|_| "failed to encrypt secret".to_string())?;

    let mut payload = Vec::with_capacity(NONCE_LEN + sealed.len());
    payload.extend_from_slice(&nonce_bytes);
    payload.extend_from_slice(&sealed);
    Ok(format!(
        "{SECRET_PREFIX}{}",
        base64::engine::general_purpose::STANDARD_NO_PAD.encode(payload)
    ))
}

/// Decrypts a protected secret, or returns a legacy plaintext value unchanged.
///
/// Returning legacy plaintext lets older settings migrate on their next save.
///
/// Refs: I-Shell-Runtime-OnlyIO
///
/// # Complexity
/// O(N) where N is stored value length. Allocates one plaintext buffer.
///
/// # Errors
/// Returns an error if a protected value is malformed or cannot be decrypted.
pub fn reveal_secret(value: &str) -> Result<String, String> {
    if !is_protected_secret(value) {
        return Ok(value.to_string());
    }

    let encoded = value.trim_start_matches(SECRET_PREFIX);
    let payload = base64::engine::general_purpose::STANDARD_NO_PAD
        .decode(encoded)
        .map_err(|_| "protected secret is not valid base64".to_string())?;
    if payload.len() <= NONCE_LEN {
        return Err("protected secret payload is too short".into());
    }

    let mut nonce_bytes = [0u8; NONCE_LEN];
    nonce_bytes.copy_from_slice(&payload[..NONCE_LEN]);
    let mut sealed = payload[NONCE_LEN..].to_vec();

    let key = encryption_key()?;
    let unbound = UnboundKey::new(&AES_256_GCM, &key)
        .map_err(|_| "failed to initialize secret cipher".to_string())?;
    let opening_key = LessSafeKey::new(unbound);
    let nonce = Nonce::assume_unique_for_key(nonce_bytes);
    let plaintext = opening_key
        .open_in_place(nonce, Aad::empty(), &mut sealed)
        .map_err(|_| "failed to decrypt protected secret".to_string())?;
    String::from_utf8(plaintext.to_vec())
        .map_err(|_| "protected secret is not valid UTF-8".to_string())
}

fn encryption_key() -> Result<[u8; KEY_LEN], String> {
    let passphrase = secret_passphrase();
    if passphrase.is_empty() {
        return Err("secret key material is empty".into());
    }

    let iterations = NonZeroU32::new(PBKDF2_ITERATIONS)
        .ok_or_else(|| "invalid PBKDF2 iteration count".to_string())?;
    let mut key = [0u8; KEY_LEN];
    derive(
        PBKDF2_HMAC_SHA256,
        iterations,
        SALT,
        passphrase.as_bytes(),
        &mut key,
    );
    Ok(key)
}

fn secret_passphrase() -> String {
    if let Ok(value) = std::env::var("BRIOCHE_SECRETS_KEY")
        && !value.is_empty()
    {
        return value;
    }

    let user = env_or_fallback("USER", "USERNAME", "unknown-user");
    let home = env_or_fallback("HOME", "USERPROFILE", "unknown-home");
    let host = env_or_fallback("HOSTNAME", "COMPUTERNAME", "unknown-host");
    format!("brioche-headless-fallback:{user}:{home}:{host}")
}

fn env_or_fallback(primary: &str, secondary: &str, fallback: &str) -> String {
    match std::env::var(primary).or_else(|_| std::env::var(secondary)) {
        Ok(value) => value,
        Err(_) => fallback.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protect_secret_removes_plaintext_from_marker() -> Result<(), String> {
        let protected = protect_secret("sk-test-secret")?;
        assert!(is_protected_secret(&protected));
        assert!(
            !protected.contains("sk-test-secret"),
            "protected marker must not contain plaintext"
        );
        assert_eq!(reveal_secret(&protected)?, "sk-test-secret");
        Ok(())
    }

    #[test]
    fn reveal_secret_preserves_legacy_plaintext() -> Result<(), String> {
        assert_eq!(reveal_secret("legacy-key")?, "legacy-key");
        Ok(())
    }
}
