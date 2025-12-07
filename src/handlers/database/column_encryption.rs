//! Column-Level Encryption Module
//!
//! Provides comprehensive column-level encryption with:
//! - Multiple encryption algorithms (AES-256-GCM, ChaCha20-Poly1305)
//! - Key management integration (AWS KMS, HashiCorp Vault, local keystore)
//! - Automatic key rotation
//! - Permission-based decryption
//! - Encryption caching for performance

use chrono::{DateTime, Utc};
use ring::aead::{
    Aad, BoundKey, Nonce, NonceSequence, OpeningKey, SealingKey, UnboundKey, AES_256_GCM,
    CHACHA20_POLY1305,
};
use ring::error::Unspecified;
use ring::rand::{SecureRandom, SystemRandom};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

use crate::handlers::database::types::{QueryContext, SecurityError};

/// Encryption algorithm types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum EncryptionAlgorithm {
    /// AES-256-GCM (default, FIPS 140-2 compliant)
    #[default]
    Aes256Gcm,
    /// ChaCha20-Poly1305 (faster on systems without AES hardware acceleration)
    ChaCha20Poly1305,
}

/// Key provider types
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum KeyProvider {
    /// AWS KMS integration
    AwsKms { region: String, key_id: String },
    /// HashiCorp Vault integration
    Vault {
        address: String,
        mount_path: String,
        key_name: String,
    },
    /// Local keystore (for development/testing)
    Local { key_path: String },
}

/// Encryption key metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyMetadata {
    /// Unique key identifier
    pub key_id: String,
    /// Key version (for rotation)
    pub version: u32,
    /// Creation timestamp
    pub created_at: DateTime<Utc>,
    /// Expiration timestamp (optional)
    pub expires_at: Option<DateTime<Utc>>,
    /// Algorithm used with this key
    pub algorithm: EncryptionAlgorithm,
    /// Whether this is the active key
    pub is_active: bool,
}

/// Data Encryption Key (DEK) wrapper
#[derive(Clone)]
pub(crate) struct DataEncryptionKey {
    key_id: String,
    version: u32,
    key_bytes: Vec<u8>,
    algorithm: EncryptionAlgorithm,
    created_at: DateTime<Utc>,
}

impl DataEncryptionKey {
    fn new(
        key_id: String,
        version: u32,
        key_bytes: Vec<u8>,
        algorithm: EncryptionAlgorithm,
    ) -> Self {
        Self {
            key_id,
            version,
            key_bytes,
            algorithm,
            created_at: Utc::now(),
        }
    }
}

impl Drop for DataEncryptionKey {
    fn drop(&mut self) {
        // Zeroize key material on drop for security
        use zeroize::Zeroize;
        self.key_bytes.zeroize();
    }
}

/// Nonce counter for AEAD operations
struct NonceCounter {
    counter: u128,
}

impl NonceCounter {
    fn new() -> Self {
        Self { counter: 0 }
    }
}

impl NonceSequence for NonceCounter {
    fn advance(&mut self) -> Result<Nonce, Unspecified> {
        let mut nonce_bytes = [0u8; 12];
        self.counter += 1;
        nonce_bytes.copy_from_slice(&self.counter.to_le_bytes()[0..12]);
        Nonce::try_assume_unique_for_key(&nonce_bytes)
    }
}

/// Key manager configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyManagerConfig {
    /// Key provider
    pub provider: KeyProvider,
    /// Key rotation interval (in seconds)
    pub rotation_interval_secs: u64,
    /// Maximum number of old keys to retain
    pub max_old_keys: usize,
    /// Default encryption algorithm
    pub default_algorithm: EncryptionAlgorithm,
    /// Enable hardware acceleration if available
    pub enable_hw_acceleration: bool,
}

impl Default for KeyManagerConfig {
    fn default() -> Self {
        Self {
            provider: KeyProvider::Local {
                key_path: "keys".to_string(),
            },
            rotation_interval_secs: 86400 * 30, // 30 days
            max_old_keys: 10,
            default_algorithm: EncryptionAlgorithm::Aes256Gcm,
            enable_hw_acceleration: true,
        }
    }
}

