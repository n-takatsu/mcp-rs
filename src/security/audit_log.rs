//! 包括的監査ログシステム
//! セキュリティイベント、攻撃の試み、システムアクセスの詳細な記録

use crate::error::SecurityError;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;
use tokio::io::AsyncWriteExt;
use tokio::sync::{Mutex, RwLock};
use uuid::Uuid;

/// 監査ログエントリのレベル
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AuditLevel {
    /// 情報レベル - 通常のアクセス・操作
    Info,
    /// 警告レベル - 疑わしい活動
    Warning,
    /// エラーレベル - セキュリティ違反
    Error,
    /// 重大レベル - システムに対する脅威
    Critical,
}

impl fmt::Display for AuditLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AuditLevel::Info => write!(f, "INFO"),
            AuditLevel::Warning => write!(f, "WARN"),
            AuditLevel::Error => write!(f, "ERROR"),
            AuditLevel::Critical => write!(f, "CRITICAL"),
        }
    }
}

/// 監査ログカテゴリ
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum AuditCategory {
    /// 認証関連イベント
    Authentication,
    /// 認可関連イベント
    Authorization,
    /// データアクセス
    DataAccess,
    /// セキュリティ攻撃
    SecurityAttack,
    /// システム設定変更
    ConfigChange,
    /// ネットワークアクティビティ
    NetworkActivity,
    /// エラー・例外
    Error,
}

impl fmt::Display for AuditCategory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AuditCategory::Authentication => write!(f, "AUTH"),
            AuditCategory::Authorization => write!(f, "AUTHZ"),
            AuditCategory::DataAccess => write!(f, "DATA"),
            AuditCategory::SecurityAttack => write!(f, "ATTACK"),
            AuditCategory::ConfigChange => write!(f, "CONFIG"),
            AuditCategory::NetworkActivity => write!(f, "NETWORK"),
            AuditCategory::Error => write!(f, "ERROR"),
        }
    }
}

/// 監査ログエントリ
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditLogEntry {
    /// 一意の識別子
    pub id: Uuid,
    /// タイムスタンプ（UTC）
    pub timestamp: DateTime<Utc>,
    /// ログレベル
    pub level: AuditLevel,
    /// カテゴリ
    pub category: AuditCategory,
    /// イベントの説明
    pub message: String,
    /// ユーザーID（存在する場合）
    pub user_id: Option<String>,
    /// セッションID（存在する場合）
    pub session_id: Option<String>,
    /// IPアドレス
    pub ip_address: Option<String>,
    /// ユーザーエージェント
    pub user_agent: Option<String>,
    /// 追加のメタデータ
    pub metadata: HashMap<String, String>,
    /// 要求されたリソース
    pub resource: Option<String>,
    /// 実行されたアクション
    pub action: Option<String>,
    /// 結果（成功/失敗）
    pub result: Option<String>,
}

impl AuditLogEntry {
    /// 新しい監査ログエントリを作成
    pub fn new(level: AuditLevel, category: AuditCategory, message: String) -> Self {
        Self {
            id: Uuid::new_v4(),
            timestamp: Utc::now(),
            level,
            category,
            message,
            user_id: None,
            session_id: None,
            ip_address: None,
            user_agent: None,
            metadata: HashMap::new(),
            resource: None,
            action: None,
            result: None,
        }
    }

    /// ユーザー情報を設定
    pub fn with_user(mut self, user_id: String) -> Self {
        self.user_id = Some(user_id);
        self
    }

    /// セッション情報を設定
    pub fn with_session(mut self, session_id: String) -> Self {
        self.session_id = Some(session_id);
        self
    }

    /// リクエスト情報を設定
    pub fn with_request_info(mut self, ip: String, user_agent: String) -> Self {
        self.ip_address = Some(ip);
        self.user_agent = Some(user_agent);
        self
    }

    /// リソースとアクションを設定
    pub fn with_action(mut self, resource: String, action: String) -> Self {
        self.resource = Some(resource);
        self.action = Some(action);
        self
    }

    /// 結果を設定
    pub fn with_result(mut self, result: String) -> Self {
        self.result = Some(result);
        self
    }

    /// メタデータを追加
    pub fn add_metadata(mut self, key: String, value: String) -> Self {
        self.metadata.insert(key, value);
        self
    }
}

