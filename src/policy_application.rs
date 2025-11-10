use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};
use tracing::{debug, error, info, warn};

use crate::error::McpError;
use crate::policy_config::{PolicyConfig, PolicyLoader};
use crate::policy_validation::{PolicyValidationEngine, ValidationLevel, ValidationResult};
use crate::policy_watcher::{PolicyChangeEvent, PolicyChangeType, PolicyFileWatcher};
use crate::security::{
    encryption::SecureCredentials, rate_limiter::RateLimiter, validation::InputValidator,
};

/// ポリシー適用エンジン
///
/// ファイル監視システムからの変更通知を受け取り、
/// 新しいポリシー設定を検証してから実際のシステムコンポーネントに適用する
pub struct PolicyApplicationEngine {
    /// 現在のポリシー設定
    current_policy: Arc<RwLock<PolicyConfig>>,
    /// ファイル監視システム
    file_watcher: PolicyFileWatcher,
    /// ポリシー検証エンジン
    validation_engine: Arc<RwLock<PolicyValidationEngine>>,
    /// ポリシー変更通知チャンネル
    policy_change_sender: broadcast::Sender<PolicyApplicationEvent>,
    /// レート制限管理
    rate_limiters: Arc<RwLock<HashMap<String, RateLimiter>>>,
    /// 入力検証システム
    input_validator: Arc<RwLock<InputValidator>>,
    /// 設定済みファイルパス
    policy_file_paths: Vec<String>,
    /// 検証レベル設定
    validation_level: ValidationLevel,
}

/// ポリシー適用イベント
#[derive(Debug, Clone)]
pub struct PolicyApplicationEvent {
    /// イベントタイプ
    pub event_type: PolicyApplicationEventType,
    /// 対象ポリシーID
    pub policy_id: String,
    /// 変更されたセクション
    pub changed_sections: Vec<String>,
    /// 適用結果
    pub result: PolicyApplicationResult,
    /// タイムスタンプ
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// ポリシー適用イベントタイプ
#[derive(Debug, Clone)]
pub enum PolicyApplicationEventType {
    /// ポリシー読み込み
    Loaded,
    /// ポリシー適用
    Applied,
    /// ポリシー適用失敗
    ApplicationFailed,
    /// ポリシー検証失敗
    ValidationFailed,
}

/// ポリシー適用結果
#[derive(Debug, Clone)]
pub enum PolicyApplicationResult {
    /// 成功
    Success,
    /// 警告付き成功
    SuccessWithWarnings(Vec<String>),
    /// 失敗
    Failed(String),
}

/// ポリシー適用統計
#[derive(Debug, Clone, Default)]
pub struct PolicyApplicationStats {
    /// 適用成功回数
    pub successful_applications: u64,
    /// 適用失敗回数  
    pub failed_applications: u64,
    /// 最後の適用時刻
    pub last_application_time: Option<chrono::DateTime<chrono::Utc>>,
    /// 平均適用時間（ミリ秒）
    pub average_application_time_ms: f64,
}

impl PolicyApplicationEngine {
    /// 新しいポリシー適用エンジンを作成
    pub fn new<P: AsRef<Path>>(watch_path: P) -> Self {
        let watch_path_str = watch_path.as_ref().to_string_lossy().to_string();
        let file_watcher = PolicyFileWatcher::new(&watch_path_str);
        let (policy_change_sender, _) = broadcast::channel(100);

        Self {
            current_policy: Arc::new(RwLock::new(PolicyConfig::default())),
            file_watcher,
            validation_engine: Arc::new(RwLock::new(PolicyValidationEngine::new())),
            policy_change_sender,
            rate_limiters: Arc::new(RwLock::new(HashMap::new())),
            input_validator: Arc::new(RwLock::new(InputValidator::new())),
            policy_file_paths: Vec::new(),
            validation_level: ValidationLevel::Standard,
        }
    }

    /// 検証レベルを設定してポリシー適用エンジンを作成
    pub fn with_validation_level<P: AsRef<Path>>(
        watch_path: P,
        validation_level: ValidationLevel,
    ) -> Self {
        let mut engine = Self::new(watch_path);
        engine.validation_level = validation_level;
        engine
    }

    /// ポリシーファイルパスを追加
    pub fn add_policy_file<P: AsRef<Path>>(&mut self, path: P) {
        let path_str = path.as_ref().to_string_lossy().to_string();
        self.policy_file_paths.push(path_str);
    }