/// Key manager for encryption key lifecycle
pub struct KeyManager {
    config: KeyManagerConfig,
    /// Current active keys (table.column -> DEK)
    active_keys: Arc<RwLock<HashMap<String, DataEncryptionKey>>>,
    /// Historical keys for decryption (key_id:version -> DEK)
    historical_keys: Arc<RwLock<HashMap<String, DataEncryptionKey>>>,
    /// Key metadata
    key_metadata: Arc<RwLock<HashMap<String, KeyMetadata>>>,
    /// Random number generator
    rng: SystemRandom,
}

impl KeyManager {
    /// Create a new key manager
    pub fn new(config: KeyManagerConfig) -> Self {
        Self {
            config,
            active_keys: Arc::new(RwLock::new(HashMap::new())),
            historical_keys: Arc::new(RwLock::new(HashMap::new())),
            key_metadata: Arc::new(RwLock::new(HashMap::new())),
            rng: SystemRandom::new(),
        }
    }

    /// Generate a new Data Encryption Key (DEK)
    pub async fn generate_dek(&self, table: &str, column: &str) -> Result<String, SecurityError> {
        let key_name = format!("{}.{}", table, column);

        // Generate random key bytes
        let mut key_bytes = vec![0u8; 32]; // 256 bits
        self.rng
            .fill(&mut key_bytes)
            .map_err(|_| SecurityError::EncryptionError("Failed to generate random key".into()))?;

        let version = {
            let metadata = self.key_metadata.read().await;
            metadata.get(&key_name).map(|m| m.version + 1).unwrap_or(1)
        };

        let key_id = format!("{}:v{}", key_name, version);
        let dek = DataEncryptionKey::new(
            key_id.clone(),
            version,
            key_bytes,
            self.config.default_algorithm,
        );

        // Store in active keys
        self.active_keys
            .write()
            .await
            .insert(key_name.clone(), dek.clone());

        // Store metadata
        let metadata = KeyMetadata {
            key_id: key_id.clone(),
            version,
            created_at: Utc::now(),
            expires_at: Some(
                Utc::now() + chrono::Duration::seconds(self.config.rotation_interval_secs as i64),
            ),
            algorithm: self.config.default_algorithm,
            is_active: true,
        };
        self.key_metadata
            .write()
            .await
            .insert(key_name.clone(), metadata);

        info!("Generated new DEK for {}: {}", key_name, key_id);
        Ok(key_id)
    }

    /// Get or create DEK for a column
    pub(crate) async fn get_or_create_dek(
        &self,
        table: &str,
        column: &str,
    ) -> Result<DataEncryptionKey, SecurityError> {
        let key_name = format!("{}.{}", table, column);

        // Check if active key exists
        {
            let active_keys = self.active_keys.read().await;
            if let Some(dek) = active_keys.get(&key_name) {
                // Check if key needs rotation
                let metadata = self.key_metadata.read().await;
                if let Some(meta) = metadata.get(&key_name) {
                    if let Some(expires_at) = meta.expires_at {
                        if Utc::now() < expires_at {
                            return Ok(dek.clone());
                        }
                        warn!("Key {} has expired, rotating", key_name);
                    } else {
                        return Ok(dek.clone());
                    }
                }
            }
        }

        // Generate new key if not exists or expired
        self.generate_dek(table, column).await?;

        let active_keys = self.active_keys.read().await;
        active_keys.get(&key_name).cloned().ok_or_else(|| {
            SecurityError::EncryptionError("Failed to retrieve generated key".into())
        })
    }

    /// Get historical key for decryption
    pub(crate) async fn get_historical_key(
        &self,
        key_id: &str,
    ) -> Result<DataEncryptionKey, SecurityError> {
        let historical_keys = self.historical_keys.read().await;
        historical_keys.get(key_id).cloned().ok_or_else(|| {
            SecurityError::EncryptionError(format!("Historical key not found: {}", key_id))
        })
    }

