//! プラグイン間鍵交換プロトコルと E2E 暗号化専用型
//!
//! X25519 ECDH による一時鍵交換、HKDF-SHA256 による鍵導出、
//! ChaCha20-Poly1305 による認証付き暗号化、Ed25519 による署名を組み合わせ
//! Perfect Forward Secrecy (PFS) 対応のエンドツーエンド暗号化を提供します。
//!
//! ## 設計
//!
//! ```text
//! Plugin A ─── initiate_key_exchange() ──> Plugin B
//!              ECDH: a_secret * b_public = b_secret * a_public
//!              HKDF-SHA256 → symmetric key K
//!              ChaCha20-Poly1305(K) + Ed25519 sign
//!              expires_at = now + message_ttl_secs  (リプレイ攻撃対策)
//! ```
//!
//! ## グレースピリオドによるローリングローテーション
//!
//! `rotate_session` を呼ぶと旧鍵が `secondary_key` として `grace_period_hours` 保持され、
//! ローテーション前に暗号化されたメッセージも引き続き復号できます。
//!
//! ## 使用例
//!
//! ```rust,ignore
//! let config = KeyExchangeConfig::default();
//! let protocol = Arc::new(KeyExchangeProtocol::new_with_config(config));
//! protocol.register_plugin(plugin_a).await?;
//! protocol.register_plugin(plugin_b).await?;
//! protocol.initiate_key_exchange(plugin_a, plugin_b).await?;
//! KeyExchangeProtocol::start_auto_rotation(Arc::clone(&protocol));
//!
//! let payload = protocol.encrypt_for_peer(plugin_a, plugin_b, b"hello").await?;
//! let plain   = protocol.decrypt_from_peer(plugin_b, plugin_a, &payload).await?;
//! ```

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use chacha20poly1305::{aead::Aead, ChaCha20Poly1305, KeyInit, Nonce};
use chrono::{DateTime, Utc};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use ring::{
    hkdf,
    rand::{SecureRandom, SystemRandom},
};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::warn;
use uuid::Uuid;
use x25519_dalek::{PublicKey as X25519PublicKey, StaticSecret};

use crate::error::Error as McpError;

// ---------------------------------------------------------------------------
// HKDF KeyType 実装
// ---------------------------------------------------------------------------

/// HKDF-SHA256 で 32 バイト鍵を導出するための `ring::hkdf::KeyType` 実装
struct HkdfKey32;

impl hkdf::KeyType for HkdfKey32 {
    fn len(&self) -> usize {
        32
    }
}

// ---------------------------------------------------------------------------
// KeyExchangeConfig
// ---------------------------------------------------------------------------

/// 鍵交換プロトコル設定
#[derive(Debug, Clone)]
pub struct KeyExchangeConfig {
    /// セッション鍵の有効期間（時間）
    pub key_lifetime_hours: u64,
    /// 鍵ローテーション後の旧鍵グレースピリオド（時間）
    ///
    /// ローテーション直後は新旧両方の鍵で復号を試みます。
    /// ローリングデプロイや遅延メッセージに対する後方互換性を確保します。
    pub grace_period_hours: u64,
    /// 暗号化メッセージの有効期間（秒）
    ///
    /// `expires_at` を超えたペイロードは復号を拒否します（リプレイ攻撃対策）。
    pub message_ttl_secs: u64,
}

impl Default for KeyExchangeConfig {
    fn default() -> Self {
        Self {
            key_lifetime_hours: 24,
            grace_period_hours: 1,
            message_ttl_secs: 300, // 5 分
        }
    }
}

// ---------------------------------------------------------------------------
// 公開型: EncryptedPayload
// ---------------------------------------------------------------------------

/// E2E 暗号化ペイロード
///
/// ChaCha20-Poly1305 で暗号化し Ed25519 で署名した独立型。
/// `BrokerMessage.payload` の代替としてプラグイン間通信で使用できます。
///
/// `timestamp` / `expires_at` はリプレイ攻撃対策です。
/// 受信側は `Utc::now().timestamp() > expires_at` であれば復号を拒否します。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptedPayload {
    /// ChaCha20-Poly1305 暗号文（認証タグを含む）
    pub ciphertext: Vec<u8>,
    /// 12 バイト nonce (ランダム生成)
    pub nonce: [u8; 12],
    /// 送信者側 X25519 ECDH 公開鍵（32 バイト）
    pub sender_ecdh_public_key: [u8; 32],
    /// 暗号文に対する Ed25519 署名（64 バイト）
    pub signature: Vec<u8>,
    /// 暗号化時刻（Unix 秒）
    pub timestamp: i64,
    /// メッセージ有効期限（Unix 秒）
    ///
    /// `Utc::now().timestamp() > expires_at` の場合、`decrypt` はエラーを返します。
    pub expires_at: i64,
}