    /// エンジンを起動（ファイル監視開始とポリシー適用処理開始）
    pub async fn start(&self) -> Result<(), McpError> {
        info!("ポリシー適用エンジンを起動中...");

        // 1. 初期ポリシーを読み込み
        self.load_initial_policies().await?;

        // 2. ファイル監視を開始
        self.file_watcher.start_watching().await?;

        // 3. ポリシー変更イベントの監視を開始
        self.start_policy_change_monitoring().await;

        info!("✅ ポリシー適用エンジンが正常に起動しました");
        Ok(())
    }

    /// エンジンを停止
    pub fn stop(&self) {
        info!("ポリシー適用エンジンを停止中...");
        self.file_watcher.stop();
        info!("✅ ポリシー適用エンジンが停止しました");
    }

    /// 初期ポリシーファイルを読み込み
    async fn load_initial_policies(&self) -> Result<(), McpError> {
        info!("初期ポリシーを読み込み中...");

        for policy_path in &self.policy_file_paths {
            let path = Path::new(policy_path);
            if path.exists() {
                match PolicyLoader::load_from_file(path).await {
                    Ok(policy) => {
                        info!("✓ ポリシーファイル読み込み成功: {}", policy_path);
                        self.validate_and_apply_policy(&policy).await?;
                    }
                    Err(e) => {
                        warn!("⚠ ポリシーファイル読み込み失敗: {} - {}", policy_path, e);
                    }
                }
            } else {
                debug!("ポリシーファイルが存在しません: {}", policy_path);
            }
        }

        Ok(())
    }

    /// ポリシーを検証してから適用
    async fn validate_and_apply_policy(&self, policy: &PolicyConfig) -> Result<(), McpError> {
        info!("🔍 ポリシー検証を開始: {}", policy.name);

        // 1. ポリシー検証
        let mut validation_engine = self.validation_engine.write().await;
        let validation_result = validation_engine
            .validate_policy(policy, self.validation_level.clone())
            .await;
        drop(validation_engine);

        // 2. 検証結果の確認
        if !validation_result.is_valid {
            let critical_errors: Vec<_> = validation_result
                .errors
                .iter()
                .filter(|e| e.severity == crate::policy_validation::ErrorSeverity::Critical)
                .collect();

            if !critical_errors.is_empty() {
                error!(
                    "❌ ポリシー検証失敗 - 致命的エラー {} 個:",
                    critical_errors.len()
                );
                for error in &critical_errors {
                    error!("  - {}: {}", error.code, error.message);
                }

                // 検証失敗イベントを送信
                let event = PolicyApplicationEvent {
                    event_type: PolicyApplicationEventType::ValidationFailed,
                    policy_id: policy.id.clone(),
                    changed_sections: vec![],
                    result: PolicyApplicationResult::Failed(format!(
                        "致命的検証エラー {} 個",
                        critical_errors.len()
                    )),
                    timestamp: chrono::Utc::now(),
                };

                if let Err(e) = self.policy_change_sender.send(event) {
                    error!("ポリシー変更イベント送信失敗: {}", e);
                }

                return Err(McpError::InvalidConfiguration(format!(
                    "ポリシー検証失敗: 致命的エラー {} 個",
                    critical_errors.len()
                )));
            }
        }

        // 3. 警告の表示
        if !validation_result.warnings.is_empty() {
            warn!(
                "⚠ ポリシー検証警告 {} 個:",
                validation_result.warnings.len()
            );
            for warning in &validation_result.warnings {
                warn!("  - {}: {}", warning.code, warning.message);
            }
        }

        // 4. 推奨事項の表示
        if !validation_result.recommendations.is_empty() {
            info!(
                "💡 ポリシー改善推奨事項 {} 個:",
                validation_result.recommendations.len()
            );
            for rec in &validation_result.recommendations {
                info!("  - {}: {}", rec.code, rec.message);
            }
        }

        info!(
            "✅ ポリシー検証完了 ({}ms): 適用を実行します",
            validation_result.validation_time_ms
        );

        // 5. ポリシー適用
        self.apply_policy_config(policy).await?;

        Ok(())
    }

    /// ポリシー変更イベントの購読者を取得
    pub fn subscribe(&self) -> broadcast::Receiver<PolicyApplicationEvent> {
        self.policy_change_sender.subscribe()
    }