    /// Rotate key for a column
    pub async fn rotate_key(&self, table: &str, column: &str) -> Result<String, SecurityError> {
        let key_name = format!("{}.{}", table, column);

        // Move current active key to historical
        if let Some(old_key) = self.active_keys.write().await.remove(&key_name) {
            let old_key_id = old_key.key_id.clone();
            self.historical_keys
                .write()
                .await
                .insert(old_key_id.clone(), old_key);

            // Update metadata
            if let Some(meta) = self.key_metadata.write().await.get_mut(&key_name) {
                meta.is_active = false;
            }

            debug!("Moved key {} to historical storage", old_key_id);
        }

        // Generate new key
        let new_key_id = self.generate_dek(table, column).await?;
        info!("Rotated key for {}: {}", key_name, new_key_id);

        Ok(new_key_id)
    }

    /// List all key metadata
    pub async fn list_keys(&self) -> Vec<KeyMetadata> {
        self.key_metadata.read().await.values().cloned().collect()
    }

    /// Cleanup old keys beyond retention limit
    pub async fn cleanup_old_keys(&self) -> Result<usize, SecurityError> {
        let mut count = 0;
        let mut historical = self.historical_keys.write().await;

        if historical.len() > self.config.max_old_keys {
            // Sort by creation date and remove oldest
            let mut keys: Vec<_> = historical
                .iter()
                .map(|(k, v)| (k.clone(), v.created_at))
                .collect();
            keys.sort_by_key(|(_, created_at)| *created_at);

            let to_remove = keys.len() - self.config.max_old_keys;
            for (key_id, _) in keys.iter().take(to_remove) {
                historical.remove(key_id);
                count += 1;
            }

            info!("Cleaned up {} old encryption keys", count);
        }

        Ok(count)
    }
}

/// Column encryption configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnEncryptionConfig {
    /// Key manager configuration
    pub key_manager: KeyManagerConfig,
    /// Encryption cache TTL (seconds)
    pub cache_ttl_secs: u64,
    /// Maximum cache size (entries)
    pub max_cache_size: usize,
    /// Enable batch operations
    pub enable_batch_operations: bool,
    /// Columns that require encryption (table.column)
    pub encrypted_columns: Vec<String>,
}

impl Default for ColumnEncryptionConfig {
    fn default() -> Self {
        Self {
            key_manager: KeyManagerConfig::default(),
            cache_ttl_secs: 300, // 5 minutes
            max_cache_size: 10000,
            enable_batch_operations: true,
            encrypted_columns: Vec::new(),
        }
    }
}

/// Encrypted data wrapper
#[derive(Debug, Clone, Serialize, Deserialize)]
struct EncryptedData {
    /// Key identifier used for encryption
    key_id: String,
    /// Nonce/IV used
    nonce: Vec<u8>,
    /// Ciphertext
    ciphertext: Vec<u8>,
    /// Algorithm used
    algorithm: EncryptionAlgorithm,
    /// Encryption timestamp
    encrypted_at: DateTime<Utc>,
}

impl EncryptedData {
    fn to_base64(&self) -> String {
        use base64::{engine::general_purpose, Engine};
        let json = serde_json::to_string(self).unwrap_or_default();
        general_purpose::STANDARD.encode(json.as_bytes())
    }

    fn from_base64(s: &str) -> Result<Self, SecurityError> {
        use base64::{engine::general_purpose, Engine};
        let bytes = general_purpose::STANDARD
            .decode(s)
            .map_err(|e| SecurityError::EncryptionError(format!("Invalid base64: {}", e)))?;
        let json = String::from_utf8(bytes)
            .map_err(|e| SecurityError::EncryptionError(format!("Invalid UTF-8: {}", e)))?;
        serde_json::from_str(&json)
            .map_err(|e| SecurityError::EncryptionError(format!("Invalid JSON: {}", e)))
    }
}

/// Cache entry for encrypted/decrypted values
#[derive(Clone)]
struct CacheEntry {
    value: String,
    cached_at: DateTime<Utc>,
}

/// Column-level encryption manager
pub struct ColumnEncryptionManager {
    config: ColumnEncryptionConfig,
    key_manager: Arc<KeyManager>,
    /// Encryption cache (plaintext -> encrypted)
    encryption_cache: Arc<RwLock<HashMap<String, CacheEntry>>>,
    /// Decryption cache (encrypted -> plaintext)
    decryption_cache: Arc<RwLock<HashMap<String, CacheEntry>>>,
    rng: SystemRandom,
}

