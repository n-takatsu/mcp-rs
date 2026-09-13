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
#[cfg(feature = "hsm")]
use tokio::sync::OnceCell;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

use crate::handlers::database::column_encryption_rbac::{
    ColumnEncryptionRbac, EncryptionAuditLog, EncryptionOperation,
};
use crate::handlers::database::types::{QueryContext, SecurityError};
use crate::security::auth::types::AuthUser;
#[cfg(feature = "hsm")]
use crate::security::hsm::{HsmProvider, Pkcs11Config};
#[cfg(feature = "hsm")]
use cryptoki::object::ObjectHandle;

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
#[derive(Debug, Clone, Serialize, Deserialize)]
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
    /// PKCS#11 HSM integration (SoftHSM2, or a real/cloud HSM's PKCS#11
    /// module). Unlike `AwsKms`/`Vault` above, this variant is actually
    /// wired up: keys are generated inside the token with
    /// `CKA_EXTRACTABLE = false` and never exist as raw bytes in this
    /// process's memory. Only [`EncryptionAlgorithm::Aes256Gcm`] is
    /// supported with this provider.
    #[cfg(feature = "hsm")]
    Pkcs11 {
        library_path: String,
        slot_id: Option<u64>,
        /// The token's user PIN. Held as a `SecretString` (redacted by its
        /// own `Debug` impl) and serialized as a fixed placeholder rather
        /// than the real value, so it can't leak via `{:?}` logging or by
        /// accidentally serializing this config (e.g. into telemetry or a
        /// diagnostics dump).
        #[serde(serialize_with = "redact_pin", deserialize_with = "deserialize_pin")]
        pin: secrecy::SecretString,
    },
}

/// Manual `PartialEq` (the field types on `Pkcs11.pin` no longer support a
/// derive - `secrecy::SecretString` deliberately doesn't implement
/// `PartialEq`, to discourage casually comparing secrets). For every other
/// variant this compares exactly like the derive would have. For `Pkcs11`,
/// it compares everything except the PIN: two provider configs pointing at
/// the same library/slot are equal regardless of PIN, matching how the rest
/// of this type already treats the PIN as not participating in identity.
impl PartialEq for KeyProvider {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (
                Self::AwsKms {
                    region: r1,
                    key_id: k1,
                },
                Self::AwsKms {
                    region: r2,
                    key_id: k2,
                },
            ) => r1 == r2 && k1 == k2,
            (
                Self::Vault {
                    address: a1,
                    mount_path: m1,
                    key_name: k1,
                },
                Self::Vault {
                    address: a2,
                    mount_path: m2,
                    key_name: k2,
                },
            ) => a1 == a2 && m1 == m2 && k1 == k2,
            (Self::Local { key_path: p1 }, Self::Local { key_path: p2 }) => p1 == p2,
            #[cfg(feature = "hsm")]
            (
                Self::Pkcs11 {
                    library_path: l1,
                    slot_id: s1,
                    pin: _,
                },
                Self::Pkcs11 {
                    library_path: l2,
                    slot_id: s2,
                    pin: _,
                },
            ) => l1 == l2 && s1 == s2,
            _ => false,
        }
    }
}

impl Eq for KeyProvider {}

#[cfg(feature = "hsm")]
fn redact_pin<S: serde::Serializer>(
    _pin: &secrecy::SecretString,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    serializer.serialize_str("***REDACTED***")
}

