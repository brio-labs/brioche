//! Encrypted-at-rest secret values.
//!
//! The [`Secret`] wrapper stores a plaintext value in memory and serializes it
//! as an AEAD ciphertext so that API keys never appear as plaintext in JSON
//! configuration files.
//!
//! Encryption uses a master key obtained from (in order):
//!
//! 1. The `BRIOCHE_MASTER_KEY` environment variable. The value is run through
//!    Argon2id with a fixed application salt to derive a 256-bit key.
//! 2. The OS keyring (service `brioche-desktop`, username `master-key`). A
//!    random 256-bit key is generated and stored on first use if no entry
//!    exists.
//!
//! If neither source is available, serialization fails loudly rather than
//! falling back to plaintext.
//!
//! Refs: I-Shell-Runtime-OnlyIO

use std::fmt;

use argon2::password_hash::SaltString;
use argon2::{Argon2, PasswordHasher};
use base64::{Engine, engine::general_purpose::STANDARD as BASE64};
use chacha20poly1305::aead::generic_array::typenum::Unsigned;
use chacha20poly1305::{
    AeadCore, ChaCha20Poly1305, Key, KeyInit, Nonce,
    aead::{Aead, OsRng},
};
use rand::RngCore;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Errors that can occur when encrypting or decrypting a secret.
///
/// Refs: I-Shell-Runtime-OnlyIO
#[derive(Clone, Debug, thiserror::Error)]
pub enum SecretError {
    /// No master key source is available.
    #[error("no encryption key available: set BRIOCHE_MASTER_KEY or configure an OS keyring")]
    NoMasterKey,
    /// The master key material is invalid.
    #[error("invalid master key: {0}")]
    InvalidMasterKey(String),
    /// Encryption failed.
    #[error("encryption failed: {0}")]
    EncryptFailed(String),
    /// Decryption failed.
    #[error("decryption failed: {0}")]
    DecryptFailed(String),
}

const KEYRING_SERVICE: &str = "brioche-desktop";
const KEYRING_USERNAME: &str = "master-key";
const ENV_MASTER_KEY: &str = "BRIOCHE_MASTER_KEY";
const ENCRYPTED_PREFIX: &str = "enc:v1:";
const ARGON2_SALT: &str = "brioche-shell-persistence-v1";

/// A plaintext secret that serializes to an encrypted string.
///
/// `Debug` and `Serialize` never emit the plaintext. Use [`Secret::expose`]
/// to access the value when it is actually needed.
///
/// Refs: I-Shell-Runtime-OnlyIO
#[derive(Clone, Default, PartialEq, Eq)]
pub struct Secret {
    plaintext: String,
}

impl Secret {
    /// Creates a secret from the given plaintext.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    pub fn new(plaintext: impl Into<String>) -> Self {
        Self {
            plaintext: plaintext.into(),
        }
    }

    /// Returns the plaintext value.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    pub fn expose(&self) -> &str {
        &self.plaintext
    }

    /// Returns whether the contained secret is empty.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    pub fn is_empty(&self) -> bool {
        self.plaintext.is_empty()
    }

    /// Consumes the secret and returns the plaintext value.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    pub fn into_inner(self) -> String {
        self.plaintext
    }

    /// Encrypts the plaintext with the master key.
    fn encrypt(&self) -> Result<String, SecretError> {
        if self.plaintext.is_empty() {
            return Ok(String::new());
        }
        let key = master_key()?;
        let cipher = ChaCha20Poly1305::new(&key);
        let nonce = ChaCha20Poly1305::generate_nonce(&mut OsRng);
        let ciphertext = cipher
            .encrypt(&nonce, self.plaintext.as_bytes())
            .map_err(|err| SecretError::EncryptFailed(err.to_string()))?;
        let mut blob = Vec::with_capacity(nonce.len() + ciphertext.len());
        blob.extend_from_slice(&nonce);
        blob.extend_from_slice(&ciphertext);
        Ok(format!("{ENCRYPTED_PREFIX}{}", BASE64.encode(&blob)))
    }

    /// Decrypts a value produced by [`Self::encrypt`].
    ///
    /// Strings without the encrypted prefix are treated as plaintext legacy
    /// values and are returned as-is, allowing existing configs to migrate
    /// silently on the next save.
    fn decrypt(ciphertext: &str) -> Result<Self, SecretError> {
        if ciphertext.is_empty() {
            return Ok(Self::default());
        }
        let Some(encoded) = ciphertext.strip_prefix(ENCRYPTED_PREFIX) else {
            return Ok(Self::new(ciphertext));
        };
        let key = master_key()?;
        let cipher = ChaCha20Poly1305::new(&key);
        let blob = BASE64
            .decode(encoded)
            .map_err(|err| SecretError::DecryptFailed(err.to_string()))?;
        let nonce_len = <ChaCha20Poly1305 as AeadCore>::NonceSize::USIZE;
        if blob.len() < nonce_len {
            return Err(SecretError::DecryptFailed("blob too short".into()));
        }
        let (nonce_bytes, enc_bytes) = blob.split_at(nonce_len);
        let nonce = Nonce::from_slice(nonce_bytes);
        let plaintext = cipher
            .decrypt(nonce, enc_bytes)
            .map_err(|err| SecretError::DecryptFailed(err.to_string()))?;
        let plaintext = String::from_utf8(plaintext)
            .map_err(|err| SecretError::DecryptFailed(err.to_string()))?;
        Ok(Self::new(plaintext))
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret([redacted])")
    }
}