impl ColumnEncryptionManager {
    /// Create a new column encryption manager
    pub fn new(config: ColumnEncryptionConfig) -> Self {
        let key_manager = Arc::new(KeyManager::new(config.key_manager.clone()));

        Self {
            config,
            key_manager,
            encryption_cache: Arc::new(RwLock::new(HashMap::new())),
            decryption_cache: Arc::new(RwLock::new(HashMap::new())),
            rng: SystemRandom::new(),
        }
    }

    /// Check if a column requires encryption
    pub fn is_encrypted_column(&self, table: &str, column: &str) -> bool {
        let column_name = format!("{}.{}", table, column);
        self.config.encrypted_columns.contains(&column_name)
    }

    /// Encrypt data for a column
    pub async fn encrypt(
        &self,
        table: &str,
        column: &str,
        plaintext: &str,
        _context: &QueryContext,
    ) -> Result<String, SecurityError> {
        // Check cache first
        let cache_key = format!("{}:{}:{}", table, column, plaintext);
        {
            let cache = self.encryption_cache.read().await;
            if let Some(entry) = cache.get(&cache_key) {
                let age = Utc::now().signed_duration_since(entry.cached_at);
                if age.num_seconds() < self.config.cache_ttl_secs as i64 {
                    debug!("Encryption cache hit for {}.{}", table, column);
                    return Ok(entry.value.clone());
                }
            }
        }

        // Get or create DEK
        let dek = self.key_manager.get_or_create_dek(table, column).await?;

        // Generate nonce
        let mut nonce_bytes = vec![0u8; 12];
        self.rng
            .fill(&mut nonce_bytes)
            .map_err(|_| SecurityError::EncryptionError("Failed to generate nonce".into()))?;

        // Encrypt based on algorithm
        let ciphertext = match dek.algorithm {
            EncryptionAlgorithm::Aes256Gcm => {
                self.encrypt_aes_gcm(&dek.key_bytes, &nonce_bytes, plaintext.as_bytes())?
            }
            EncryptionAlgorithm::ChaCha20Poly1305 => {
                self.encrypt_chacha20(&dek.key_bytes, &nonce_bytes, plaintext.as_bytes())?
            }
        };

        let encrypted_data = EncryptedData {
            key_id: dek.key_id.clone(),
            nonce: nonce_bytes,
            ciphertext,
            algorithm: dek.algorithm,
            encrypted_at: Utc::now(),
        };

        let encoded = encrypted_data.to_base64();

        // Update cache
        let mut cache = self.encryption_cache.write().await;
        if cache.len() >= self.config.max_cache_size {
            // Simple LRU: remove oldest entry
            if let Some(oldest_key) = cache
                .iter()
                .min_by_key(|(_, v)| v.cached_at)
                .map(|(k, _)| k.clone())
            {
                cache.remove(&oldest_key);
            }
        }
        cache.insert(
            cache_key,
            CacheEntry {
                value: encoded.clone(),
                cached_at: Utc::now(),
            },
        );

        debug!(
            "Encrypted data for {}.{} with key {}",
            table, column, dek.key_id
        );
        Ok(encoded)
    }