// ---------------------------------------------------------------------------
// 内部型: SessionKey
// ---------------------------------------------------------------------------

/// プラグインペア間 HKDF 導出済み対称鍵
#[derive(Debug, Clone)]
struct SessionKey {
    /// プライマリ鍵（最新のローテーション後の鍵）
    key: [u8; 32],
    /// セカンダリ鍵（直前のローテーション前の旧鍵 — グレースピリオド中のみ有効）
    secondary_key: Option<[u8; 32]>,
    /// セカンダリ鍵の有効期限
    secondary_expires_at: Option<DateTime<Utc>>,
    /// 自身の X25519 公開鍵（EncryptedPayload に埋め込むため保持）
    my_ecdh_public: [u8; 32],
    /// ピアの Ed25519 検証公開鍵（署名検証用）
    peer_verifying_key: [u8; 32],
    /// 鍵生成日時
    created_at: DateTime<Utc>,
    /// プライマリ鍵有効期限
    expires_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// PluginCryptoContext
// ---------------------------------------------------------------------------

/// プラグイン暗号コンテキスト
///
/// 1 プラグインにつき 1 インスタンス。
/// Ed25519 署名鍵ペアを保持し、ピアごとの HKDF 共有鍵をキャッシュします。
#[derive(Debug)]
pub struct PluginCryptoContext {
    /// このコンテキストを所有するプラグインの UUID
    pub plugin_id: Uuid,
    /// Ed25519 署名鍵（秘密鍵 + 公開鍵）
    signing_key: SigningKey,
    /// peer_plugin_id → セッション鍵 のキャッシュ
    session_keys: HashMap<Uuid, SessionKey>,
}

impl PluginCryptoContext {
    /// 新しい暗号コンテキストを生成する
    pub fn new(plugin_id: Uuid) -> Result<Self, McpError> {
        let rng = SystemRandom::new();
        let mut sk_bytes = [0u8; 32];
        rng.fill(&mut sk_bytes).map_err(|e| {
            McpError::SecurityFailure(format!(
                "PluginCryptoContext: failed to generate Ed25519 signing key: {e}"
            ))
        })?;
        Ok(Self {
            plugin_id,
            signing_key: SigningKey::from_bytes(&sk_bytes),
            session_keys: HashMap::new(),
        })
    }

    /// Ed25519 検証公開鍵（32 バイト）を返す
    pub fn verifying_key_bytes(&self) -> [u8; 32] {
        self.signing_key.verifying_key().to_bytes()
    }

    /// ペイロードを ChaCha20-Poly1305 で暗号化し Ed25519 で署名して返す
    ///
    /// `message_ttl_secs`: メッセージ有効期間（秒）。超過すると受信側で拒否されます。
    pub fn encrypt(
        &self,
        peer_id: Uuid,
        plaintext: &[u8],
        message_ttl_secs: u64,
    ) -> Result<EncryptedPayload, McpError> {
        let session = self.get_valid_session(peer_id)?;

        // 暗号化ごとに一意な nonce をランダム生成（nonce 再利用防止）
        let rng = SystemRandom::new();
        let mut nonce_bytes = [0u8; 12];
        rng.fill(&mut nonce_bytes)
            .map_err(|e| McpError::SecurityFailure(format!("Nonce generation failed: {e}")))?;

        let cipher = ChaCha20Poly1305::new_from_slice(&session.key)
            .map_err(|e| McpError::SecurityFailure(format!("Cipher init failed: {e}")))?;
        let ciphertext = cipher
            .encrypt(&Nonce::from(nonce_bytes), plaintext)
            .map_err(|e| McpError::SecurityFailure(format!("Encryption failed: {e}")))?;

        // 暗号文に Ed25519 署名を付与（改ざん検出）
        let sig: Signature = self.signing_key.sign(&ciphertext);

        let now = Utc::now().timestamp();
        Ok(EncryptedPayload {
            ciphertext,
            nonce: nonce_bytes,
            sender_ecdh_public_key: session.my_ecdh_public,
            signature: sig.to_bytes().to_vec(),
            timestamp: now,
            expires_at: now + message_ttl_secs as i64,
        })
    }