#[cfg(feature = "hsm")]
fn deserialize_pin<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<secrecy::SecretString, D::Error> {
    let pin = String::deserialize(deserializer)?;
    Ok(secrecy::SecretString::from(pin))
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

/// Where a [`DataEncryptionKey`]'s actual key material lives.
#[derive(Clone)]
pub(crate) enum KeyBacking {
    /// Raw key bytes held in this process's memory.
    Software(Vec<u8>),
    /// A non-extractable key generated inside a PKCS#11 token; only an
    /// opaque handle is held here, never the raw key bytes.
    #[cfg(feature = "hsm")]
    Hsm(ObjectHandle),
}

/// The algorithm a [`DataEncryptionKey`] with the given `backing` actually
/// encrypts/decrypts with. For `Software` this is whatever the caller
/// configured, but an `Hsm`-backed key always goes through
/// [`HsmProvider::encrypt_gcm`]/`decrypt_gcm` regardless of `configured` -
/// deriving it here instead of trusting `configured` directly means a
/// stale or misconfigured `default_algorithm` (e.g. changed to
/// `ChaCha20Poly1305` after keys were already created under the Pkcs11
/// provider) can never make a `DataEncryptionKey`'s recorded algorithm
/// disagree with the cipher that will actually run.
fn effective_algorithm(
    backing: &KeyBacking,
    configured: EncryptionAlgorithm,
) -> EncryptionAlgorithm {
    match backing {
        KeyBacking::Software(_) => configured,
        #[cfg(feature = "hsm")]
        KeyBacking::Hsm(_) => EncryptionAlgorithm::Aes256Gcm,
    }
}

/// Data Encryption Key (DEK) wrapper
#[derive(Clone)]
pub(crate) struct DataEncryptionKey {
    key_id: String,
    version: u32,
    backing: KeyBacking,
    algorithm: EncryptionAlgorithm,
    created_at: DateTime<Utc>,
}

impl DataEncryptionKey {
    fn new(
        key_id: String,
        version: u32,
        backing: KeyBacking,
        algorithm: EncryptionAlgorithm,
    ) -> Self {
        Self {
            key_id,
            version,
            backing,
            algorithm,
            created_at: Utc::now(),
        }
    }
}

impl Drop for DataEncryptionKey {
    fn drop(&mut self) {
        // Zeroize key material on drop for security. An HSM-backed key has
        // no local secret material to zero - the handle is just an opaque
        // id, never the key itself. (Without the "hsm" feature, `KeyBacking`
        // only ever has the `Software` variant, making this pattern
        // irrefutable - that's fine, it still needs to run.)
        #[cfg_attr(not(feature = "hsm"), allow(irrefutable_let_patterns))]
        if let KeyBacking::Software(key_bytes) = &mut self.backing {
            use zeroize::Zeroize;
            key_bytes.zeroize();
        }
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

/// Parse the trailing `:v{N}` version number off a `key_id` such as
/// `"users.ssn:v2"`. Used only to reconstruct a [`DataEncryptionKey`]'s
/// `version` field when recovering an HSM-backed key by its label alone -
/// the version number embedded there is cosmetic (used for logging/ordering)
/// and never re-derived to decide the *next* version to mint.
#[cfg(feature = "hsm")]
fn parse_version_suffix(key_id: &str) -> Option<u32> {
    key_id.rsplit_once(":v")?.1.parse().ok()
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
    /// Lazily-connected HSM session, used only when `config.provider` is
    /// [`KeyProvider::Pkcs11`]. Connecting requires I/O (loading the PKCS#11
    /// library, opening a session, logging in), so it happens on first use
    /// rather than in [`Self::new`], which stays infallible.
    #[cfg(feature = "hsm")]
    hsm_provider: OnceCell<Arc<HsmProvider>>,
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
            #[cfg(feature = "hsm")]
            hsm_provider: OnceCell::new(),
        }
    }

    /// Connect to (or reuse the existing connection to) the configured
    /// PKCS#11 token. Only called when `config.provider` is `Pkcs11`.
    #[cfg(feature = "hsm")]
    async fn hsm_provider(
        &self,
        library_path: &str,
        slot_id: Option<u64>,
        pin: &secrecy::SecretString,
    ) -> Result<&Arc<HsmProvider>, SecurityError> {
        self.hsm_provider
            .get_or_try_init(|| async {
                let config = Pkcs11Config {
                    library_path: library_path.to_string(),
                    slot_id,
                    pin: pin.clone(),
                };
                HsmProvider::connect(&config).map(Arc::new).map_err(|e| {
                    SecurityError::EncryptionError(format!("HSM connection failed: {e}"))
                })
            })
            .await
    }

    /// Encrypt via the already-connected HSM session. Only ever called for
    /// a DEK whose backing is [`KeyBacking::Hsm`], which can only exist
    /// after [`Self::generate_dek`] already established the connection -
    /// `get()` returning `None` here would mean that invariant was broken.
    #[cfg(feature = "hsm")]
    pub(crate) async fn hsm_encrypt(
        &self,
        handle: ObjectHandle,
        nonce: &mut [u8],
        plaintext: &[u8],
    ) -> Result<Vec<u8>, SecurityError> {
        let provider = self
            .hsm_provider
            .get()
            .ok_or_else(|| SecurityError::EncryptionError("HSM provider not connected".into()))?;
        provider
            .encrypt_gcm(handle, nonce, plaintext)
            .await
            .map_err(|e| SecurityError::EncryptionError(format!("HSM encryption failed: {e}")))
    }

    /// Decrypt via the already-connected HSM session. See
    /// [`Self::hsm_encrypt`] for why `get()` is expected to always succeed.
    #[cfg(feature = "hsm")]
    pub(crate) async fn hsm_decrypt(
        &self,
        handle: ObjectHandle,
        nonce: &mut [u8],
        ciphertext: &[u8],
    ) -> Result<Vec<u8>, SecurityError> {
        let provider = self
            .hsm_provider
            .get()
            .ok_or_else(|| SecurityError::EncryptionError("HSM provider not connected".into()))?;
        provider
            .decrypt_gcm(handle, nonce, ciphertext)
            .await
            .map_err(|e| SecurityError::EncryptionError(format!("HSM decryption failed: {e}")))
    }

    /// Generate a new Data Encryption Key (DEK)
    pub async fn generate_dek(&self, table: &str, column: &str) -> Result<String, SecurityError> {
        let key_name = format!("{}.{}", table, column);

        let (backing, version, key_id) = match &self.config.provider {
            #[cfg(feature = "hsm")]
            KeyProvider::Pkcs11 {
                library_path,
                slot_id,
                pin,
            } => {
                if self.config.default_algorithm != EncryptionAlgorithm::Aes256Gcm {
                    return Err(SecurityError::EncryptionError(
                        "the PKCS#11 HSM provider only supports the Aes256Gcm algorithm".into(),
                    ));
                }
                let provider = self.hsm_provider(library_path, *slot_id, pin).await?;

                // The in-memory version counter below does not survive a
                // process restart, but the token's key objects do - consult
                // both and take the higher one so a restart (with an empty
                // `key_metadata`) can never make this pick a version that
                // collides with, or shadows, one that was already rotated
                // past.
                let in_memory_version = {
                    let metadata = self.key_metadata.read().await;
                    metadata.get(&key_name).map(|m| m.version)
                };
                let hsm_highest_version = provider
                    .find_highest_version(&key_name)
                    .await
                    .map_err(|e| {
                        SecurityError::EncryptionError(format!("HSM version lookup failed: {e}"))
                    })?
                    .map(|(v, _)| v);
                let version = in_memory_version
                    .into_iter()
                    .chain(hsm_highest_version)
                    .max()
                    .map(|v| v + 1)
                    .unwrap_or(1);
                let key_id = format!("{}:v{}", key_name, version);

                let handle = provider
                    .get_or_generate_aes_key(&key_id)
                    .await
                    .map_err(|e| {
                        SecurityError::EncryptionError(format!("HSM key generation failed: {e}"))
                    })?;
                (KeyBacking::Hsm(handle), version, key_id)
            }
            _ => {
                let version = {
                    let metadata = self.key_metadata.read().await;
                    metadata.get(&key_name).map(|m| m.version + 1).unwrap_or(1)
                };
                let key_id = format!("{}:v{}", key_name, version);

                // Generate random key bytes
                let mut key_bytes = vec![0u8; 32]; // 256 bits
                self.rng.fill(&mut key_bytes).map_err(|_| {
                    SecurityError::EncryptionError("Failed to generate random key".into())
                })?;
                (KeyBacking::Software(key_bytes), version, key_id)
            }
        };

        let algorithm = effective_algorithm(&backing, self.config.default_algorithm);
        let dek = DataEncryptionKey::new(key_id.clone(), version, backing, algorithm);

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
            algorithm,
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
        let mut cold_cache = true;

        // Check if active key exists
        {
            let active_keys = self.active_keys.read().await;
            if let Some(dek) = active_keys.get(&key_name) {
                cold_cache = false;
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

        // A cold cache (nothing in `active_keys` for this column at all)
        // means either this is the very first access ever, or the process
        // just restarted and lost its in-memory bookkeeping. For the HSM
        // provider those two cases are distinguishable: the token's key
        // objects survive a restart, so check there first and adopt the
        // existing key rather than unconditionally minting a new version -
        // otherwise every restart would silently rotate the key even
        // though nobody asked for that.
        #[cfg(feature = "hsm")]
        if cold_cache {
            if let Some(dek) = self.adopt_existing_hsm_dek(table, column).await? {
                return Ok(dek);
            }
        }
        #[cfg(not(feature = "hsm"))]
        let _ = cold_cache;

        // Generate new key if not exists or expired
        self.generate_dek(table, column).await?;

        let active_keys = self.active_keys.read().await;
        active_keys.get(&key_name).cloned().ok_or_else(|| {
            SecurityError::EncryptionError("Failed to retrieve generated key".into())
        })
    }

    /// If the configured provider is [`KeyProvider::Pkcs11`] and a key for
    /// `table.column` already exists in the token (at whatever version is
    /// highest), adopt it as the active key without minting a new version,
    /// and return it. Returns `Ok(None)` for every other provider, and for
    /// the Pkcs11 provider when no key has ever been created for this
    /// column yet (the caller should then fall through to
    /// [`Self::generate_dek`] for a true first-time creation).
    #[cfg(feature = "hsm")]
    async fn adopt_existing_hsm_dek(
        &self,
        table: &str,
        column: &str,
    ) -> Result<Option<DataEncryptionKey>, SecurityError> {
        let KeyProvider::Pkcs11 {
            library_path,
            slot_id,
            pin,
        } = &self.config.provider
        else {
            return Ok(None);
        };
        if self.config.default_algorithm != EncryptionAlgorithm::Aes256Gcm {
            return Err(SecurityError::EncryptionError(
                "the PKCS#11 HSM provider only supports the Aes256Gcm algorithm".into(),
            ));
        }
        let key_name = format!("{}.{}", table, column);
        let provider = self.hsm_provider(library_path, *slot_id, pin).await?;
        let Some((version, handle)) =
            provider
                .find_highest_version(&key_name)
                .await
                .map_err(|e| {
                    SecurityError::EncryptionError(format!("HSM version lookup failed: {e}"))
                })?
        else {
            return Ok(None);
        };

        let key_id = format!("{}:v{}", key_name, version);
        let backing = KeyBacking::Hsm(handle);
        let algorithm = effective_algorithm(&backing, self.config.default_algorithm);
        let dek = DataEncryptionKey::new(key_id.clone(), version, backing, algorithm);

        self.active_keys
            .write()
            .await
            .insert(key_name.clone(), dek.clone());
        self.key_metadata.write().await.insert(
            key_name.clone(),
            KeyMetadata {
                key_id,
                version,
                // The key's true creation time isn't recoverable from the
                // token alone; this only affects when the adopted key's
                // *next* expiry-driven rotation is due, not its identity.
                created_at: Utc::now(),
                expires_at: Some(
                    Utc::now()
                        + chrono::Duration::seconds(self.config.rotation_interval_secs as i64),
                ),
                algorithm,
                is_active: true,
            },
        );

        info!(
            "Adopted existing HSM-backed DEK for {}: v{}",
            key_name, version
        );
        Ok(Some(dek))
    }

    /// Get historical key for decryption
    pub(crate) async fn get_historical_key(
        &self,
        key_id: &str,
    ) -> Result<DataEncryptionKey, SecurityError> {
        if let Some(dek) = self.historical_keys.read().await.get(key_id).cloned() {
            return Ok(dek);
        }

        // Not in the in-memory map - for the HSM provider this doesn't
        // necessarily mean the key is gone: `historical_keys` is wiped on
        // every process restart, but a rotated-out key's token object is
        // still sitting in the HSM under this exact `key_id` as its label.
        // Look it up directly instead of failing.
        #[cfg(feature = "hsm")]
        if let KeyProvider::Pkcs11 {
            library_path,
            slot_id,
            pin,
        } = &self.config.provider
        {
            let provider = self.hsm_provider(library_path, *slot_id, pin).await?;
            if let Some(handle) = provider.find_key(key_id).await.map_err(|e| {
                SecurityError::EncryptionError(format!("HSM key lookup failed: {e}"))
            })? {
                let version = parse_version_suffix(key_id).unwrap_or(0);
                let backing = KeyBacking::Hsm(handle);
                // Decryption of an existing HSM-backed key always uses
                // Aes256Gcm regardless of the currently configured
                // `default_algorithm` (see `effective_algorithm`) - a
                // config change since this key was created must not stop
                // already-encrypted data from decrypting correctly.
                let algorithm = effective_algorithm(&backing, self.config.default_algorithm);
                let dek = DataEncryptionKey::new(key_id.to_string(), version, backing, algorithm);
                self.historical_keys
                    .write()
                    .await
                    .insert(key_id.to_string(), dek.clone());
                return Ok(dek);
            }
        }

        Err(SecurityError::EncryptionError(format!(
            "Historical key not found: {}",
            key_id
        )))
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
    /// RBAC integration for column encryption (optional)
    rbac: Option<Arc<ColumnEncryptionRbac>>,
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
            rbac: None,
        }
    }

    /// Create a new column encryption manager with RBAC integration
    pub fn with_rbac(config: ColumnEncryptionConfig, rbac: Arc<ColumnEncryptionRbac>) -> Self {
        let mut manager = Self::new(config);
        manager.rbac = Some(rbac);
        manager
    }

    /// Check if a column requires encryption
    pub fn is_encrypted_column(&self, table: &str, column: &str) -> bool {
        let column_name = format!("{}.{}", table, column);
        self.config.encrypted_columns.contains(&column_name)
    }

    /// Whether any column is configured for encryption at all - callers can
    /// use this to skip per-row enforcement entirely when the feature isn't
    /// in use.
    pub fn has_encrypted_columns(&self) -> bool {
        !self.config.encrypted_columns.is_empty()
    }

    /// Encrypt data for a column
    pub async fn encrypt(
        &self,
        table: &str,
        column: &str,
        plaintext: &str,
        context: &QueryContext,
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

        // Encrypt based on where the key lives and, for software keys, the
        // configured algorithm. HSM-backed keys are always Aes256Gcm - see
        // the check in KeyManager::generate_dek.
        let ciphertext = match &dek.backing {
            #[cfg(feature = "hsm")]
            KeyBacking::Hsm(handle) => {
                self.key_manager
                    .hsm_encrypt(*handle, &mut nonce_bytes, plaintext.as_bytes())
                    .await?
            }
            KeyBacking::Software(key_bytes) => match dek.algorithm {
                EncryptionAlgorithm::Aes256Gcm => {
                    self.encrypt_aes_gcm(key_bytes, &nonce_bytes, plaintext.as_bytes())?
                }
                EncryptionAlgorithm::ChaCha20Poly1305 => {
                    self.encrypt_chacha20(key_bytes, &nonce_bytes, plaintext.as_bytes())?
                }
            },
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

        // Log successful encryption
        self.log_audit(
            EncryptionOperation::Encrypt,
            (table, column),
            context,
            context.user_id.as_deref(),
            true,
            None,
        )
        .await;

        Ok(encoded)
    }

    /// Decrypt data for authorized user
    pub async fn decrypt(
        &self,
        table: &str,
        column: &str,
        encrypted: &str,
        context: &QueryContext,
        auth_user: Option<&AuthUser>,
    ) -> Result<String, SecurityError> {
        // Check permissions first
        let has_permission = self
            .check_decrypt_permission(table, column, auth_user)
            .await?;

        if !has_permission {
            // Log permission denied
            self.log_audit(
                EncryptionOperation::Decrypt,
                (table, column),
                context,
                auth_user.map(|u| u.id.as_str()),
                false,
                Some("Permission denied".to_string()),
            )
            .await;
            return Ok("***ENCRYPTED***".to_string());
        }

        self.decrypt_authorized(table, column, encrypted, context, auth_user)
            .await
    }

    /// Decrypts every value in `ciphertexts` for the same `(table, column)`
    /// in one call. The permission check (and, on denial, the audit log
    /// entry) happens exactly once for the whole batch, not once per value -
    /// unlike calling [`Self::decrypt`] in a loop, which would write one
    /// denied-permission audit row (and warning) per row of a query result,
    /// even though the permission outcome for a given column can't differ
    /// row to row.
    pub async fn decrypt_batch(
        &self,
        table: &str,
        column: &str,
        ciphertexts: &[&str],
        context: &QueryContext,
        auth_user: Option<&AuthUser>,
    ) -> Result<Vec<String>, SecurityError> {
        let has_permission = self
            .check_decrypt_permission(table, column, auth_user)
            .await?;

        if !has_permission {
            self.log_audit(
                EncryptionOperation::Decrypt,
                (table, column),
                context,
                auth_user.map(|u| u.id.as_str()),
                false,
                Some(format!(
                    "Permission denied ({} value(s))",
                    ciphertexts.len()
                )),
            )
            .await;
            return Ok(ciphertexts
                .iter()
                .map(|_| "***ENCRYPTED***".to_string())
                .collect());
        }

        // Batch the success audit too: N successful decrypts for the same
        // (table, column) in one query is one fact ("this user decrypted N
        // values from this column just now"), not N independent events -
        // auditing it N times would scale the same denied-audit problem
        // this method exists to avoid straight onto the success path.
        //
        // A single malformed ciphertext (corrupt data, a bad key, anything
        // that isn't the base64 `EncryptedData` this column is supposed to
        // hold) must not fail the whole batch: the caller is decrypting one
        // column across every row of a query result, and one bad row's
        // content shouldn't hide every other row's legitimate values. Mask
        // just that value and keep going.
        let mut results = Vec::with_capacity(ciphertexts.len());
        let mut failures = 0usize;
        for ciphertext in ciphertexts {
            match self.decrypt_value(table, column, ciphertext).await {
                Ok(plaintext) => results.push(plaintext),
                Err(e) => {
                    warn!(
                        "Failed to decrypt value for {table}.{column}, masking this value instead of failing the query: {e}"
                    );
                    failures += 1;
                    results.push("***DECRYPTION_FAILED***".to_string());
                }
            }
        }

        let message = if failures > 0 {
            format!(
                "{} value(s) decrypted, {failures} failed to decrypt",
                ciphertexts.len() - failures
            )
        } else {
            format!("{} value(s) decrypted", ciphertexts.len())
        };
        self.log_audit(
            EncryptionOperation::Decrypt,
            (table, column),
            context,
            auth_user.map(|u| u.id.as_str()),
            failures == 0,
            Some(message),
        )
        .await;

        Ok(results)
    }

    /// [`Self::decrypt_value`] plus a single-value audit log entry, run once
    /// permission has already been confirmed by the caller.
    async fn decrypt_authorized(
        &self,
        table: &str,
        column: &str,
        encrypted: &str,
        context: &QueryContext,
        auth_user: Option<&AuthUser>,
    ) -> Result<String, SecurityError> {
        let plaintext = self.decrypt_value(table, column, encrypted).await?;

        self.log_audit(
            EncryptionOperation::Decrypt,
            (table, column),
            context,
            auth_user.map(|u| u.id.as_str()),
            true,
            None,
        )
        .await;

        Ok(plaintext)
    }

    /// The actual cache/key-management/decryption work, with no audit
    /// logging of its own - callers decide how to audit (once per value, or
    /// once for a whole batch).
    async fn decrypt_value(
        &self,
        table: &str,
        column: &str,
        encrypted: &str,
    ) -> Result<String, SecurityError> {
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

        // Decrypt based on where the key lives and, for software keys, the
        // algorithm it was encrypted with. Only the HSM path needs `&mut`
        // access to the nonce.
        #[cfg_attr(not(feature = "hsm"), allow(unused_mut))]
        let mut nonce = encrypted_data.nonce.clone();
        let plaintext_bytes = match &dek.backing {
            #[cfg(feature = "hsm")]
            KeyBacking::Hsm(handle) => {
                self.key_manager
                    .hsm_decrypt(*handle, &mut nonce, &encrypted_data.ciphertext)
                    .await?
            }
            KeyBacking::Software(key_bytes) => match encrypted_data.algorithm {
                EncryptionAlgorithm::Aes256Gcm => {
                    self.decrypt_aes_gcm(key_bytes, &nonce, &encrypted_data.ciphertext)?
                }
                EncryptionAlgorithm::ChaCha20Poly1305 => {
                    self.decrypt_chacha20(key_bytes, &nonce, &encrypted_data.ciphertext)?
                }
            },
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
    ///
    /// `auth_user` must be the caller's real, verified identity (with its
    /// actual roles) - previously this fabricated an empty-roles `AuthUser`
    /// from just a user ID string, which meant the RBAC check below could
    /// never actually grant anything through a real role, silently
    /// defeating per-role permissions.
    async fn check_decrypt_permission(
        &self,
        table: &str,
        column: &str,
        auth_user: Option<&AuthUser>,
    ) -> Result<bool, SecurityError> {
        // If RBAC is configured, it is the source of truth for this user's
        // real roles.
        if let Some(rbac) = &self.rbac {
            let Some(user) = auth_user else {
                return Ok(false);
            };

            return match rbac
                .check_permission(user, table, column, EncryptionOperation::Decrypt)
                .await
            {
                // Denial is audited by the caller (decrypt() / decrypt_batch()
                // -> log_audit()) - logging it here too would write a
                // duplicate audit row and emit a duplicate warning.
                Ok(has_permission) => Ok(has_permission),
                Err(e) => {
                    error!("RBAC permission check failed: {}", e);
                    Err(SecurityError::AccessDenied(format!(
                        "Failed to check permissions: {}",
                        e
                    )))
                }
            };
        }

        // Fallback: allow decryption for any verified identity if no RBAC
        // store is configured (there is no per-role policy to enforce).
        Ok(auth_user.is_some())
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

    /// Log encryption operation to audit log
    ///
    /// Always emits a structured tracing event, since without an RBAC store
    /// attached there is nowhere else to record denied or failed operations -
    /// previously this whole function was a silent no-op unless RBAC was
    /// configured, so unauthorized decrypt attempts left no trace at all.
    async fn log_audit(
        &self,
        operation: EncryptionOperation,
        target: (&str, &str),
        context: &QueryContext,
        user_id: Option<&str>,
        success: bool,
        error: Option<String>,
    ) {
        let (table, column) = target;
        let user_id_display = user_id.unwrap_or("<unauthenticated>");
        if success {
            debug!(
                user_id = user_id_display,
                operation = %operation,
                table,
                column,
                "encryption audit"
            );
        } else {
            warn!(
                user_id = user_id_display,
                operation = %operation,
                table,
                column,
                error = error.as_deref().unwrap_or("unknown error"),
                "encryption audit"
            );
        }

        if let Some(rbac) = &self.rbac {
            if let Some(user_id) = user_id {
                let audit_log = EncryptionAuditLog {
                    user_id: user_id.to_string(),
                    operation,
                    table_name: table.to_string(),
                    column_name: column.to_string(),
                    success,
                    error_message: error,
                    request_ip: context.source_ip.clone(),
                    user_agent: context.client_info.clone(),
                };

                if let Err(e) = rbac.audit_log(&audit_log).await {
                    error!("Failed to write audit log: {}", e);
                }
            }
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

        let user = AuthUser::new("admin".to_string(), "admin".to_string());
        let decrypted = manager
            .decrypt("users", "ssn", &encrypted, &context, Some(&user))
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

    /// The Pkcs11 provider only supports Aes256Gcm (PKCS#11 has no standard
    /// ChaCha20-Poly1305 mechanism). This must be rejected before ever
    /// trying to connect to a token, so this test needs no real PKCS#11
    /// library to be meaningful.
    #[cfg(feature = "hsm")]
    #[tokio::test]
    async fn generate_dek_rejects_chacha20_with_pkcs11_provider() {
        let config = KeyManagerConfig {
            provider: KeyProvider::Pkcs11 {
                library_path: "/nonexistent/libsofthsm2.so".to_string(),
                slot_id: None,
                pin: secrecy::SecretString::from("1234".to_string()),
            },
            default_algorithm: EncryptionAlgorithm::ChaCha20Poly1305,
            ..KeyManagerConfig::default()
        };
        let key_manager = KeyManager::new(config);

        let err = key_manager
            .generate_dek("users", "email")
            .await
            .unwrap_err();
        assert!(
            format!("{err}").contains("Aes256Gcm"),
            "expected an Aes256Gcm-only error, got: {err}"
        );
    }

    /// The same Aes256Gcm-only restriction must also be enforced on the
    /// cold-cache "adopt an existing HSM key" path
    /// (`get_or_create_dek` -> `adopt_existing_hsm_dek`), not just on
    /// `generate_dek` - otherwise a misconfigured `default_algorithm`
    /// could let an adopted key's recorded algorithm silently disagree
    /// with the AES-GCM cipher that actually runs. The check happens
    /// before ever connecting to a token, so this needs no real PKCS#11
    /// library either.
    #[cfg(feature = "hsm")]
    #[tokio::test]
    async fn get_or_create_dek_rejects_chacha20_with_pkcs11_provider_on_cold_cache() {
        let config = KeyManagerConfig {
            provider: KeyProvider::Pkcs11 {
                library_path: "/nonexistent/libsofthsm2.so".to_string(),
                slot_id: None,
                pin: secrecy::SecretString::from("1234".to_string()),
            },
            default_algorithm: EncryptionAlgorithm::ChaCha20Poly1305,
            ..KeyManagerConfig::default()
        };
        let key_manager = KeyManager::new(config);

        let err = match key_manager.get_or_create_dek("users", "email").await {
            Ok(_) => panic!("expected an error, got a DEK"),
            Err(e) => e,
        };
        assert!(
            format!("{err}").contains("Aes256Gcm"),
            "expected an Aes256Gcm-only error, got: {err}"
        );
    }

    /// A Pkcs11 provider pointed at a library that doesn't exist must
    /// surface a clear connection error instead of panicking, and must not
    /// require a real HSM/SoftHSM2 to exercise this failure path.
    #[cfg(feature = "hsm")]
    #[tokio::test]
    async fn generate_dek_surfaces_connection_error_for_missing_pkcs11_library() {
        let config = KeyManagerConfig {
            provider: KeyProvider::Pkcs11 {
                library_path: "/nonexistent/libsofthsm2.so".to_string(),
                slot_id: None,
                pin: secrecy::SecretString::from("1234".to_string()),
            },
            default_algorithm: EncryptionAlgorithm::Aes256Gcm,
            ..KeyManagerConfig::default()
        };
        let key_manager = KeyManager::new(config);

        let err = key_manager
            .generate_dek("users", "email")
            .await
            .unwrap_err();
        assert!(
            format!("{err}").contains("HSM connection failed"),
            "expected an HSM connection error, got: {err}"
        );
    }

    /// The PIN must never show up verbatim in `{:?}` output or in
    /// serialized config - both are places it could end up in logs,
    /// telemetry, or a diagnostics dump.
    #[cfg(feature = "hsm")]
    #[test]
    fn pkcs11_provider_pin_is_redacted_in_debug_and_serialize() {
        let provider = KeyProvider::Pkcs11 {
            library_path: "/usr/lib/softhsm/libsofthsm2.so".to_string(),
            slot_id: None,
            pin: secrecy::SecretString::from("super-secret-pin".to_string()),
        };

        let debug_output = format!("{provider:?}");
        assert!(
            !debug_output.contains("super-secret-pin"),
            "Debug output must not contain the raw PIN: {debug_output}"
        );

        let json = serde_json::to_string(&provider).expect("KeyProvider should serialize");
        assert!(
            !json.contains("super-secret-pin"),
            "serialized config must not contain the raw PIN: {json}"
        );

        // Deserializing still recovers the real PIN - only serialization
        // (the "write this out somewhere") is redacted.
        let KeyProvider::Pkcs11 {
            pin: roundtripped_pin,
            ..
        } = serde_json::from_str::<KeyProvider>(
            r#"{"Pkcs11":{"library_path":"lib.so","slot_id":null,"pin":"super-secret-pin"}}"#,
        )
        .expect("KeyProvider should deserialize")
        else {
            panic!("expected a Pkcs11 provider");
        };
        assert_eq!(
            secrecy::ExposeSecret::expose_secret(&roundtripped_pin),
            "super-secret-pin"
        );
    }

    /// `KeyProvider` keeps its (previously derived) `PartialEq`/`Eq` -
    /// restored as a manual impl since `SecretString` doesn't support
    /// deriving it. Two `Pkcs11` configs pointing at the same library/slot
    /// are equal regardless of PIN.
    #[test]
    fn key_provider_equality_matches_non_secret_fields() {
        assert_eq!(
            KeyProvider::Local {
                key_path: "keys".to_string(),
            },
            KeyProvider::Local {
                key_path: "keys".to_string(),
            }
        );
        assert_ne!(
            KeyProvider::Local {
                key_path: "keys".to_string(),
            },
            KeyProvider::Local {
                key_path: "other".to_string(),
            }
        );

        #[cfg(feature = "hsm")]
        {
            let a = KeyProvider::Pkcs11 {
                library_path: "/usr/lib/softhsm/libsofthsm2.so".to_string(),
                slot_id: Some(0),
                pin: secrecy::SecretString::from("pin-a".to_string()),
            };
            let b = KeyProvider::Pkcs11 {
                library_path: "/usr/lib/softhsm/libsofthsm2.so".to_string(),
                slot_id: Some(0),
                pin: secrecy::SecretString::from("pin-b".to_string()),
            };
            assert_eq!(a, b, "PIN must not affect equality");

            let different_slot = KeyProvider::Pkcs11 {
                library_path: "/usr/lib/softhsm/libsofthsm2.so".to_string(),
                slot_id: Some(1),
                pin: secrecy::SecretString::from("pin-a".to_string()),
            };
            assert_ne!(a, different_slot);

            assert_ne!(
                a,
                KeyProvider::Local {
                    key_path: "keys".to_string()
                },
                "different variants must never be equal"
            );
        }
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
        let user = AuthUser::new("admin".to_string(), "admin".to_string());
        let decrypted = manager
            .decrypt("users", "data", &encrypted, &context, Some(&user))
            .await
            .unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[tokio::test]
    async fn test_permission_check() {
        let mut config = ColumnEncryptionConfig::default();
        config.encrypted_columns.push("users.ssn".to_string());

        let manager = ColumnEncryptionManager::new(config);

        // A verified identity can decrypt when no RBAC store is configured
        let context_auth = create_test_context("admin");
        let user = AuthUser::new("admin".to_string(), "admin".to_string());
        let encrypted = manager
            .encrypt("users", "ssn", "123-45-6789", &context_auth)
            .await
            .unwrap();
        let decrypted = manager
            .decrypt("users", "ssn", &encrypted, &context_auth, Some(&user))
            .await
            .unwrap();
        assert_eq!(decrypted, "123-45-6789");

        // No identity provided cannot decrypt
        let context_unauth = create_test_context("guest");
        let result = manager
            .decrypt("users", "ssn", &encrypted, &context_unauth, None)
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