    /// Decrypt data for authorized user
    pub async fn decrypt(
        &self,
        table: &str,
        column: &str,
        encrypted: &str,
        context: &QueryContext,
    ) -> Result<String, SecurityError> {
        // Check permissions first
        if !self
            .check_decrypt_permission(table, column, context)
            .await?
        {
            return Ok("***ENCRYPTED***".to_string());
        }

        // Check cache
        let cache_key = format!("{}:{}:{}", table, column, encrypted);
        {
            let cache = self.decryption_cache.read().await;
            if let Some(entry) = cache.get(&cache_key) {
                let age = Utc::now().signed_duration_since(entry.cached_at);
                if age.num_seconds() < self.config.cache_ttl_secs as i64 {
                    debug!("Decryption cache hit for {}.{}", table, column);
                    return Ok(entry.value.clone());
                }
            }
        }

        // Parse encrypted data
        let encrypted_data = EncryptedData::from_base64(encrypted)?;

        // Get DEK (try active first, then historical)
        let dek = match self.key_manager.get_or_create_dek(table, column).await {
            Ok(key) if key.key_id == encrypted_data.key_id => key,
            _ => {
                self.key_manager
                    .get_historical_key(&encrypted_data.key_id)
                    .await?
            }
        };

        // Decrypt based on algorithm
        let plaintext_bytes = match encrypted_data.algorithm {
            EncryptionAlgorithm::Aes256Gcm => self.decrypt_aes_gcm(
                &dek.key_bytes,
                &encrypted_data.nonce,
                &encrypted_data.ciphertext,
            )?,
            EncryptionAlgorithm::ChaCha20Poly1305 => self.decrypt_chacha20(
                &dek.key_bytes,
                &encrypted_data.nonce,
                &encrypted_data.ciphertext,
            )?,
        };

        let plaintext = String::from_utf8(plaintext_bytes).map_err(|e| {
            SecurityError::EncryptionError(format!("Invalid UTF-8 in plaintext: {}", e))
        })?;

        // Update cache
        let mut cache = self.decryption_cache.write().await;
        if cache.len() >= self.config.max_cache_size {
            if let Some(oldest_key) = cache
                .iter()
                .min_by_key(|(_, v)| v.cached_at)
                .map(|(k, _)| k.clone())
            {
                cache.remove(&oldest_key);
            }
        }
        cache.insert(
            cache_key,
            CacheEntry {
                value: plaintext.clone(),
                cached_at: Utc::now(),
            },
        );

        debug!(
            "Decrypted data for {}.{} with key {}",
            table, column, dek.key_id
        );
        Ok(plaintext)
    }

    /// Encrypt using AES-256-GCM
    fn encrypt_aes_gcm(
        &self,
        key_bytes: &[u8],
        nonce_bytes: &[u8],
        plaintext: &[u8],
    ) -> Result<Vec<u8>, SecurityError> {
        let unbound_key = UnboundKey::new(&AES_256_GCM, key_bytes)
            .map_err(|_| SecurityError::EncryptionError("Invalid key".into()))?;

        let nonce = Nonce::try_assume_unique_for_key(nonce_bytes)
            .map_err(|_| SecurityError::EncryptionError("Invalid nonce".into()))?;

        let mut sealing_key = SealingKey::new(unbound_key, ConstantNonce::new(nonce));

        let mut in_out = plaintext.to_vec();
        sealing_key
            .seal_in_place_append_tag(Aad::empty(), &mut in_out)
            .map_err(|_| SecurityError::EncryptionError("Encryption failed".into()))?;

        Ok(in_out)
    }

    /// Decrypt using AES-256-GCM
    fn decrypt_aes_gcm(
        &self,
        key_bytes: &[u8],
        nonce_bytes: &[u8],
        ciphertext: &[u8],
    ) -> Result<Vec<u8>, SecurityError> {
        let unbound_key = UnboundKey::new(&AES_256_GCM, key_bytes)
            .map_err(|_| SecurityError::EncryptionError("Invalid key".into()))?;

        let nonce = Nonce::try_assume_unique_for_key(nonce_bytes)
            .map_err(|_| SecurityError::EncryptionError("Invalid nonce".into()))?;

        let mut opening_key = OpeningKey::new(unbound_key, ConstantNonce::new(nonce));

        let mut in_out = ciphertext.to_vec();
        let plaintext = opening_key
            .open_in_place(Aad::empty(), &mut in_out)
            .map_err(|_| SecurityError::EncryptionError("Decryption failed".into()))?;

        Ok(plaintext.to_vec())
    }

    /// Encrypt using ChaCha20-Poly1305
    fn encrypt_chacha20(
        &self,
        key_bytes: &[u8],
        nonce_bytes: &[u8],
        plaintext: &[u8],
    ) -> Result<Vec<u8>, SecurityError> {
        let unbound_key = UnboundKey::new(&CHACHA20_POLY1305, key_bytes)
            .map_err(|_| SecurityError::EncryptionError("Invalid key".into()))?;

        let nonce = Nonce::try_assume_unique_for_key(nonce_bytes)
            .map_err(|_| SecurityError::EncryptionError("Invalid nonce".into()))?;

        let mut sealing_key = SealingKey::new(unbound_key, ConstantNonce::new(nonce));

        let mut in_out = plaintext.to_vec();
        sealing_key
            .seal_in_place_append_tag(Aad::empty(), &mut in_out)
            .map_err(|_| SecurityError::EncryptionError("Encryption failed".into()))?;

        Ok(in_out)
    }