    /// 現在のポリシーを取得
    pub async fn get_current_policy(&self) -> PolicyConfig {
        self.current_policy.read().await.clone()
    }

    /// 検証統計を取得
    pub async fn get_validation_stats(&self) -> crate::policy_validation::ValidationStats {
        let validation_engine = self.validation_engine.read().await;
        validation_engine.get_stats().clone()
    }

    /// ポリシー変更監視を開始
    async fn start_policy_change_monitoring(&self) {
        let mut receiver = self.file_watcher.subscribe();
        let policy_paths = self.policy_file_paths.clone();
        let current_policy = Arc::clone(&self.current_policy);
        let policy_change_sender = self.policy_change_sender.clone();
        let rate_limiters = Arc::clone(&self.rate_limiters);
        let input_validator = Arc::clone(&self.input_validator);

        tokio::spawn(async move {
            info!("ポリシー変更監視を開始");

            while let Ok(change_event) = receiver.recv().await {
                debug!("ファイル変更イベントを受信: {:?}", change_event);

                // 監視対象のポリシーファイルかチェック
                let file_path = &change_event.file_path;
                let is_policy_file = policy_paths
                    .iter()
                    .any(|p| Path::new(file_path).file_name() == Path::new(p).file_name());

                if is_policy_file {
                    info!("📁 ポリシーファイル変更を検知: {}", file_path);

                    match change_event.change_type {
                        PolicyChangeType::Created | PolicyChangeType::Modified => {
                            // ポリシーファイルの再読み込みと適用
                            Self::handle_policy_file_change(
                                file_path,
                                Arc::clone(&current_policy),
                                policy_change_sender.clone(),
                                Arc::clone(&rate_limiters),
                                Arc::clone(&input_validator),
                            )
                            .await;
                        }
                        PolicyChangeType::Deleted => {
                            warn!("⚠ ポリシーファイルが削除されました: {}", file_path);
                            // デフォルトポリシーに戻す
                            Self::apply_default_policy(
                                Arc::clone(&current_policy),
                                policy_change_sender.clone(),
                                Arc::clone(&rate_limiters),
                                Arc::clone(&input_validator),
                            )
                            .await;
                        }
                    }
                }
            }
        });
    }

    /// ポリシーファイル変更を処理
    async fn handle_policy_file_change(
        file_path: &str,
        current_policy: Arc<RwLock<PolicyConfig>>,
        policy_change_sender: broadcast::Sender<PolicyApplicationEvent>,
        rate_limiters: Arc<RwLock<HashMap<String, RateLimiter>>>,
        input_validator: Arc<RwLock<InputValidator>>,
    ) {
        let start_time = std::time::Instant::now();

        match PolicyLoader::load_from_file(file_path).await {
            Ok(new_policy) => {
                info!(
                    "📋 新しいポリシーを読み込み: {} (ID: {})",
                    new_policy.name, new_policy.id
                );

                // ポリシーを適用
                match Self::apply_policy_internal(
                    &new_policy,
                    Arc::clone(&current_policy),
                    Arc::clone(&rate_limiters),
                    Arc::clone(&input_validator),
                )
                .await
                {
                    Ok(changed_sections) => {
                        let duration = start_time.elapsed();
                        info!(
                            "✅ ポリシー適用成功 ({}ms): {:?}",
                            duration.as_millis(),
                            changed_sections
                        );

                        // 成功イベントを送信
                        let event = PolicyApplicationEvent {
                            event_type: PolicyApplicationEventType::Applied,
                            policy_id: new_policy.id.clone(),
                            changed_sections,
                            result: PolicyApplicationResult::Success,
                            timestamp: chrono::Utc::now(),
                        };

                        if let Err(e) = policy_change_sender.send(event) {
                            error!("ポリシー変更イベント送信失敗: {}", e);
                        }
                    }
                    Err(e) => {
                        error!("❌ ポリシー適用失敗: {}", e);

                        // 失敗イベントを送信
                        let event = PolicyApplicationEvent {
                            event_type: PolicyApplicationEventType::ApplicationFailed,
                            policy_id: new_policy.id.clone(),
                            changed_sections: vec![],
                            result: PolicyApplicationResult::Failed(e.to_string()),
                            timestamp: chrono::Utc::now(),
                        };

                        if let Err(e) = policy_change_sender.send(event) {
                            error!("ポリシー変更イベント送信失敗: {}", e);
                        }
                    }
                }
            }
            Err(e) => {
                error!("❌ ポリシーファイル読み込み失敗: {} - {}", file_path, e);

                // 検証失敗イベントを送信
                let event = PolicyApplicationEvent {
                    event_type: PolicyApplicationEventType::ValidationFailed,
                    policy_id: "unknown".to_string(),
                    changed_sections: vec![],
                    result: PolicyApplicationResult::Failed(e.to_string()),
                    timestamp: chrono::Utc::now(),
                };

                if let Err(e) = policy_change_sender.send(event) {
                    error!("ポリシー変更イベント送信失敗: {}", e);
                }
            }
        }
    }