impl Serialize for Secret {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.encrypt()
            .map_err(serde::ser::Error::custom)?
            .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Secret {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        Self::decrypt(&value).map_err(serde::de::Error::custom)
    }
}

impl From<String> for Secret {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

impl From<&str> for Secret {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

#[allow(clippy::manual_unwrap_or_default)]
impl From<Option<String>> for Secret {
    fn from(value: Option<String>) -> Self {
        Self::new(if let Some(s) = value { s } else { String::new() })
    }
}

#[cfg(not(test))]
fn master_key() -> Result<Key, SecretError> {
    use std::sync::LazyLock;
    static MASTER_KEY: LazyLock<Result<Key, SecretError>> = LazyLock::new(resolve_master_key);
    MASTER_KEY.clone()
}

#[cfg(test)]
fn master_key() -> Result<Key, SecretError> {
    resolve_master_key()
}

fn resolve_master_key() -> Result<Key, SecretError> {
    if let Ok(password) = std::env::var(ENV_MASTER_KEY) {
        return derive_key_from_password(&password);
    }
    keyring_master_key()
}

fn derive_key_from_password(password: &str) -> Result<Key, SecretError> {
    let salt = SaltString::encode_b64(ARGON2_SALT.as_bytes())
        .map_err(|err| SecretError::InvalidMasterKey(err.to_string()))?;
    let argon2 = Argon2::default();
    let password_hash = argon2
        .hash_password(password.as_bytes(), &salt)
        .map_err(|err| SecretError::InvalidMasterKey(err.to_string()))?;
    let hash = password_hash
        .hash
        .ok_or_else(|| SecretError::InvalidMasterKey("argon2 produced no hash output".into()))?;
    let bytes = hash.as_bytes();
    if bytes.len() != 32 {
        return Err(SecretError::InvalidMasterKey(format!(
            "expected 32-byte key, got {}",
            bytes.len()
        )));
    }
    Ok(*Key::from_slice(bytes))
}

fn keyring_master_key() -> Result<Key, SecretError> {
    let entry = keyring::Entry::new(KEYRING_SERVICE, KEYRING_USERNAME).map_err(|err| {
        SecretError::InvalidMasterKey(format!("failed to open keyring entry: {err}"))
    })?;
    match entry.get_password() {
        Ok(encoded) => decode_key(&encoded),
        Err(keyring::Error::NoEntry) => {
            let mut raw = [0u8; 32];
            OsRng.fill_bytes(&mut raw);
            let encoded = BASE64.encode(raw);
            entry.set_password(&encoded).map_err(|err| {
                SecretError::InvalidMasterKey(format!("failed to store keyring entry: {err}"))
            })?;
            decode_key(&encoded)
        }
        Err(err) => Err(SecretError::InvalidMasterKey(format!(
            "failed to read keyring entry: {err}"
        ))),
    }
}

fn decode_key(encoded: &str) -> Result<Key, SecretError> {
    let bytes = BASE64
        .decode(encoded)
        .map_err(|err| SecretError::InvalidMasterKey(err.to_string()))?;
    if bytes.len() != 32 {
        return Err(SecretError::InvalidMasterKey(format!(
            "expected 32-byte key, got {}",
            bytes.len()
        )));
    }
    Ok(*Key::from_slice(&bytes))
}

/// Replaces plaintext strings at known secret paths with encrypted values.
///
/// Refs: I-Shell-Runtime-OnlyIO
pub(crate) fn encrypt_secret_values(modules: &mut serde_json::Map<String, serde_json::Value>) {
    encrypt_object_path(modules, &["chat", "api_key"]);
    encrypt_array_item_path(modules, &["memory", "endpoints"], &["api_key"]);
    encrypt_array_item_path(modules, &["chat", "fallback_models"], &["api_key"]);
}

fn encrypt_object_path(map: &mut serde_json::Map<String, serde_json::Value>, path: &[&str]) {
    let Some(serde_json::Value::String(plaintext)) = get_mut_path(map, path) else {
        return;
    };
    if plaintext.starts_with(ENCRYPTED_PREFIX) || plaintext.is_empty() {
        return;
    }
    let secret = Secret::new(std::mem::take(plaintext));
    if let Ok(encrypted) = secret.encrypt()
        && let Some(value) = get_mut_path(map, path)
    {
        *value = serde_json::Value::String(encrypted);
    }
}
fn encrypt_array_item_path(
    map: &mut serde_json::Map<String, serde_json::Value>,
    array_path: &[&str],
    field_path: &[&str],
) {
    let Some(serde_json::Value::Array(items)) = get_mut_path(map, array_path) else {
        return;
    };
    for item in items.iter_mut().filter_map(|v| v.as_object_mut()) {
        encrypt_object_path(item, field_path);
    }
}

fn get_mut_path<'a>(
    map: &'a mut serde_json::Map<String, serde_json::Value>,
    path: &[&str],
) -> Option<&'a mut serde_json::Value> {
    if path.is_empty() {
        return None;
    }
    let mut current: &mut serde_json::Value = map.get_mut(path[0])?;
    for part in &path[1..] {
        current = current.get_mut(part)?;
    }
    Some(current)
}