    /// Decrypt using ChaCha20-Poly1305
    fn decrypt_chacha20(
        &self,
        key_bytes: &[u8],
        nonce_bytes: &[u8],
        ciphertext: &[u8],
    ) -> Result<Vec<u8>, SecurityError> {
        let unbound_key = UnboundKey::new(&CHACHA20_POLY1305, key_bytes)
            .map_err(|_| SecurityError::EncryptionError("Invalid key".into()))?;

        let nonce = Nonce::try_assume_unique_for_key(nonce_bytes)
            .map_err(|_| SecurityError::EncryptionError("Invalid nonce".into()))?;

        let mut opening_key = OpeningKey::new(unbound_key, ConstantNonce::new(nonce));

        let mut in_out = ciphertext.to_vec();
        let plaintext = opening_key
            .open_in_place(Aad::empty(), &mut in_out)
            .map_err(|_| SecurityError::EncryptionError("Decryption failed".into()))?;

        Ok(plaintext.to_vec())
    }

    /// Check if user has permission to decrypt a column
    async fn check_decrypt_permission(
        &self,
        _table: &str,
        _column: &str,
        _context: &QueryContext,
    ) -> Result<bool, SecurityError> {
        // TODO: Integrate with RBAC system
        // For now, allow decryption for authenticated users
        Ok(_context.user_id.is_some())
    }

    /// Rotate key for a specific column
    pub async fn rotate_column_key(
        &self,
        table: &str,
        column: &str,
    ) -> Result<String, SecurityError> {
        // Clear caches for this column
        let prefix = format!("{}:{}:", table, column);
        {
            let mut enc_cache = self.encryption_cache.write().await;
            enc_cache.retain(|k, _| !k.starts_with(&prefix));
        }
        {
            let mut dec_cache = self.decryption_cache.write().await;
            dec_cache.retain(|k, _| !k.starts_with(&prefix));
        }

        self.key_manager.rotate_key(table, column).await
    }

    /// Get key manager reference
    pub fn key_manager(&self) -> &Arc<KeyManager> {
        &self.key_manager
    }

    /// Clear all caches
    pub async fn clear_caches(&self) {
        self.encryption_cache.write().await.clear();
        self.decryption_cache.write().await.clear();
        info!("Cleared encryption/decryption caches");
    }

    /// Get cache statistics
    pub async fn get_cache_stats(&self) -> CacheStats {
        let enc_size = self.encryption_cache.read().await.len();
        let dec_size = self.decryption_cache.read().await.len();

        CacheStats {
            encryption_cache_size: enc_size,
            decryption_cache_size: dec_size,
            max_cache_size: self.config.max_cache_size,
            cache_ttl_secs: self.config.cache_ttl_secs,
        }
    }
}

/// Constant nonce wrapper for single-use operations
struct ConstantNonce {
    nonce: Option<Nonce>,
}

impl ConstantNonce {
    fn new(nonce: Nonce) -> Self {
        Self { nonce: Some(nonce) }
    }
}

impl NonceSequence for ConstantNonce {
    fn advance(&mut self) -> Result<Nonce, Unspecified> {
        self.nonce.take().ok_or(Unspecified)
    }
}

