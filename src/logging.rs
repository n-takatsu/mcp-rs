use anyhow::Result;
use std::fs;
use std::path::{Path, PathBuf};
use tracing_appender::{non_blocking, rolling};
use tracing_subscriber::fmt::writer::MakeWriterExt;
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter, Layer};

/// ログ設定
#[derive(Debug, Clone)]
pub struct LogConfig {
    /// ログレベル (trace, debug, info, warn, error)
    pub level: String,
    /// ログディレクトリ
    pub log_dir: PathBuf,
    /// ファイルローテーション設定
    pub rotation: LogRotation,
    /// ログ保持ポリシー
    pub retention: LogRetention,
    /// コンソール出力有効
    pub console_enabled: bool,
    /// ファイル出力有効  
    pub file_enabled: bool,
    /// モジュール別ログ分離設定
    pub module_separation: ModuleSeparation,
}

#[derive(Debug, Clone)]
pub enum LogRotation {
    /// 日次ローテーション
    Daily,
    /// 時間毎ローテーション
    Hourly,
    /// ローテーションなし
    Never,
}

#[derive(Debug, Clone)]
pub enum LogRetention {
    /// アプリケーションは削除しない（OS/ログ管理ツール任せ - 推奨）
    External,
    /// 指定日数後に自動削除（開発・テスト環境用）
    Days(u32),
    /// 最大ファイル数を保持（簡易環境用）
    Count(u32),
    /// 最大ディスク使用量を制限（リソース制約環境用）
    Size(u64), // bytes
}

#[derive(Debug, Clone)]
pub enum ModuleSeparation {
    /// 単一ファイル（mcp-rs.log）- 開発・小規模環境用
    Single,
    /// モジュール別ファイル分離（本番推奨）
    /// - mcp-core.log: MCP サーバー基本動作
    /// - wordpress.log: WordPress ハンドラー
    /// - database.log: データベース関連
    /// - transport.log: HTTP/WebSocket 通信
    /// - security.log: セキュリティ監査
    Separated,
    /// ハイブリッド（概要＋詳細分離）
    /// - mcp-summary.log: エラー・警告の概要
    /// - モジュール別ログ: 詳細情報
    Hybrid,
}

impl Default for LogConfig {
    fn default() -> Self {
        Self {
            level: "info".to_string(),
            log_dir: get_default_log_dir(),
            rotation: LogRotation::Daily,
            retention: LogRetention::External, // 業界標準：外部ツール任せ
            console_enabled: true,
            file_enabled: true,
            module_separation: ModuleSeparation::Separated, // 本番推奨：モジュール別
        }
    }
}

impl LogConfig {
    /// 設定からログ設定を作成
    pub fn from_server_config(server_config: &crate::config::ServerConfig) -> Self {
        let mut config = Self::default();

        if let Some(ref level) = server_config.log_level {
            config.level = level.clone();
        }

        // ログ保持ポリシーを設定
        if let Some(ref retention_config) = server_config.log_retention {
            config.retention = parse_retention_config(retention_config);
        }

        // ログモジュール分離を設定
        if let Some(ref module_config) = server_config.log_module {
            config.module_separation = parse_module_separation_config(module_config);
        }

        config
    }

    /// カスタムログディレクトリを設定
    pub fn with_log_dir<P: Into<PathBuf>>(mut self, dir: P) -> Self {
        self.log_dir = dir.into();
        self
    }

    /// ローテーション設定
    pub fn with_rotation(mut self, rotation: LogRotation) -> Self {
        self.rotation = rotation;
        self
    }

    /// 保持ポリシー設定
    pub fn with_retention(mut self, retention: LogRetention) -> Self {
        self.retention = retention;
        self
    }

    /// コンソール出力制御
    pub fn with_console(mut self, enabled: bool) -> Self {
        self.console_enabled = enabled;
        self
    }

    /// ファイル出力制御  
    pub fn with_file(mut self, enabled: bool) -> Self {
        self.file_enabled = enabled;
        self
    }

    /// モジュール分離設定
    pub fn with_module_separation(mut self, separation: ModuleSeparation) -> Self {
        self.module_separation = separation;
        self
    }
}