/// Decrypts values produced by [`encrypt_secret_values`] back to plaintext.
///
/// Refs: I-Shell-Runtime-OnlyIO
pub(crate) fn decrypt_secret_values(modules: &mut serde_json::Map<String, serde_json::Value>) {
    decrypt_object_path(modules, &["chat", "api_key"]);
    decrypt_array_item_path(modules, &["memory", "endpoints"], &["api_key"]);
    decrypt_array_item_path(modules, &["chat", "fallback_models"], &["api_key"]);
}

fn decrypt_object_path(map: &mut serde_json::Map<String, serde_json::Value>, path: &[&str]) {
    let Some(serde_json::Value::String(encrypted)) = get_mut_path(map, path) else {
        return;
    };
    if let Ok(secret) = Secret::decrypt(encrypted)
        && let Some(value) = get_mut_path(map, path)
    {
        *value = serde_json::Value::String(secret.into_inner());
    }
}

fn decrypt_array_item_path(
    map: &mut serde_json::Map<String, serde_json::Value>,
    array_path: &[&str],
    field_path: &[&str],
) {
    let Some(serde_json::Value::Array(items)) = get_mut_path(map, array_path) else {
        return;
    };
    for item in items.iter_mut().filter_map(|v| v.as_object_mut()) {
        decrypt_object_path(item, field_path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with_test_key<F, R>(f: F) -> R
    where
        F: FnOnce() -> R,
    {
        let original = std::env::var(ENV_MASTER_KEY).ok();
        unsafe {
            std::env::set_var(ENV_MASTER_KEY, "test-master-key-for-ci-only");
        }
        let result = f();
        match original {
            Some(v) => unsafe { std::env::set_var(ENV_MASTER_KEY, v) },
            None => unsafe { std::env::remove_var(ENV_MASTER_KEY) },
        }
        result
    }

    #[test]
    fn secret_round_trip() -> Result<(), Box<dyn std::error::Error>> {
        with_test_key(|| {
            let secret = Secret::new("my-super-secret-key");
            let json = serde_json::to_string(&secret)?;
            let recovered: Secret = serde_json::from_str(&json)?;
            assert_eq!(secret, recovered);
            Ok(())
        })
    }

    #[test]
    fn secret_debug_redacts() {
        let secret = Secret::new("my-super-secret-key");
        let formatted = format!("{secret:?}");
        assert!(!formatted.contains("my-super-secret-key"));
        assert!(formatted.contains("[redacted]"));
    }

    #[test]
    fn secret_serialization_hides_plaintext() -> Result<(), Box<dyn std::error::Error>> {
        with_test_key(|| {
            let secret = Secret::new("sk-openrouter-private-token");
            let json = serde_json::to_string(&secret)?;
            assert!(!json.contains("sk-openrouter-private-token"));
            assert!(json.contains(ENCRYPTED_PREFIX));
            Ok(())
        })
    }

    #[test]
    fn secret_decrypts_legacy_plaintext() -> Result<(), Box<dyn std::error::Error>> {
        with_test_key(|| {
            let recovered: Secret = serde_json::from_str("\"plain-legacy-key\"")?;
            assert_eq!(recovered.expose(), "plain-legacy-key");
            Ok(())
        })
    }

    #[test]
    fn empty_secret_serializes_to_empty_string() -> Result<(), Box<dyn std::error::Error>> {
        with_test_key(|| {
            let secret = Secret::default();
            let json = serde_json::to_string(&secret)?;
            assert_eq!(json, "\"\"");
            Ok(())
        })
    }
}