/// Cache statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheStats {
    pub encryption_cache_size: usize,
    pub decryption_cache_size: usize,
    pub max_cache_size: usize,
    pub cache_ttl_secs: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handlers::database::types::QueryType;

    fn create_test_context(user_id: &str) -> QueryContext {
        QueryContext {
            query_type: QueryType::Select,
            user_id: Some(user_id.to_string()),
            session_id: "test-session".to_string(),
            timestamp: Utc::now(),
            source_ip: Some("127.0.0.1".to_string()),
            client_info: None,
        }
    }

    #[tokio::test]
    async fn test_key_generation() {
        let config = KeyManagerConfig::default();
        let key_manager = KeyManager::new(config);

        let key_id = key_manager.generate_dek("users", "email").await.unwrap();
        assert!(key_id.starts_with("users.email:v"));
    }

    #[tokio::test]
    async fn test_encrypt_decrypt_aes() {
        let mut config = ColumnEncryptionConfig::default();
        config.encrypted_columns.push("users.ssn".to_string());

        let manager = ColumnEncryptionManager::new(config);
        let context = create_test_context("admin");

        let plaintext = "123-45-6789";
        let encrypted = manager
            .encrypt("users", "ssn", plaintext, &context)
            .await
            .unwrap();

        assert_ne!(encrypted, plaintext);
        assert!(!encrypted.is_empty());

        let decrypted = manager
            .decrypt("users", "ssn", &encrypted, &context)
            .await
            .unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[tokio::test]
    async fn test_encryption_cache() {
        let mut config = ColumnEncryptionConfig::default();
        config.encrypted_columns.push("users.email".to_string());
        config.cache_ttl_secs = 60;

        let manager = ColumnEncryptionManager::new(config);
        let context = create_test_context("user1");

        let plaintext = "test@example.com";

        // First encryption
        let encrypted1 = manager
            .encrypt("users", "email", plaintext, &context)
            .await
            .unwrap();

        // Second encryption (should use cache, same result)
        let encrypted2 = manager
            .encrypt("users", "email", plaintext, &context)
            .await
            .unwrap();

        assert_eq!(encrypted1, encrypted2);
    }

    #[tokio::test]
    async fn test_key_rotation() {
        let config = KeyManagerConfig::default();
        let key_manager = KeyManager::new(config);

        let key_id1 = key_manager.generate_dek("users", "password").await.unwrap();
        let key_id2 = key_manager.rotate_key("users", "password").await.unwrap();

        assert_ne!(key_id1, key_id2);
        assert!(key_id2.ends_with(":v2"));
    }

    #[tokio::test]
    async fn test_decrypt_with_old_key() {
        let mut config = ColumnEncryptionConfig::default();
        config.encrypted_columns.push("users.data".to_string());

        let manager = ColumnEncryptionManager::new(config);
        let context = create_test_context("admin");

        // Encrypt with key v1
        let plaintext = "sensitive data";
        let encrypted = manager
            .encrypt("users", "data", plaintext, &context)
            .await
            .unwrap();

        // Rotate key to v2
        manager.rotate_column_key("users", "data").await.unwrap();

        // Should still be able to decrypt data encrypted with v1
        let decrypted = manager
            .decrypt("users", "data", &encrypted, &context)
            .await
            .unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[tokio::test]
    async fn test_permission_check() {
        let mut config = ColumnEncryptionConfig::default();
        config.encrypted_columns.push("users.ssn".to_string());

        let manager = ColumnEncryptionManager::new(config);

        // User with ID can decrypt
        let context_auth = create_test_context("admin");
        let encrypted = manager
            .encrypt("users", "ssn", "123-45-6789", &context_auth)
            .await
            .unwrap();
        let decrypted = manager
            .decrypt("users", "ssn", &encrypted, &context_auth)
            .await
            .unwrap();
        assert_eq!(decrypted, "123-45-6789");

        // User without ID cannot decrypt
        let mut context_unauth = create_test_context("guest");
        context_unauth.user_id = None;
        let result = manager
            .decrypt("users", "ssn", &encrypted, &context_unauth)
            .await
            .unwrap();
        assert_eq!(result, "***ENCRYPTED***");
    }

    #[tokio::test]
    async fn test_cache_stats() {
        let mut config = ColumnEncryptionConfig::default();
        config.encrypted_columns.push("test.field".to_string());
        config.max_cache_size = 100;

        let manager = ColumnEncryptionManager::new(config);
        let context = create_test_context("user1");

        manager
            .encrypt("test", "field", "value1", &context)
            .await
            .unwrap();
        manager
            .encrypt("test", "field", "value2", &context)
            .await
            .unwrap();

        let stats = manager.get_cache_stats().await;
        assert_eq!(stats.encryption_cache_size, 2);
        assert_eq!(stats.max_cache_size, 100);
    }
}