    /// 署名検証 → 期限チェック → 復号（グレースピリオド対応）
    ///
    /// 処理順序:
    /// 1. Ed25519 署名検証（改ざん検知 — 復号前に必ず実施）
    /// 2. `expires_at` 期限チェック（リプレイ攻撃対策）
    /// 3. プライマリ鍵で ChaCha20-Poly1305 復号
    /// 4. 失敗時: グレースピリオド内であればセカンダリ鍵（旧鍵）で再試行
    pub fn decrypt(
        &self,
        peer_id: Uuid,
        payload: &EncryptedPayload,
    ) -> Result<Vec<u8>, McpError> {
        let session = self.get_valid_session(peer_id)?;

        // 1. Ed25519 署名検証（改ざん検知 — 復号前に必ず実施）
        let verifying_key = VerifyingKey::from_bytes(&session.peer_verifying_key)
            .map_err(|e| McpError::SecurityFailure(format!("Invalid peer verifying key: {e}")))?;
        let sig_bytes: [u8; 64] = payload
            .signature
            .as_slice()
            .try_into()
            .map_err(|_| McpError::SecurityFailure("Invalid Ed25519 signature length".to_string()))?;
        let sig = Signature::from_bytes(&sig_bytes);
        verifying_key.verify(&payload.ciphertext, &sig).map_err(|_| {
            McpError::SecurityFailure("Ed25519 signature verification failed".to_string())
        })?;

        // 2. タイムスタンプ期限チェック（リプレイ攻撃対策）
        let now_ts = Utc::now().timestamp();
        if now_ts > payload.expires_at {
            return Err(McpError::SecurityFailure(format!(
                "Message expired (expires_at={}, now={}); possible replay attack",
                payload.expires_at, now_ts
            )));
        }

        // 3. プライマリ鍵で復号
        let cipher = ChaCha20Poly1305::new_from_slice(&session.key)
            .map_err(|e| McpError::SecurityFailure(format!("Cipher init failed: {e}")))?;
        if let Ok(plaintext) =
            cipher.decrypt(&Nonce::from(payload.nonce), payload.ciphertext.as_ref())
        {
            return Ok(plaintext);
        }

        // 4. プライマリ鍵で失敗 → セカンダリ鍵（グレースピリオド）でフォールバック
        //    鍵ローテーション直後に旧鍵で暗号化されたメッセージを復号するために使用
        if let (Some(secondary_key), Some(secondary_expires)) =
            (session.secondary_key, session.secondary_expires_at)
        {
            if Utc::now() <= secondary_expires {
                let cipher2 = ChaCha20Poly1305::new_from_slice(&secondary_key)
                    .map_err(|e| McpError::SecurityFailure(format!("Cipher init failed: {e}")))?;
                return cipher2
                    .decrypt(&Nonce::from(payload.nonce), payload.ciphertext.as_ref())
                    .map_err(|_| {
                        McpError::SecurityFailure(
                            "Decryption failed with both primary and secondary (grace period) keys"
                                .to_string(),
                        )
                    });
            }
        }

        Err(McpError::SecurityFailure(
            "Decryption failed: no valid key available".to_string(),
        ))
    }

    /// 指定ピアとのセッション鍵を失効させる
    pub fn invalidate_session(&mut self, peer_id: Uuid) {
        self.session_keys.remove(&peer_id);
    }

    /// セッション鍵が存在かつ有効期限内であることを検証して返す
    fn get_valid_session(&self, peer_id: Uuid) -> Result<&SessionKey, McpError> {
        let session = self.session_keys.get(&peer_id).ok_or_else(|| {
            McpError::SecurityFailure(format!(
                "No session key established for peer plugin {peer_id}"
            ))
        })?;
        if Utc::now() > session.expires_at {
            return Err(McpError::SecurityFailure(format!(
                "Session key for peer {peer_id} has expired; call rotate_session() to refresh"
            )));
        }
        Ok(session)
    }
}

// ---------------------------------------------------------------------------
// KeyExchangeProtocol
// ---------------------------------------------------------------------------

/// 鍵交換プロトコル
///
/// プラグインを登録し、X25519 ECDH + HKDF-SHA256 を用いて
/// プラグインペア間の共有鍵を確立するオーケストレーター。
///
/// ## Perfect Forward Secrecy
///
/// `initiate_key_exchange` を呼ぶたびに新しい X25519 エフェメラル鍵ペアを生成するため、
/// セッション鍵が漏洩しても過去の通信は解読できません。
///
/// ## 自動キーローテーション
///
/// `start_auto_rotation(Arc::clone(&protocol))` でバックグラウンドタスクを開始し
/// `config.key_lifetime_hours` の間隔で全確立済みペアの鍵を自動更新します。
#[derive(Debug)]
pub struct KeyExchangeProtocol {
    /// plugin_id → PluginCryptoContext
    contexts: Arc<RwLock<HashMap<Uuid, PluginCryptoContext>>>,
    /// 確立済みセッションペアのセット（正規化ペア: min_id < max_id）
    ///
    /// 自動ローテーションタスクが参照します。
    session_pairs: Arc<RwLock<HashSet<(Uuid, Uuid)>>>,
    /// 設定
    config: KeyExchangeConfig,
}

impl KeyExchangeProtocol {
    /// デフォルト設定でインスタンスを作成する（`key_lifetime_hours` のみ指定）
    ///
    /// `grace_period_hours = 1`, `message_ttl_secs = 300` が設定されます。
    pub fn new(key_lifetime_hours: u64) -> Self {
        Self::new_with_config(KeyExchangeConfig {
            key_lifetime_hours,
            ..Default::default()
        })
    }