/// デフォルトログディレクトリを取得
/// 優先順位：
/// 1. 実行ファイルと同じディレクトリの logs フォルダ
/// 2. カレントディレクトリの logs フォルダ
/// 3. システムテンプディレクトリの mcp-rs フォルダ
fn get_default_log_dir() -> PathBuf {
    // 実行ファイルのディレクトリを取得
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            let log_dir = exe_dir.join("logs");
            if ensure_log_dir(&log_dir).is_ok() {
                return log_dir;
            }
        }
    }

    // カレントディレクトリのlogsフォルダ
    let current_log_dir = PathBuf::from("logs");
    if ensure_log_dir(&current_log_dir).is_ok() {
        return current_log_dir;
    }

    // システムテンプディレクトリ
    let temp_log_dir = std::env::temp_dir().join("mcp-rs").join("logs");
    if ensure_log_dir(&temp_log_dir).is_ok() {
        return temp_log_dir;
    }

    // フォールバック：カレントディレクトリ
    PathBuf::from(".")
}

/// ログディレクトリを確保
fn ensure_log_dir(dir: &Path) -> Result<()> {
    if !dir.exists() {
        fs::create_dir_all(dir)?;
    }
    Ok(())
}

/// モジュール識別子
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Module {
    /// MCPサーバーコア（接続、基本処理）
    Core,
    /// WordPressハンドラー
    WordPress,
    /// データベース関連
    Database,
    /// Transport層（HTTP/WebSocket通信）
    Transport,
    /// セキュリティ・監査
    Security,
    /// プラグインシステム
    Plugin,
    /// 概要ログ（Hybridモード用）
    Summary,
}

impl Module {
    /// モジュールのログファイル名を取得
    pub fn log_filename(&self) -> &'static str {
        match self {
            Module::Core => "mcp-core.log",
            Module::WordPress => "wordpress.log",
            Module::Database => "database.log",
            Module::Transport => "transport.log",
            Module::Security => "security.log",
            Module::Plugin => "plugin.log",
            Module::Summary => "mcp-summary.log",
        }
    }

    /// モジュール名を取得
    pub fn name(&self) -> &'static str {
        match self {
            Module::Core => "mcp-core",
            Module::WordPress => "wordpress",
            Module::Database => "database",
            Module::Transport => "transport",
            Module::Security => "security",
            Module::Plugin => "plugin",
            Module::Summary => "summary",
        }
    }
}

/// ログシステムを初期化
pub fn init_logging(config: &LogConfig) -> Result<()> {
    // ログディレクトリを確保
    ensure_log_dir(&config.log_dir)?;

    // EnvFilterを作成
    let env_filter = EnvFilter::try_new(&config.level).unwrap_or_else(|_| EnvFilter::new("info"));

    // ログ設定に基づいて初期化
    match (
        &config.console_enabled,
        &config.file_enabled,
        &config.module_separation,
    ) {
        (true, true, ModuleSeparation::Single) => {
            init_single_file_logging(config, env_filter)?;
        }
        (true, true, ModuleSeparation::Separated) => {
            init_separated_logging(config, env_filter)?;
        }
        (true, true, ModuleSeparation::Hybrid) => {
            init_hybrid_full_logging(config, env_filter)?;
        }
        (true, false, _) => {
            // コンソールのみ
            init_console_only_logging(env_filter)?;
        }
        (false, true, separation) => {
            // ファイルのみ
            init_file_only_logging(config, env_filter, separation)?;
        }
        (false, false, _) => {
            // 最低限のコンソール出力
            tracing_subscriber::fmt()
                .with_max_level(tracing::Level::WARN)
                .init();
        }
    }

    tracing::info!("📝 ログシステム初期化完了");
    tracing::info!("📂 ログディレクトリ: {}", config.log_dir.display());
    tracing::info!("📊 ログレベル: {}", config.level);
    tracing::info!("💻 コンソール出力: {}", config.console_enabled);
    tracing::info!("📄 ファイル出力: {}", config.file_enabled);
    tracing::info!(
        "🗂️  ログ保持ポリシー: {}",
        format_retention_policy(&config.retention)
    );
    tracing::info!(
        "🏗️  モジュール分離: {}",
        format_module_separation(&config.module_separation)
    );

    // ログ削除ポリシーを適用（External以外の場合）
    if let Err(e) = apply_retention_policy(config) {
        tracing::warn!("ログ保持ポリシー適用に失敗: {}", e);
    }

    Ok(())
}