/// 監査ログ統計情報
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditStatistics {
    /// 総ログエントリ数
    pub total_entries: u64,
    /// レベル別エントリ数
    pub entries_by_level: HashMap<AuditLevel, u64>,
    /// カテゴリ別エントリ数
    pub entries_by_category: HashMap<AuditCategory, u64>,
    /// 過去24時間のエントリ数
    pub entries_last_24h: u64,
    /// 最初のエントリのタイムスタンプ
    pub first_entry_time: Option<DateTime<Utc>>,
    /// 最後のエントリのタイムスタンプ
    pub last_entry_time: Option<DateTime<Utc>>,
}

/// 監査ログフィルター条件
#[derive(Debug, Clone, Default)]
pub struct AuditFilter {
    /// 開始時刻
    pub start_time: Option<DateTime<Utc>>,
    /// 終了時刻
    pub end_time: Option<DateTime<Utc>>,
    /// ログレベル
    pub levels: Option<Vec<AuditLevel>>,
    /// カテゴリ
    pub categories: Option<Vec<AuditCategory>>,
    /// ユーザーID
    pub user_id: Option<String>,
    /// IPアドレス
    pub ip_address: Option<String>,
    /// 検索キーワード
    pub keyword: Option<String>,
}

/// 監査ログシステム設定
#[derive(Debug, Clone)]
pub struct AuditConfig {
    /// メモリ内保持する最大エントリ数
    pub max_memory_entries: usize,
    /// ログレベルフィルター（これ以下のレベルは記録しない）
    pub min_log_level: AuditLevel,
    /// ファイル出力を有効にするか（`log_file_path`も必要）
    pub enable_file_output: bool,
    /// ログファイルパス
    pub log_file_path: Option<String>,
    /// JSON形式で出力するか
    pub json_format: bool,
    /// サイズでローテーションするか
    pub rotation_enabled: bool,
    /// この大きさ（バイト）を超える前に、ファイルをローテーションする
    pub rotation_size: u64,
    /// ローテーションで残す古いファイルの数（`audit.log.1`〜）
    pub max_files: u32,
}

impl Default for AuditConfig {
    fn default() -> Self {
        Self {
            max_memory_entries: 10000,
            min_log_level: AuditLevel::Info,
            // 既定ではファイルに書かない（メモリのみ）。以前は作業フォルダの
            // `audit.log`に書いていたが、作業フォルダは起動のしかたで変わり、
            // 書き込めない場所では記録されなかった。本体は設定の`[audit]`から
            // 書き込み先を決める（#416）
            enable_file_output: false,
            log_file_path: None,
            json_format: true,
            rotation_enabled: true,
            rotation_size: 100 * 1024 * 1024, // 100MB
            max_files: 5,
        }
    }
}

/// 監査ログシステム
#[derive(Debug)]
pub struct AuditLogger {
    /// 設定
    config: AuditConfig,
    /// メモリ内ログエントリ
    entries: Arc<RwLock<Vec<AuditLogEntry>>>,
    /// 統計情報
    statistics: Arc<Mutex<AuditStatistics>>,
    /// ファイルへの書き込み（とローテーション）を1件ずつにする
    file_lock: Arc<Mutex<()>>,
}

impl AuditLogger {
    /// 新しい監査ログシステムを作成
    pub fn new(config: AuditConfig) -> Self {
        Self {
            config,
            entries: Arc::new(RwLock::new(Vec::new())),
            statistics: Arc::new(Mutex::new(AuditStatistics {
                total_entries: 0,
                entries_by_level: HashMap::new(),
                entries_by_category: HashMap::new(),
                entries_last_24h: 0,
                first_entry_time: None,
                last_entry_time: None,
            })),
            file_lock: Arc::new(Mutex::new(())),
        }
    }

    /// ファイルに書く設定なら、その書き込み先。
    pub fn file_path(&self) -> Option<&str> {
        if self.config.enable_file_output {
            self.config.log_file_path.as_deref()
        } else {
            None
        }
    }

    /// ファイルに書く設定なら、書き込めるかを確かめる（起動時に、書き
    /// 込めない場合を知らせるため）。フォルダが無ければ作る。
    pub async fn check_file_output(&self) -> Result<(), String> {
        let Some(path) = self.file_path() else {
            return Ok(());
        };
        if let Some(parent) = std::path::Path::new(path).parent() {
            if !parent.as_os_str().is_empty() {
                tokio::fs::create_dir_all(parent)
                    .await
                    .map_err(|e| format!("could not create {}: {}", parent.display(), e))?;
            }
        }
        tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .await
            .map(|_| ())
            .map_err(|e| format!("could not open {}: {}", path, e))
    }