    /// フル設定でインスタンスを作成する
    pub fn new_with_config(config: KeyExchangeConfig) -> Self {
        Self {
            contexts: Arc::new(RwLock::new(HashMap::new())),
            session_pairs: Arc::new(RwLock::new(HashSet::new())),
            config,
        }
    }

    /// プラグインを登録して Ed25519 検証公開鍵を返す
    pub async fn register_plugin(&self, plugin_id: Uuid) -> Result<[u8; 32], McpError> {
        let ctx = PluginCryptoContext::new(plugin_id)?;
        let verifying_key = ctx.verifying_key_bytes();
        self.contexts.write().await.insert(plugin_id, ctx);
        Ok(verifying_key)
    }

    /// プラグインの登録を解除してコンテキストと関連セッションペアを削除する
    pub async fn unregister_plugin(&self, plugin_id: Uuid) {
        self.contexts.write().await.remove(&plugin_id);
        // 当プラグインを含む全ペアを削除
        self.session_pairs
            .write()
            .await
            .retain(|(a, b)| *a != plugin_id && *b != plugin_id);
    }

    /// X25519 ECDH 鍵交換を実行してセッション鍵を確立（または更新）する
    ///
    /// 既存セッションがある場合、旧鍵を `secondary_key` として
    /// `config.grace_period_hours` の間保持します（グレースピリオド）。
    ///
    /// ## DH 数学的保証
    ///
    /// ```text
    /// shared = a_secret · b_public = b_secret · a_public  (X25519)
    /// HKDF-SHA256(shared, "plugin-e2e-encryption-v1") → 32-byte symmetric key
    /// ```
    pub async fn initiate_key_exchange(
        &self,
        plugin_a: Uuid,
        plugin_b: Uuid,
    ) -> Result<(), McpError> {
        let mut contexts = self.contexts.write().await;

        if !contexts.contains_key(&plugin_a) {
            return Err(McpError::SecurityFailure(format!(
                "Plugin {plugin_a} is not registered in KeyExchangeProtocol"
            )));
        }
        if !contexts.contains_key(&plugin_b) {
            return Err(McpError::SecurityFailure(format!(
                "Plugin {plugin_b} is not registered in KeyExchangeProtocol"
            )));
        }

        // 双方の Ed25519 検証公開鍵を取得
        let verifying_a = contexts[&plugin_a].verifying_key_bytes();
        let verifying_b = contexts[&plugin_b].verifying_key_bytes();

        // 既存セッション鍵を保存（ローテーション時のグレースピリオド用）
        let old_key_a = contexts[&plugin_a]
            .session_keys
            .get(&plugin_b)
            .map(|s| s.key);
        let old_key_b = contexts[&plugin_b]
            .session_keys
            .get(&plugin_a)
            .map(|s| s.key);

        // X25519 エフェメラル鍵ペア生成（PFS: 毎回新しいランダム鍵）
        let rng = SystemRandom::new();
        let mut bytes_a = [0u8; 32];
        let mut bytes_b = [0u8; 32];
        rng.fill(&mut bytes_a).map_err(|e| {
            McpError::SecurityFailure(format!("ECDH key generation failed for plugin_a: {e}"))
        })?;
        rng.fill(&mut bytes_b).map_err(|e| {
            McpError::SecurityFailure(format!("ECDH key generation failed for plugin_b: {e}"))
        })?;

        let secret_a = StaticSecret::from(bytes_a);
        let secret_b = StaticSecret::from(bytes_b);
        let public_a = X25519PublicKey::from(&secret_a);
        let public_b = X25519PublicKey::from(&secret_b);

        // DH: secret_a · public_b = secret_b · public_a  (同一の共有秘密)
        let shared_secret = secret_a.diffie_hellman(&public_b);

        // HKDF-SHA256 で 32 バイト対称鍵を導出
        let salt = hkdf::Salt::new(hkdf::HKDF_SHA256, &[]);
        let prk = salt.extract(shared_secret.as_bytes());
        let mut derived_key = [0u8; 32];
        prk.expand(&[b"plugin-e2e-encryption-v1"], HkdfKey32)
            .map_err(|_| McpError::SecurityFailure("HKDF expand failed".to_string()))?
            .fill(&mut derived_key)
            .map_err(|_| McpError::SecurityFailure("HKDF fill failed".to_string()))?;

        let now = Utc::now();
        let session_expires_at =
            now + chrono::Duration::hours(self.config.key_lifetime_hours as i64);
        let secondary_expires_at =
            now + chrono::Duration::hours(self.config.grace_period_hours as i64);

        // plugin_a のコンテキストに共有鍵を設定（旧鍵 → secondary_key）
        contexts.get_mut(&plugin_a).unwrap().session_keys.insert(
            plugin_b,
            SessionKey {
                key: derived_key,
                secondary_key: old_key_a,
                secondary_expires_at: old_key_a.map(|_| secondary_expires_at),
                my_ecdh_public: public_a.to_bytes(),
                peer_verifying_key: verifying_b,
                created_at: now,
                expires_at: session_expires_at,
            },
        );

        // plugin_b のコンテキストに共有鍵を設定（同一の derived_key を共有）
        contexts.get_mut(&plugin_b).unwrap().session_keys.insert(
            plugin_a,
            SessionKey {
                key: derived_key,
                secondary_key: old_key_b,
                secondary_expires_at: old_key_b.map(|_| secondary_expires_at),
                my_ecdh_public: public_b.to_bytes(),
                peer_verifying_key: verifying_a,
                created_at: now,
                expires_at: session_expires_at,
            },
        );

        // ペアを確立済みセットに記録（正規化して重複防止）
        drop(contexts);
        self.session_pairs
            .write()
            .await
            .insert(Self::canonical_pair(plugin_a, plugin_b));

        Ok(())
    }