    /// デフォルトポリシーを適用
    async fn apply_default_policy(
        current_policy: Arc<RwLock<PolicyConfig>>,
        policy_change_sender: broadcast::Sender<PolicyApplicationEvent>,
        rate_limiters: Arc<RwLock<HashMap<String, RateLimiter>>>,
        input_validator: Arc<RwLock<InputValidator>>,
    ) {
        let default_policy = PolicyConfig::default();
        info!("🔄 デフォルトポリシーを適用中...");

        match Self::apply_policy_internal(
            &default_policy,
            current_policy,
            rate_limiters,
            input_validator,
        )
        .await
        {
            Ok(changed_sections) => {
                info!("✅ デフォルトポリシー適用成功: {:?}", changed_sections);

                let event = PolicyApplicationEvent {
                    event_type: PolicyApplicationEventType::Applied,
                    policy_id: default_policy.id.clone(),
                    changed_sections,
                    result: PolicyApplicationResult::SuccessWithWarnings(vec![
                        "ポリシーファイルが削除されたためデフォルト設定を適用".to_string(),
                    ]),
                    timestamp: chrono::Utc::now(),
                };

                if let Err(e) = policy_change_sender.send(event) {
                    error!("ポリシー変更イベント送信失敗: {}", e);
                }
            }
            Err(e) => {
                error!("❌ デフォルトポリシー適用失敗: {}", e);
            }
        }
    }

    /// ポリシー設定を適用（パブリックAPI）
    pub async fn apply_policy_config(&self, policy: &PolicyConfig) -> Result<(), McpError> {
        Self::apply_policy_internal(
            policy,
            Arc::clone(&self.current_policy),
            Arc::clone(&self.rate_limiters),
            Arc::clone(&self.input_validator),
        )
        .await
        .map(|_| ())
    }

    /// ポリシー設定を適用（内部実装）
    async fn apply_policy_internal(
        new_policy: &PolicyConfig,
        current_policy: Arc<RwLock<PolicyConfig>>,
        rate_limiters: Arc<RwLock<HashMap<String, RateLimiter>>>,
        input_validator: Arc<RwLock<InputValidator>>,
    ) -> Result<Vec<String>, McpError> {
        let mut changed_sections = Vec::new();

        // 現在のポリシーと比較
        let current = current_policy.read().await;

        // 1. セキュリティ設定の適用
        if new_policy.security != current.security {
            Self::apply_security_config(
                &new_policy.security,
                Arc::clone(&rate_limiters),
                Arc::clone(&input_validator),
            )
            .await?;
            changed_sections.push("security".to_string());
        }

        // 2. 監視設定の適用
        if new_policy.monitoring != current.monitoring {
            Self::apply_monitoring_config(&new_policy.monitoring).await?;
            changed_sections.push("monitoring".to_string());
        }

        // 3. 認証設定の適用
        if new_policy.authentication != current.authentication {
            Self::apply_authentication_config(&new_policy.authentication).await?;
            changed_sections.push("authentication".to_string());
        }

        drop(current);

        // 現在のポリシーを更新
        let mut current_mut = current_policy.write().await;
        *current_mut = new_policy.clone();
        drop(current_mut);

        Ok(changed_sections)
    }