    /// デフォルト設定で監査ログシステムを作成
    pub fn with_defaults() -> Self {
        Self::new(AuditConfig::default())
    }

    /// ログエントリを記録
    pub async fn log(&self, entry: AuditLogEntry) -> Result<(), SecurityError> {
        // レベルフィルタリング
        if entry.level < self.config.min_log_level {
            return Ok(());
        }

        // メモリに保存
        {
            let mut entries = self.entries.write().await;
            entries.push(entry.clone());

            // メモリ制限チェック
            if entries.len() > self.config.max_memory_entries {
                entries.remove(0); // 古いエントリを削除
            }
        }

        // 統計更新
        {
            let mut stats = self.statistics.lock().await;
            stats.total_entries += 1;

            *stats
                .entries_by_level
                .entry(entry.level.clone())
                .or_insert(0) += 1;
            *stats
                .entries_by_category
                .entry(entry.category.clone())
                .or_insert(0) += 1;

            if stats.first_entry_time.is_none() {
                stats.first_entry_time = Some(entry.timestamp);
            }
            stats.last_entry_time = Some(entry.timestamp);

            // 過去24時間のエントリ数を更新
            let now = Utc::now();
            let yesterday = now - chrono::Duration::hours(24);
            if entry.timestamp > yesterday {
                stats.entries_last_24h += 1;
            }
        }

        // ファイル出力（設定されている場合）
        if self.file_path().is_some() {
            if let Err(e) = self.write_to_file(&entry).await {
                eprintln!("Failed to write audit log to file: {}", e);
            }
        }

        Ok(())
    }

    /// セキュリティ攻撃ログ
    pub async fn log_security_attack(
        &self,
        attack_type: &str,
        details: &str,
        ip: Option<String>,
        user_agent: Option<String>,
    ) -> Result<(), SecurityError> {
        let mut entry = AuditLogEntry::new(
            AuditLevel::Critical,
            AuditCategory::SecurityAttack,
            format!("Security attack detected: {} - {}", attack_type, details),
        );

        if let Some(ip) = ip {
            entry.ip_address = Some(ip);
        }
        if let Some(ua) = user_agent {
            entry.user_agent = Some(ua);
        }

        entry = entry.add_metadata("attack_type".to_string(), attack_type.to_string());

        self.log(entry).await
    }

    /// 認証ログ
    pub async fn log_authentication(
        &self,
        user_id: &str,
        success: bool,
        ip: Option<String>,
    ) -> Result<(), SecurityError> {
        let level = if success {
            AuditLevel::Info
        } else {
            AuditLevel::Warning
        };
        let message = if success {
            format!("User authentication successful: {}", user_id)
        } else {
            format!("User authentication failed: {}", user_id)
        };

        let mut entry = AuditLogEntry::new(level, AuditCategory::Authentication, message)
            .with_user(user_id.to_string())
            .with_result(if success { "SUCCESS" } else { "FAILURE" }.to_string());

        if let Some(ip) = ip {
            entry.ip_address = Some(ip);
        }

        self.log(entry).await
    }

    /// データアクセスログ
    pub async fn log_data_access(
        &self,
        user_id: &str,
        resource: &str,
        action: &str,
        success: bool,
    ) -> Result<(), SecurityError> {
        let level = if success {
            AuditLevel::Info
        } else {
            AuditLevel::Error
        };
        let message = format!(
            "Data access: {} {} {} - {}",
            user_id,
            action,
            resource,
            if success { "SUCCESS" } else { "DENIED" }
        );

        let entry = AuditLogEntry::new(level, AuditCategory::DataAccess, message)
            .with_user(user_id.to_string())
            .with_action(resource.to_string(), action.to_string())
            .with_result(if success { "SUCCESS" } else { "DENIED" }.to_string());

        self.log(entry).await
    }

    /// ログエントリを検索
    pub async fn search(&self, filter: AuditFilter) -> Vec<AuditLogEntry> {
        let entries = self.entries.read().await;

        entries
            .iter()
            .filter(|entry| self.matches_filter(entry, &filter))
            .cloned()
            .collect()
    }

    /// 統計情報を取得
    pub async fn get_statistics(&self) -> AuditStatistics {
        let stats = self.statistics.lock().await;
        stats.clone()
    }

    /// すべてのログエントリを取得
    pub async fn get_all_entries(&self) -> Vec<AuditLogEntry> {
        let entries = self.entries.read().await;
        entries.clone()
    }