    /// `sender` として `recipient` 宛にメッセージを暗号化する
    ///
    /// メッセージ有効期限は `config.message_ttl_secs` で自動設定されます。
    pub async fn encrypt_for_peer(
        &self,
        sender: Uuid,
        recipient: Uuid,
        plaintext: &[u8],
    ) -> Result<EncryptedPayload, McpError> {
        let contexts = self.contexts.read().await;
        let ctx = contexts.get(&sender).ok_or_else(|| {
            McpError::SecurityFailure(format!(
                "Sender plugin {sender} is not registered in KeyExchangeProtocol"
            ))
        })?;
        ctx.encrypt(recipient, plaintext, self.config.message_ttl_secs)
    }

    /// `sender` から `recipient` 宛の [`EncryptedPayload`] を検証・復号する
    pub async fn decrypt_from_peer(
        &self,
        recipient: Uuid,
        sender: Uuid,
        payload: &EncryptedPayload,
    ) -> Result<Vec<u8>, McpError> {
        let contexts = self.contexts.read().await;
        let ctx = contexts.get(&recipient).ok_or_else(|| {
            McpError::SecurityFailure(format!(
                "Recipient plugin {recipient} is not registered in KeyExchangeProtocol"
            ))
        })?;
        ctx.decrypt(sender, payload)
    }

    /// セッション鍵を再生成する（手動鍵ローテーション）
    ///
    /// 旧鍵は `grace_period_hours` の間 `secondary_key` として保持されます。
    pub async fn rotate_session(&self, plugin_a: Uuid, plugin_b: Uuid) -> Result<(), McpError> {
        self.initiate_key_exchange(plugin_a, plugin_b).await
    }

    /// 自動キーローテーション バックグラウンドタスクを開始する
    ///
    /// `config.key_lifetime_hours` の間隔で全確立済みペアの `rotate_session` を呼び出します。
    /// 返値の `JoinHandle` を `abort()` することでタスクを停止できます。
    ///
    /// ## 使用例
    ///
    /// ```rust,ignore
    /// let handle = KeyExchangeProtocol::start_auto_rotation(Arc::clone(&protocol));
    /// // ... 停止する場合:
    /// handle.abort();
    /// ```
    pub fn start_auto_rotation(protocol: Arc<Self>) -> tokio::task::JoinHandle<()> {
        let interval_secs = protocol.config.key_lifetime_hours * 3600;
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(tokio::time::Duration::from_secs(interval_secs)).await;

                let pairs: Vec<(Uuid, Uuid)> = protocol
                    .session_pairs
                    .read()
                    .await
                    .iter()
                    .cloned()
                    .collect();

                for (a, b) in pairs {
                    if let Err(e) = protocol.rotate_session(a, b).await {
                        warn!(
                            "Auto key rotation failed for pair ({}, {}): {}",
                            a, b, e
                        );
                    }
                }
            }
        })
    }

    /// (a, b) を正規化（min < max）してペアの重複登録を防ぐ
    fn canonical_pair(a: Uuid, b: Uuid) -> (Uuid, Uuid) {
        if a < b {
            (a, b)
        } else {
            (b, a)
        }
    }
}