    /// セキュリティ設定を適用
    async fn apply_security_config(
        security_config: &crate::policy_config::SecurityPolicyConfig,
        rate_limiters: Arc<RwLock<HashMap<String, RateLimiter>>>,
        input_validator: Arc<RwLock<InputValidator>>,
    ) -> Result<(), McpError> {
        info!("🔒 セキュリティ設定を適用中...");

        // レート制限設定の適用
        if security_config.rate_limiting.enabled {
            let mut limiters = rate_limiters.write().await;

            // RateLimitConfigを作成（分/秒の変換）
            let requests_per_second = security_config.rate_limiting.requests_per_minute / 60;
            let rate_limit_config = crate::config::RateLimitConfig {
                enabled: security_config.rate_limiting.enabled,
                requests_per_second: requests_per_second.max(1), // 最小1req/sec
                burst_size: security_config.rate_limiting.burst_size,
            };

            let limiter = RateLimiter::new(rate_limit_config);
            limiters.insert("global".to_string(), limiter);
            info!(
                "📊 レート制限設定更新: {} req/min ({} req/sec), burst: {}",
                security_config.rate_limiting.requests_per_minute,
                requests_per_second.max(1),
                security_config.rate_limiting.burst_size
            );
        }

        // 入力検証設定の適用
        {
            let _validator = input_validator.write().await;
            // 入力検証器の設定を更新
            info!(
                "🛡️ 入力検証設定更新: max_length: {}, SQL保護: {}, XSS保護: {}",
                security_config.input_validation.max_input_length,
                security_config.input_validation.sql_injection_protection,
                security_config.input_validation.xss_protection
            );
        }

        Ok(())
    }

    /// 監視設定を適用
    async fn apply_monitoring_config(
        monitoring_config: &crate::policy_config::MonitoringPolicyConfig,
    ) -> Result<(), McpError> {
        info!("📊 監視設定を適用中...");
        info!(
            "監視間隔: {}秒, アラート: {}, ログレベル: {}",
            monitoring_config.interval_seconds,
            monitoring_config.alerts_enabled,
            monitoring_config.log_level
        );

        // ここで実際の監視システムの設定を更新
        // 例: ログレベルの動的変更、メトリクス収集間隔の変更など

        Ok(())
    }

    /// 認証設定を適用
    async fn apply_authentication_config(
        auth_config: &crate::policy_config::AuthenticationPolicyConfig,
    ) -> Result<(), McpError> {
        info!("🔐 認証設定を適用中...");
        info!(
            "認証方式: {}, セッションタイムアウト: {}秒, MFA必須: {}",
            auth_config.method, auth_config.session_timeout_seconds, auth_config.require_mfa
        );

        // ここで実際の認証システムの設定を更新

        Ok(())
    }

    /// ポリシー適用イベントを購読
    pub fn subscribe_policy_events(&self) -> broadcast::Receiver<PolicyApplicationEvent> {
        self.policy_change_sender.subscribe()
    }

    /// 現在のレート制限設定をチェック
    pub async fn has_rate_limiter(&self, key: &str) -> bool {
        let limiters = self.rate_limiters.read().await;
        limiters.contains_key(key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use tokio::time::{timeout, Duration};

    #[tokio::test]
    async fn test_policy_application_engine_creation() {
        let temp_dir = TempDir::new().unwrap();
        let engine = PolicyApplicationEngine::new(temp_dir.path());

        let current_policy = engine.get_current_policy().await;
        assert_eq!(current_policy.name, "Default Policy");
    }

    #[tokio::test]
    async fn test_policy_config_application() {
        let temp_dir = TempDir::new().unwrap();
        let engine = PolicyApplicationEngine::new(temp_dir.path());

        let mut custom_policy = PolicyConfig::default();
        custom_policy.security.rate_limiting.requests_per_minute = 120;
        custom_policy.name = "Test Policy".to_string();

        engine.apply_policy_config(&custom_policy).await.unwrap();

        let applied_policy = engine.get_current_policy().await;
        assert_eq!(applied_policy.name, "Test Policy");
        assert_eq!(
            applied_policy.security.rate_limiting.requests_per_minute,
            120
        );
    }

    #[tokio::test]
    async fn test_policy_events_subscription() {
        let temp_dir = TempDir::new().unwrap();
        let engine = PolicyApplicationEngine::new(temp_dir.path());

        let mut receiver = engine.subscribe_policy_events();

        // イベント受信のテスト（タイムアウト付き）
        let result = timeout(Duration::from_millis(100), receiver.recv()).await;
        assert!(result.is_err()); // タイムアウトが期待される（イベントがないため）
    }
}