    /// 最新のログエントリを取得
    pub async fn get_recent_entries(&self, count: usize) -> Vec<AuditLogEntry> {
        let entries = self.entries.read().await;
        let start = if entries.len() > count {
            entries.len() - count
        } else {
            0
        };
        entries[start..].to_vec()
    }

    /// ログをクリア
    pub async fn clear_logs(&self) -> Result<(), SecurityError> {
        {
            let mut entries = self.entries.write().await;
            entries.clear();
        }

        {
            let mut stats = self.statistics.lock().await;
            *stats = AuditStatistics {
                total_entries: 0,
                entries_by_level: HashMap::new(),
                entries_by_category: HashMap::new(),
                entries_last_24h: 0,
                first_entry_time: None,
                last_entry_time: None,
            };
        }

        Ok(())
    }

    /// このユーザーの過去の監査ログエントリを削除せず redact する。
    /// GDPR Art.17(3)(b)/(e)（法的義務・権利の防御）により、セキュリティ
    /// 監査証跡は構造を保持したまま残す必要があるため、エントリ自体は
    /// 削除せず、PIIやオンライン識別子を含みうるフィールド（メッセージ
    /// 本文・IP・UA・セッションID・メタデータ）のみを消去する。
    ///
    /// `user_id`自体は意図的に残す（どのアカウントに対する処理だったかを
    /// 後から監査・調査できるようにするため）。そのためこれは完全な
    /// 匿名化（anonymization）ではなく、残す識別子を`user_id`に限定した
    /// redactionである点に注意。
    pub async fn redact_user(&self, user_id: &str) -> usize {
        let mut entries = self.entries.write().await;
        let mut count = 0;
        for entry in entries.iter_mut() {
            if entry.user_id.as_deref() == Some(user_id) {
                entry.message = "[REDACTED: GDPR/CCPA erasure request]".to_string();
                entry.ip_address = None;
                entry.user_agent = None;
                entry.session_id = None;
                entry.metadata.clear();
                entry
                    .metadata
                    .insert("gdpr_redacted".to_string(), "true".to_string());
                count += 1;
            }
        }
        count
    }

    /// フィルター条件にマッチするかチェック
    fn matches_filter(&self, entry: &AuditLogEntry, filter: &AuditFilter) -> bool {
        // 時間範囲チェック
        if let Some(start) = filter.start_time {
            if entry.timestamp < start {
                return false;
            }
        }
        if let Some(end) = filter.end_time {
            if entry.timestamp > end {
                return false;
            }
        }

        // レベルチェック
        if let Some(ref levels) = filter.levels {
            if !levels.contains(&entry.level) {
                return false;
            }
        }

        // カテゴリチェック
        if let Some(ref categories) = filter.categories {
            if !categories.contains(&entry.category) {
                return false;
            }
        }

        // ユーザーIDチェック
        if let Some(ref user_id) = filter.user_id {
            if entry.user_id.as_ref() != Some(user_id) {
                return false;
            }
        }

        // IPアドレスチェック
        if let Some(ref ip) = filter.ip_address {
            if entry.ip_address.as_ref() != Some(ip) {
                return false;
            }
        }

        // キーワードチェック
        if let Some(ref keyword) = filter.keyword {
            let keyword_lower = keyword.to_lowercase();
            if !entry.message.to_lowercase().contains(&keyword_lower) {
                return false;
            }
        }

        true
    }

    /// ファイルに書き込み
    async fn write_to_file(&self, entry: &AuditLogEntry) -> Result<(), SecurityError> {
        if let Some(file_path) = self.file_path() {
            let content = if self.config.json_format {
                serde_json::to_string(entry).map_err(|e| {
                    SecurityError::Configuration(format!("JSON serialization failed: {}", e))
                })?
            } else {
                format!(
                    "{} [{}] [{}] {} - {}",
                    entry.timestamp.format("%Y-%m-%d %H:%M:%S UTC"),
                    entry.level,
                    entry.category,
                    entry.user_id.as_deref().unwrap_or("system"),
                    entry.message
                )
            };

            let line = format!("{}\n", content);
            let _guard = self.file_lock.lock().await;
            if self.config.rotation_enabled {
                let current = tokio::fs::metadata(file_path)
                    .await
                    .map(|m| m.len())
                    .unwrap_or(0);
                if current > 0 && current + line.len() as u64 > self.config.rotation_size {
                    rotate_files(file_path, self.config.max_files)
                        .await
                        .map_err(|e| {
                            SecurityError::Configuration(format!("Log rotation failed: {}", e))
                        })?;
                }
            }

            // tokio::fs::writeは毎回ファイル全体を上書きするため、ここでは
            // 追記モードで開く。そうしないと2件目以降のログ記録で直前の
            // エントリしかディスクに残らず、プロセス再起動後やログ出荷時に
            // 監査証跡がほぼ消えてしまう。
            let mut file = tokio::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(file_path)
                .await
                .map_err(|e| SecurityError::Configuration(format!("File open failed: {}", e)))?;
            file.write_all(line.as_bytes())
                .await
                .map_err(|e| SecurityError::Configuration(format!("File write failed: {}", e)))?;
        }
        Ok(())
    }
}