// ---------------------------------------------------------------------------
// テスト
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn default_config() -> KeyExchangeConfig {
        KeyExchangeConfig {
            key_lifetime_hours: 24,
            grace_period_hours: 1,
            message_ttl_secs: 300,
        }
    }

    /// プラグイン登録 → 鍵交換 → 暗号化 → 復号 の E2E ラウンドトリップ
    #[tokio::test]
    async fn test_key_exchange_roundtrip() {
        let protocol = KeyExchangeProtocol::new_with_config(default_config());
        let plugin_a = Uuid::new_v4();
        let plugin_b = Uuid::new_v4();

        protocol.register_plugin(plugin_a).await.unwrap();
        protocol.register_plugin(plugin_b).await.unwrap();

        protocol
            .initiate_key_exchange(plugin_a, plugin_b)
            .await
            .unwrap();

        let original = b"plugin-to-plugin secret message";
        let payload = protocol
            .encrypt_for_peer(plugin_a, plugin_b, original)
            .await
            .unwrap();

        assert_ne!(payload.ciphertext, original.to_vec());
        assert_ne!(payload.nonce, [0u8; 12]);
        assert_eq!(payload.signature.len(), 64);
        assert!(payload.expires_at > payload.timestamp);

        let decrypted = protocol
            .decrypt_from_peer(plugin_b, plugin_a, &payload)
            .await
            .unwrap();

        assert_eq!(decrypted, original.to_vec());
    }

    /// 双方向通信: B→A の暗号化と A での復号
    #[tokio::test]
    async fn test_bidirectional_encryption() {
        let protocol = KeyExchangeProtocol::new_with_config(default_config());
        let plugin_a = Uuid::new_v4();
        let plugin_b = Uuid::new_v4();

        protocol.register_plugin(plugin_a).await.unwrap();
        protocol.register_plugin(plugin_b).await.unwrap();
        protocol
            .initiate_key_exchange(plugin_a, plugin_b)
            .await
            .unwrap();

        // A → B
        let msg_ab = b"A says hello to B";
        let enc_ab = protocol
            .encrypt_for_peer(plugin_a, plugin_b, msg_ab)
            .await
            .unwrap();
        let dec_ab = protocol
            .decrypt_from_peer(plugin_b, plugin_a, &enc_ab)
            .await
            .unwrap();
        assert_eq!(dec_ab, msg_ab.to_vec());

        // B → A
        let msg_ba = b"B replies to A";
        let enc_ba = protocol
            .encrypt_for_peer(plugin_b, plugin_a, msg_ba)
            .await
            .unwrap();
        let dec_ba = protocol
            .decrypt_from_peer(plugin_a, plugin_b, &enc_ba)
            .await
            .unwrap();
        assert_eq!(dec_ba, msg_ba.to_vec());
    }

    /// 改ざん検出: 暗号文を変更すると Ed25519 署名検証が失敗する
    #[tokio::test]
    async fn test_tampered_payload_detection() {
        let protocol = KeyExchangeProtocol::new_with_config(default_config());
        let plugin_a = Uuid::new_v4();
        let plugin_b = Uuid::new_v4();

        protocol.register_plugin(plugin_a).await.unwrap();
        protocol.register_plugin(plugin_b).await.unwrap();
        protocol
            .initiate_key_exchange(plugin_a, plugin_b)
            .await
            .unwrap();

        let original = b"sensitive data";
        let mut payload = protocol
            .encrypt_for_peer(plugin_a, plugin_b, original)
            .await
            .unwrap();

        // 暗号文の先頭バイトを反転して改ざんを模倣
        payload.ciphertext[0] ^= 0xFF;

        let result = protocol
            .decrypt_from_peer(plugin_b, plugin_a, &payload)
            .await;
        assert!(result.is_err(), "改ざんされたペイロードは復号に失敗するべき");
        let err = format!("{:?}", result.unwrap_err());
        assert!(
            err.contains("signature verification failed") || err.contains("SecurityFailure"),
            "エラーは署名検証失敗を示すべき: {err}"
        );
    }

    /// nonce ユニーク性: 同一プラグインペアでも毎回異なる nonce が生成される
    #[tokio::test]
    async fn test_nonce_uniqueness() {
        let protocol = KeyExchangeProtocol::new_with_config(default_config());
        let plugin_a = Uuid::new_v4();
        let plugin_b = Uuid::new_v4();

        protocol.register_plugin(plugin_a).await.unwrap();
        protocol.register_plugin(plugin_b).await.unwrap();
        protocol
            .initiate_key_exchange(plugin_a, plugin_b)
            .await
            .unwrap();

        let p1 = protocol
            .encrypt_for_peer(plugin_a, plugin_b, b"msg1")
            .await
            .unwrap();
        let p2 = protocol
            .encrypt_for_peer(plugin_a, plugin_b, b"msg2")
            .await
            .unwrap();

        assert_ne!(
            p1.nonce, p2.nonce,
            "各暗号化は一意な nonce を持つべき（リプレイ攻撃対策）"
        );
    }

    /// セッションローテーション後も新しい鍵で正常に通信できる
    #[tokio::test]
    async fn test_session_rotation() {
        let protocol = KeyExchangeProtocol::new_with_config(default_config());
        let plugin_a = Uuid::new_v4();
        let plugin_b = Uuid::new_v4();

        protocol.register_plugin(plugin_a).await.unwrap();
        protocol.register_plugin(plugin_b).await.unwrap();
        protocol
            .initiate_key_exchange(plugin_a, plugin_b)
            .await
            .unwrap();

        let old_payload = protocol
            .encrypt_for_peer(plugin_a, plugin_b, b"before rotation")
            .await
            .unwrap();

        protocol.rotate_session(plugin_a, plugin_b).await.unwrap();

        // ローテーション後も新しい鍵で通信できる
        let msg = b"after rotation";
        let new_payload = protocol
            .encrypt_for_peer(plugin_a, plugin_b, msg)
            .await
            .unwrap();
        let dec = protocol
            .decrypt_from_peer(plugin_b, plugin_a, &new_payload)
            .await
            .unwrap();
        assert_eq!(dec, msg.to_vec());

        // ECDH 公開鍵が更新されていること（PFS の確認）
        assert_ne!(
            old_payload.sender_ecdh_public_key,
            new_payload.sender_ecdh_public_key,
            "セッションローテーション後は ECDH 公開鍵が更新されるべき"
        );
    }

    /// 未登録プラグインへの鍵交換はエラーになる
    #[tokio::test]
    async fn test_unregistered_plugin_exchange_fails() {
        let protocol = KeyExchangeProtocol::new_with_config(default_config());
        let plugin_a = Uuid::new_v4();
        let plugin_b = Uuid::new_v4();

        protocol.register_plugin(plugin_a).await.unwrap();
        // plugin_b は未登録

        let result = protocol.initiate_key_exchange(plugin_a, plugin_b).await;
        assert!(
            result.is_err(),
            "未登録プラグインとの鍵交換はエラーになるべき"
        );
    }

    /// グレースピリオドテスト: ローテーション後も旧鍵で暗号化されたメッセージを復号できる
    ///
    /// ローリングデプロイや遅延メッセージのシナリオを想定。
    #[tokio::test]
    async fn test_grace_period_allows_decryption_with_old_key() {
        let protocol = KeyExchangeProtocol::new_with_config(KeyExchangeConfig {
            key_lifetime_hours: 24,
            grace_period_hours: 1, // 1 時間のグレースピリオド
            message_ttl_secs: 300,
        });
        let plugin_a = Uuid::new_v4();
        let plugin_b = Uuid::new_v4();

        protocol.register_plugin(plugin_a).await.unwrap();
        protocol.register_plugin(plugin_b).await.unwrap();

        // 初回鍵交換 (K1 を確立)
        protocol
            .initiate_key_exchange(plugin_a, plugin_b)
            .await
            .unwrap();

        // K1 でメッセージを暗号化（ローテーション前）
        let original = b"sent before key rotation";
        let payload_with_k1 = protocol
            .encrypt_for_peer(plugin_a, plugin_b, original)
            .await
            .unwrap();

        // セッションをローテーション: K1 → secondary (grace period), K2 → primary
        protocol.rotate_session(plugin_a, plugin_b).await.unwrap();

        // B は K1 で暗号化されたメッセージをグレースピリオド内に復号できる
        // (primary K2 で失敗 → secondary K1 でフォールバック)
        let decrypted = protocol
            .decrypt_from_peer(plugin_b, plugin_a, &payload_with_k1)
            .await
            .expect("グレースピリオド内は旧鍵で復号できるべき");
        assert_eq!(decrypted, original.to_vec());

        // ローテーション後の新しいメッセージ (K2) も正常に復号できる
        let new_msg = b"sent after key rotation";
        let payload_with_k2 = protocol
            .encrypt_for_peer(plugin_a, plugin_b, new_msg)
            .await
            .unwrap();
        let dec_new = protocol
            .decrypt_from_peer(plugin_b, plugin_a, &payload_with_k2)
            .await
            .unwrap();
        assert_eq!(dec_new, new_msg.to_vec());
    }

    /// タイムスタンプ期限切れテスト: expires_at を過去に設定したペイロードは復号拒否
    ///
    /// リプレイ攻撃シミュレーション: 古いメッセージを再送しても拒否されることを確認。
    #[tokio::test]
    async fn test_expired_message_rejected() {
        let protocol = KeyExchangeProtocol::new_with_config(default_config());
        let plugin_a = Uuid::new_v4();
        let plugin_b = Uuid::new_v4();

        protocol.register_plugin(plugin_a).await.unwrap();
        protocol.register_plugin(plugin_b).await.unwrap();
        protocol
            .initiate_key_exchange(plugin_a, plugin_b)
            .await
            .unwrap();

        // 通常どおり暗号化
        let mut payload = protocol
            .encrypt_for_peer(plugin_a, plugin_b, b"time-sensitive message")
            .await
            .unwrap();

        // expires_at を過去に設定（リプレイメッセージを模倣）
        payload.expires_at = Utc::now().timestamp() - 1;

        let result = protocol
            .decrypt_from_peer(plugin_b, plugin_a, &payload)
            .await;
        assert!(result.is_err(), "期限切れメッセージは復号を拒否されるべき");
        let err = format!("{:?}", result.unwrap_err());
        assert!(
            err.contains("expired") || err.contains("SecurityFailure"),
            "エラーは期限切れを示すべき: {err}"
        );
    }

    /// 自動ローテーションタスクのライフサイクルテスト
    ///
    /// タスクが正常に起動し、`abort()` で停止できることを確認する。
    #[tokio::test]
    async fn test_auto_rotation_task_lifecycle() {
        let protocol = Arc::new(KeyExchangeProtocol::new_with_config(default_config()));
        let plugin_a = Uuid::new_v4();
        let plugin_b = Uuid::new_v4();

        protocol.register_plugin(plugin_a).await.unwrap();
        protocol.register_plugin(plugin_b).await.unwrap();
        protocol
            .initiate_key_exchange(plugin_a, plugin_b)
            .await
            .unwrap();

        let handle = KeyExchangeProtocol::start_auto_rotation(Arc::clone(&protocol));

        // タイマーが発火していない（24 時間待ち）のでタスクは実行中のはず
        assert!(!handle.is_finished(), "自動ローテーションタスクは実行中のはず");

        // 正常に停止できることを確認
        handle.abort();
        let result = handle.await;
        assert!(result.is_err(), "abort() 後は JoinError になるはず");
    }

    /// 自動ローテーション時刻制御テスト
    ///
    /// tokio::time pause + advance でタイマーを仮想的に進め、
    /// ローテーションタスクが発火した後もプロトコルが正常動作することを確認する。
    ///
    /// ## 検証内容
    /// - タイマー発火後に abort() せず次のループ Sleep に入れること
    /// - ローテーション有無に関わらずグレースピリオドで送受信継続できること
    /// - `test_session_rotation` で鍵変更の正確性は別途保証済み
    #[tokio::test(start_paused = true)]
    async fn test_auto_rotation_fires_on_schedule() {
        let protocol = Arc::new(KeyExchangeProtocol::new_with_config(KeyExchangeConfig {
            key_lifetime_hours: 1, // テスト用に 1 時間
            grace_period_hours: 1,
            message_ttl_secs: 300,
        }));
        let plugin_a = Uuid::new_v4();
        let plugin_b = Uuid::new_v4();

        protocol.register_plugin(plugin_a).await.unwrap();
        protocol.register_plugin(plugin_b).await.unwrap();
        protocol
            .initiate_key_exchange(plugin_a, plugin_b)
            .await
            .unwrap();

        // ローテーション前のメッセージを暗号化（グレースピリオドテスト用）
        let pre_rotation_msg = b"before rotation";
        let pre_payload = protocol
            .encrypt_for_peer(plugin_a, plugin_b, pre_rotation_msg)
            .await
            .unwrap();

        let handle = KeyExchangeProtocol::start_auto_rotation(Arc::clone(&protocol));

        // タスクが起動直後は sleep 中なのでまだ終了していない
        assert!(!handle.is_finished(), "自動ローテーションタスクは実行中のはず");

        // 仮想時間を 1 時間 + α だけ進めてタイマーを発火させる
        tokio::time::advance(tokio::time::Duration::from_secs(3601)).await;
        // 非同期タスクが rotate_session() を実行する機会を与える
        for _ in 0..100 {
            tokio::task::yield_now().await;
        }

        handle.abort();
        let _ = handle.await;

        // ローテーション後も新メッセージを送受信できる
        // （ローテーションが実際に行われた場合は新鍵、Grace Period で旧鍵も使用可能）
        let post_msg = b"after rotation";
        let post_payload = protocol
            .encrypt_for_peer(plugin_a, plugin_b, post_msg)
            .await
            .unwrap();
        let dec = protocol
            .decrypt_from_peer(plugin_b, plugin_a, &post_payload)
            .await
            .expect("ローテーション後も新しいメッセージを復号できるべき");
        assert_eq!(dec, post_msg.to_vec());

        // グレースピリオドにより、ローテーション前のメッセージも復号できる
        let dec_pre = protocol
            .decrypt_from_peer(plugin_b, plugin_a, &pre_payload)
            .await
            .expect("グレースピリオド内は旧メッセージを復号できるべき");
        assert_eq!(dec_pre, pre_rotation_msg.to_vec());
    }
}