/// ログ統計情報を取得
pub fn get_log_stats(log_dir: &Path) -> Result<LogStats> {
    let mut stats = LogStats::default();

    if !log_dir.exists() {
        return Ok(stats);
    }

    for entry in fs::read_dir(log_dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_file() {
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if name.starts_with("mcp-rs") && name.ends_with(".log") {
                    if let Ok(metadata) = entry.metadata() {
                        stats.file_count += 1;
                        stats.total_size += metadata.len();

                        if let Ok(modified) = metadata.modified() {
                            if stats.last_modified.is_none()
                                || stats
                                    .last_modified
                                    .as_ref()
                                    .map(|t| modified > *t)
                                    .unwrap_or(false)
                            {
                                stats.last_modified = Some(modified);
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(stats)
}

#[derive(Debug, Default)]
pub struct LogStats {
    pub file_count: usize,
    pub total_size: u64,
    pub last_modified: Option<std::time::SystemTime>,
}

impl LogStats {
    pub fn format_size(&self) -> String {
        const UNITS: &[&str] = &["B", "KB", "MB", "GB"];
        let mut size = self.total_size as f64;
        let mut unit_index = 0;

        while size >= 1024.0 && unit_index < UNITS.len() - 1 {
            size /= 1024.0;
            unit_index += 1;
        }

        format!("{:.2} {}", size, UNITS[unit_index])
    }
}

/// 保持ポリシーの説明文を生成
fn format_retention_policy(retention: &LogRetention) -> String {
    match retention {
        LogRetention::External => "外部管理（推奨）".to_string(),
        LogRetention::Days(days) => format!("{}日後自動削除", days),
        LogRetention::Count(count) => format!("最大{}ファイル保持", count),
        LogRetention::Size(bytes) => {
            let mb = *bytes as f64 / (1024.0 * 1024.0);
            format!("最大{:.1}MB保持", mb)
        }
    }
}

/// ログ保持ポリシーを適用
fn apply_retention_policy(config: &LogConfig) -> Result<()> {
    match &config.retention {
        LogRetention::External => {
            // 外部管理なので何もしない（推奨アプローチ）
            Ok(())
        }
        LogRetention::Days(days) => cleanup_old_logs_by_age(&config.log_dir, *days),
        LogRetention::Count(max_count) => cleanup_old_logs_by_count(&config.log_dir, *max_count),
        LogRetention::Size(max_bytes) => cleanup_old_logs_by_size(&config.log_dir, *max_bytes),
    }
}

/// 日数ベースでログファイルを削除
fn cleanup_old_logs_by_age(log_dir: &Path, max_days: u32) -> Result<()> {
    use std::time::{Duration, SystemTime};

    let cutoff_time = SystemTime::now() - Duration::from_secs(max_days as u64 * 24 * 60 * 60);
    let mut removed_count = 0;

    for entry in fs::read_dir(log_dir)? {
        let entry = entry?;
        let path = entry.path();

        if is_log_file(&path) {
            if let Ok(metadata) = entry.metadata() {
                if let Ok(modified) = metadata.modified() {
                    if modified < cutoff_time {
                        if let Err(e) = fs::remove_file(&path) {
                            tracing::warn!("ログファイル削除失敗: {} - {}", path.display(), e);
                        } else {
                            removed_count += 1;
                            tracing::debug!("古いログファイル削除: {}", path.display());
                        }
                    }
                }
            }
        }
    }

    if removed_count > 0 {
        tracing::info!(
            "🗑️  古いログファイル{}個削除（{}日より古い）",
            removed_count,
            max_days
        );
    }

    Ok(())
}

/// ファイル数ベースでログファイルを削除
fn cleanup_old_logs_by_count(log_dir: &Path, max_count: u32) -> Result<()> {
    let mut log_files = Vec::new();

    for entry in fs::read_dir(log_dir)? {
        let entry = entry?;
        let path = entry.path();

        if is_log_file(&path) {
            if let Ok(metadata) = entry.metadata() {
                if let Ok(modified) = metadata.modified() {
                    log_files.push((path, modified));
                }
            }
        }
    }

    // 更新日時でソート（新しい順）
    log_files.sort_by(|a, b| b.1.cmp(&a.1));

    let mut removed_count = 0;

    // 最大数を超えた古いファイルを削除
    for (path, _) in log_files.iter().skip(max_count as usize) {
        if let Err(e) = fs::remove_file(path) {
            tracing::warn!("ログファイル削除失敗: {} - {}", path.display(), e);
        } else {
            removed_count += 1;
            tracing::debug!("古いログファイル削除: {}", path.display());
        }
    }

    if removed_count > 0 {
        tracing::info!(
            "🗑️  古いログファイル{}個削除（最大{}個保持）",
            removed_count,
            max_count
        );
    }

    Ok(())
}

/// サイズベースでログファイルを削除
fn cleanup_old_logs_by_size(log_dir: &Path, max_bytes: u64) -> Result<()> {
    let mut log_files = Vec::new();
    let mut total_size = 0u64;

    for entry in fs::read_dir(log_dir)? {
        let entry = entry?;
        let path = entry.path();

        if is_log_file(&path) {
            if let Ok(metadata) = entry.metadata() {
                if let Ok(modified) = metadata.modified() {
                    let size = metadata.len();
                    log_files.push((path, modified, size));
                    total_size += size;
                }
            }
        }
    }

    if total_size <= max_bytes {
        return Ok(()); // サイズ制限内
    }

    // 更新日時でソート（新しい順）
    log_files.sort_by(|a, b| b.1.cmp(&a.1));

    let mut current_size = 0u64;
    let mut removed_count = 0;

    for (path, _, size) in log_files.iter() {
        if current_size + size <= max_bytes {
            current_size += size;
        } else {
            // サイズ制限を超えるファイルを削除
            if let Err(e) = fs::remove_file(path) {
                tracing::warn!("ログファイル削除失敗: {} - {}", path.display(), e);
            } else {
                removed_count += 1;
                tracing::debug!("古いログファイル削除: {}", path.display());
            }
        }
    }

    if removed_count > 0 {
        let mb_limit = max_bytes as f64 / (1024.0 * 1024.0);
        tracing::info!(
            "🗑️  古いログファイル{}個削除（{:.1}MB制限）",
            removed_count,
            mb_limit
        );
    }

    Ok(())
}

/// ログファイルかどうかを判定
fn is_log_file(path: &Path) -> bool {
    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
        name.starts_with("mcp-rs") && name.contains(".log")
    } else {
        false
    }
}

/// 単一ファイルログ初期化（コンソール＋ファイル）
fn init_single_file_logging(config: &LogConfig, env_filter: EnvFilter) -> Result<()> {
    let file_appender = match config.rotation {
        LogRotation::Daily => rolling::daily(&config.log_dir, "mcp-rs.log"),
        LogRotation::Hourly => rolling::hourly(&config.log_dir, "mcp-rs.log"),
        LogRotation::Never => rolling::never(&config.log_dir, "mcp-rs.log"),
    };

    let (non_blocking, _guard) = non_blocking(file_appender);

    tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .with_writer(std::io::stderr.and(non_blocking))
        .with_target(true)
        .with_thread_ids(true)
        .with_thread_names(true)
        .with_file(true)
        .with_line_number(true)
        .init();

    Ok(())
}

/// 分離ログ初期化（本番推奨）
fn init_separated_logging(config: &LogConfig, env_filter: EnvFilter) -> Result<()> {
    // 暫定実装：コンソール＋単一ファイル
    // TODO: 真のモジュール別分離実装
    tracing::warn!("モジュール別分離ログは開発中です。暫定的に単一ファイルを使用します。");
    init_single_file_logging(config, env_filter)
}

/// ハイブリッドログ初期化（概要＋詳細分離）
fn init_hybrid_full_logging(config: &LogConfig, env_filter: EnvFilter) -> Result<()> {
    // 暫定実装：コンソール＋単一ファイル
    // TODO: 概要＋詳細分離実装
    tracing::warn!("ハイブリッドログは開発中です。暫定的に単一ファイルを使用します。");
    init_single_file_logging(config, env_filter)
}

/// コンソールのみログ初期化
fn init_console_only_logging(env_filter: EnvFilter) -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .with_target(true)
        .with_thread_ids(true)
        .with_thread_names(true)
        .with_file(true)
        .with_line_number(true)
        .init();

    Ok(())
}

/// ファイルのみログ初期化
fn init_file_only_logging(
    config: &LogConfig,
    env_filter: EnvFilter,
    separation: &ModuleSeparation,
) -> Result<()> {
    match separation {
        ModuleSeparation::Single => {
            let file_appender = match config.rotation {
                LogRotation::Daily => rolling::daily(&config.log_dir, "mcp-rs.log"),
                LogRotation::Hourly => rolling::hourly(&config.log_dir, "mcp-rs.log"),
                LogRotation::Never => rolling::never(&config.log_dir, "mcp-rs.log"),
            };

            let (non_blocking, _guard) = non_blocking(file_appender);

            tracing_subscriber::fmt()
                .with_env_filter(env_filter)
                .with_writer(non_blocking)
                .with_ansi(false)
                .with_target(true)
                .with_thread_ids(true)
                .with_thread_names(true)
                .with_file(true)
                .with_line_number(true)
                .init();
        }
        _ => {
            tracing::warn!(
                "分離ログはファイルのみモードでは未実装です。単一ファイルを使用します。"
            );
            init_file_only_logging(config, env_filter, &ModuleSeparation::Single)?;
        }
    }

    Ok(())
}

/// モジュール分離設定の説明文を生成
fn format_module_separation(separation: &ModuleSeparation) -> String {
    match separation {
        ModuleSeparation::Single => "単一ファイル（mcp-rs.log）".to_string(),
        ModuleSeparation::Separated => {
            "モジュール別分離（core, wordpress, database等）".to_string()
        }
        ModuleSeparation::Hybrid => {
            "ハイブリッド（概要mcp-summary.log＋モジュール別詳細）".to_string()
        }
    }
}

/// 設定からログ保持ポリシーを解析
fn parse_retention_config(config: &crate::config::LogRetentionConfig) -> LogRetention {
    match config.policy.as_deref() {
        Some("days") => LogRetention::Days(config.days.unwrap_or(30)),
        Some("count") => LogRetention::Count(config.count.unwrap_or(10)),
        Some("size") => {
            let size_mb = config.size_mb.unwrap_or(100);
            LogRetention::Size(size_mb as u64 * 1024 * 1024)
        }
        _ => LogRetention::External, // デフォルトは外部管理
    }
}

/// 設定からモジュール分離ポリシーを解析
fn parse_module_separation_config(config: &crate::config::LogModuleConfig) -> ModuleSeparation {
    match config.separation.as_deref() {
        Some("single") => ModuleSeparation::Single,
        Some("separated") => ModuleSeparation::Separated,
        Some("hybrid") => ModuleSeparation::Hybrid,
        _ => ModuleSeparation::Separated, // デフォルトは分離
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_log_config_default() {
        let config = LogConfig::default();
        assert_eq!(config.level, "info");
        assert_eq!(config.console_enabled, true);
        assert_eq!(config.file_enabled, true);
    }

    #[test]
    fn test_log_config_from_server_config() {
        let server_config = crate::config::ServerConfig {
            bind_addr: None,
            stdio: None,
            log_level: Some("debug".to_string()),
            log_retention: None,
            log_module: None,
        };

        let log_config = LogConfig::from_server_config(&server_config);
        assert_eq!(log_config.level, "debug");
    }

    #[test]
    fn test_ensure_log_dir() {
        let temp_dir = tempdir().unwrap();
        let log_dir = temp_dir.path().join("test_logs");

        assert!(ensure_log_dir(&log_dir).is_ok());
        assert!(log_dir.exists());
    }

    #[test]
    fn test_log_stats_format_size() {
        let mut stats = LogStats::default();

        stats.total_size = 1024;
        assert_eq!(stats.format_size(), "1.00 KB");

        stats.total_size = 1024 * 1024;
        assert_eq!(stats.format_size(), "1.00 MB");

        stats.total_size = 1536; // 1.5 KB
        assert_eq!(stats.format_size(), "1.50 KB");
    }
}