/// `path`を`path.1`に、`path.1`を`path.2`に…と送り、`path.<max_files>`を
/// 超える古いものを消す。`max_files`が0なら、`path`を消すだけ。
async fn rotate_files(path: &str, max_files: u32) -> std::io::Result<()> {
    let numbered = |n: u32| format!("{}.{}", path, n);
    if max_files == 0 {
        return tokio::fs::remove_file(path).await;
    }
    let _ = tokio::fs::remove_file(numbered(max_files)).await;
    for n in (1..max_files).rev() {
        if tokio::fs::metadata(numbered(n)).await.is_ok() {
            tokio::fs::rename(numbered(n), numbered(n + 1)).await?;
        }
    }
    tokio::fs::rename(path, numbered(1)).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_basic_logging() {
        let logger = AuditLogger::with_defaults();

        let entry = AuditLogEntry::new(
            AuditLevel::Info,
            AuditCategory::Authentication,
            "Test login".to_string(),
        )
        .with_user("user123".to_string());

        logger.log(entry).await.expect("Logging failed");

        let stats = logger.get_statistics().await;
        assert_eq!(stats.total_entries, 1);
        assert_eq!(stats.entries_by_level.get(&AuditLevel::Info), Some(&1));
    }

    #[tokio::test]
    async fn test_redact_user() {
        let logger = AuditLogger::with_defaults();

        let entry1 = AuditLogEntry::new(
            AuditLevel::Info,
            AuditCategory::Authentication,
            "User logged in".to_string(),
        )
        .with_user("user-1".to_string())
        .with_session("session-abc".to_string())
        .with_request_info("192.168.1.1".to_string(), "Mozilla/5.0".to_string())
        .add_metadata("key".to_string(), "value".to_string());

        let entry2 = AuditLogEntry::new(
            AuditLevel::Info,
            AuditCategory::Authentication,
            "User logged out".to_string(),
        )
        .with_user("user-1".to_string());

        let entry3 = AuditLogEntry::new(
            AuditLevel::Info,
            AuditCategory::Authentication,
            "Other user logged in".to_string(),
        )
        .with_user("user-2".to_string());

        logger.log(entry1).await.unwrap();
        logger.log(entry2).await.unwrap();
        logger.log(entry3).await.unwrap();

        let redacted_count = logger.redact_user("user-1").await;
        assert_eq!(redacted_count, 2);

        let all_entries = logger.get_all_entries().await;
        for entry in &all_entries {
            if entry.user_id.as_deref() == Some("user-1") {
                assert!(entry.message.contains("REDACTED"));
                assert!(entry.ip_address.is_none());
                assert!(entry.user_agent.is_none());
                assert!(entry.session_id.is_none());
                assert_eq!(
                    entry.metadata.get("gdpr_redacted").map(String::as_str),
                    Some("true")
                );
            } else {
                assert_eq!(entry.message, "Other user logged in");
            }
        }
    }

    #[tokio::test]
    async fn test_security_attack_logging() {
        let logger = AuditLogger::with_defaults();

        logger
            .log_security_attack(
                "XSS",
                "Script injection attempt",
                Some("192.168.1.100".to_string()),
                Some("Mozilla/5.0".to_string()),
            )
            .await
            .expect("Security logging failed");

        let entries = logger.get_all_entries().await;
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].level, AuditLevel::Critical);
        assert_eq!(entries[0].category, AuditCategory::SecurityAttack);
    }

    #[tokio::test]
    async fn test_filtering() {
        let logger = AuditLogger::with_defaults();

        // 複数のエントリを追加
        let entries = vec![
            AuditLogEntry::new(
                AuditLevel::Info,
                AuditCategory::Authentication,
                "Login".to_string(),
            ),
            AuditLogEntry::new(
                AuditLevel::Warning,
                AuditCategory::SecurityAttack,
                "Attack".to_string(),
            ),
            AuditLogEntry::new(
                AuditLevel::Error,
                AuditCategory::DataAccess,
                "Denied".to_string(),
            ),
        ];

        for entry in entries {
            logger.log(entry).await.expect("Logging failed");
        }

        // 警告レベル以上をフィルター
        let filter = AuditFilter {
            levels: Some(vec![
                AuditLevel::Warning,
                AuditLevel::Error,
                AuditLevel::Critical,
            ]),
            ..Default::default()
        };

        let filtered = logger.search(filter).await;
        assert_eq!(filtered.len(), 2);
    }

    #[tokio::test]
    async fn test_memory_limit() {
        let config = AuditConfig {
            max_memory_entries: 2,
            ..Default::default()
        };
        let logger = AuditLogger::new(config);

        // 3つのエントリを追加（制限は2つ）
        for i in 0..3 {
            let entry = AuditLogEntry::new(
                AuditLevel::Info,
                AuditCategory::Authentication,
                format!("Entry {}", i),
            );
            logger.log(entry).await.expect("Logging failed");
        }

        let entries = logger.get_all_entries().await;
        assert_eq!(entries.len(), 2); // 制限により2つのみ保持
        assert!(entries[0].message.contains("Entry 1")); // 最初のエントリは削除される
    }

    #[tokio::test]
    async fn test_file_output_appends_across_multiple_log_calls() {
        // write_to_fileが`tokio::fs::write`（毎回全体を上書き）を使って
        // いた場合、2件目のlog()呼び出しで1件目がディスクから消える。
        // 追記されていることを確認する。
        let path = format!("test_audit_append_{}.log", Uuid::new_v4());
        let config = AuditConfig {
            enable_file_output: true,
            log_file_path: Some(path.clone()),
            ..Default::default()
        };
        let logger = AuditLogger::new(config);

        logger
            .log(AuditLogEntry::new(
                AuditLevel::Info,
                AuditCategory::Authentication,
                "first".to_string(),
            ))
            .await
            .expect("Logging failed");
        logger
            .log(AuditLogEntry::new(
                AuditLevel::Info,
                AuditCategory::Authentication,
                "second".to_string(),
            ))
            .await
            .expect("Logging failed");

        let contents = tokio::fs::read_to_string(&path)
            .await
            .expect("Failed to read audit log file");
        let line_count = contents.lines().count();

        tokio::fs::remove_file(&path)
            .await
            .expect("Failed to clean up test audit log file");

        assert_eq!(
            line_count, 2,
            "expected both entries to persist, got: {}",
            contents
        );
    }

    #[tokio::test]
    async fn test_default_config_does_not_write_files() {
        // 既定ではファイルに書かない（作業フォルダに`audit.log`を作らない、#416）
        let logger = AuditLogger::with_defaults();
        assert_eq!(logger.file_path(), None);
        assert!(logger.check_file_output().await.is_ok());
    }

    #[tokio::test]
    async fn test_file_output_rotates_by_size() {
        let dir = std::env::temp_dir().join(format!("mcp-rs-audit-{}", Uuid::new_v4()));
        let path = dir.join("nested").join("audit.log");
        let path_str = path.to_string_lossy().into_owned();
        let logger = AuditLogger::new(AuditConfig {
            enable_file_output: true,
            log_file_path: Some(path_str.clone()),
            rotation_size: 400,
            max_files: 2,
            ..Default::default()
        });
        // フォルダが無ければ作る
        logger.check_file_output().await.unwrap();
        assert!(path.exists());

        for i in 0..20 {
            logger
                .log(AuditLogEntry::new(
                    AuditLevel::Info,
                    AuditCategory::DataAccess,
                    format!("entry {}", i),
                ))
                .await
                .unwrap();
        }

        let size = |p: &std::path::Path| std::fs::metadata(p).map(|m| m.len()).unwrap_or(0);
        assert!(size(&path) <= 400, "current file is {} bytes", size(&path));
        let first = dir.join("nested").join("audit.log.1");
        let second = dir.join("nested").join("audit.log.2");
        assert!(first.exists() && second.exists());
        // max_files（2）を超える古いファイルは残さない
        assert!(!dir.join("nested").join("audit.log.3").exists());
        // 最新のエントリは今のファイルにある
        let current = std::fs::read_to_string(&path).unwrap();
        assert!(current.contains("entry 19"), "{}", current);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
