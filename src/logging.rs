use anyhow::Result;
use std::fmt::Write as _;
use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use tracing::{Event, Subscriber};
use tracing_appender::non_blocking::WorkerGuard;
use tracing_appender::{non_blocking, rolling};
use tracing_subscriber::fmt::format::Writer;
use tracing_subscriber::fmt::time::{ChronoLocal, FormatTime};
use tracing_subscriber::fmt::{FmtContext, FormatEvent, FormatFields, FormattedFields};
use tracing_subscriber::registry::LookupSpan;
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter, Layer};

/// ログのタイムスタンプをローカル時刻・ミリ秒3桁・タイムゾーン付きで
/// 出力するフォーマッター（例: `2026-10-10T16:43:13.227+09:00`）。
/// デフォルトのUTC・マイクロ秒表記より人間が読みやすいため使う。
fn local_time_timer() -> ChronoLocal {
    ChronoLocal::new("%Y-%m-%dT%H:%M:%S%.3f%:z".to_string())
}

/// 各ログ行に`[pid N]`を必ず含めるカスタムフォーマッター。
///
/// Claude Desktopが同じ設定のmcp-rsを複数同時に起動することがあり
/// （実例: 同じログファイルに複数PIDの出力が混在していた）、どの行が
/// どのプロセスのものか区別できる必要がある。`tracing_subscriber`の
/// 既定フォーマッターにはプロセスID表示の仕組みがないため、
/// `FormatEvent`を自前実装する。表示項目・順序は既定の`Format<Full>`に
/// 揃え、レベルの直後に`[pid N]`を挿入する。
struct PidFormatter {
    timer: ChronoLocal,
}

impl<S, N> FormatEvent<S, N> for PidFormatter
where
    S: Subscriber + for<'a> LookupSpan<'a>,
    N: for<'a> FormatFields<'a> + 'static,
{
    fn format_event(
        &self,
        ctx: &FmtContext<'_, S, N>,
        mut writer: Writer<'_>,
        event: &Event<'_>,
    ) -> std::fmt::Result {
        let meta = event.metadata();

        self.timer.format_time(&mut writer)?;
        write!(writer, "  ")?;

        let level_str = match *meta.level() {
            tracing::Level::TRACE => "TRACE",
            tracing::Level::DEBUG => "DEBUG",
            tracing::Level::INFO => " INFO",
            tracing::Level::WARN => " WARN",
            tracing::Level::ERROR => "ERROR",
        };
        if writer.has_ansi_escapes() {
            let color = match *meta.level() {
                tracing::Level::TRACE => "35",
                tracing::Level::DEBUG => "34",
                tracing::Level::INFO => "32",
                tracing::Level::WARN => "33",
                tracing::Level::ERROR => "31",
            };
            write!(writer, "\x1b[{}m{}\x1b[0m ", color, level_str)?;
        } else {
            write!(writer, "{} ", level_str)?;
        }

        write!(writer, "[pid {}] ", std::process::id())?;

        let current_thread = std::thread::current();
        match current_thread.name() {
            Some(name) => write!(writer, "{} ", name)?,
            None => write!(writer, "{:0>2?} ", current_thread.id())?,
        }

        if let Some(scope) = ctx.event_scope() {
            let mut seen = false;
            for span in scope.from_root() {
                write!(writer, "{}", span.metadata().name())?;
                seen = true;

                let ext = span.extensions();
                if let Some(fields) = ext.get::<FormattedFields<N>>() {
                    if !fields.is_empty() {
                        write!(writer, "{{{}}}", fields)?;
                    }
                }
                write!(writer, ":")?;
            }
            if seen {
                writer.write_char(' ')?;
            }
        }

        write!(writer, "{}: ", meta.target())?;

        match (meta.file(), meta.line()) {
            (Some(file), Some(line)) => write!(writer, "{}:{}: ", file, line)?,
            (Some(file), None) => write!(writer, "{}: ", file)?,
            _ => {}
        }

        ctx.format_fields(writer.by_ref(), event)?;
        writeln!(writer)
    }
}

/// `[pid N]`入りのカスタムフォーマッターを、ローカル時刻タイマー付きで
/// 構築する。
fn pid_formatter() -> PidFormatter {
    PidFormatter {
        timer: local_time_timer(),
    }
}

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
            log_dir: primary_log_dir_candidate(),
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

/// ログディレクトリの第一候補を返す（書き込み可否は未検証）。
/// 実際の書き込み試行、および失敗時のフォールバックは
/// `resolve_log_destination`が`init_logging`実行時に行う。
///
/// 優先順位：
/// 1. 実行ファイルと同じディレクトリの logs フォルダ
/// 2. （実行ファイルパスが取得できない場合）カレントディレクトリの logs フォルダ
fn primary_log_dir_candidate() -> PathBuf {
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            return exe_dir.join("logs");
        }
    }

    PathBuf::from("logs")
}

/// 第一候補（`LogConfig::log_dir`）が書き込めなかった場合に試す、
/// ユーザー権限で書き込み可能な可能性が高い候補ディレクトリ（優先順）。
/// `C:\Program Files\...`のように実行ファイルが昇格権限の場所に
/// インストールされ、非昇格プロセス（例: Claude Desktop経由のMCP
/// サーバー起動）からは書き込めないケースを主な対象とする。
///
/// Windowsでは`%LOCALAPPDATA%`のような既知フォルダーは**使わない**。
/// Claude Desktop（MSIXパッケージ）から起動された子プロセスの場合、
/// `AppData`配下への実ファイルI/Oは`AppData\Local\Packages\<パッケージ>\
/// LocalCache\...`へ透過的にリダイレクトされることがあり（パス仮想化）、
/// プロセス自身の`create_dir_all`/`OpenOptions::open`/`fs::metadata`は
/// すべて成功したように見えても、表示したパスには実ファイルが存在しない
/// という事態が起きる（同一プロセス内の自己検証では検出できない）。
/// `%USERPROFILE%`直下はこの既知フォルダー仮想化の対象外のため、こちらを
/// 使う。
fn fallback_log_dir_candidates() -> Vec<PathBuf> {
    let mut candidates = Vec::new();

    #[cfg(windows)]
    if let Some(user_profile) = std::env::var_os("USERPROFILE") {
        candidates.push(PathBuf::from(user_profile).join(".mcp-rs").join("logs"));
    }

    #[cfg(not(windows))]
    {
        if let Some(xdg_state) = std::env::var_os("XDG_STATE_HOME") {
            candidates.push(PathBuf::from(xdg_state).join("mcp-rs").join("logs"));
        } else if let Some(home) = std::env::var_os("HOME") {
            candidates.push(
                PathBuf::from(home)
                    .join(".local")
                    .join("state")
                    .join("mcp-rs")
                    .join("logs"),
            );
        }
    }

    candidates.push(PathBuf::from("logs"));
    candidates.push(std::env::temp_dir().join("mcp-rs").join("logs"));

    candidates
}

/// `rotation`と現在時刻から、tracing-appenderが実際に使うログファイル名を
/// 再計算する（`tracing_appender::rolling::Inner::join_date`は非公開の
/// ためここで同じ命名規則を再現する）。呼び出し時点の時刻で都度計算する
/// ことで、プロセスを起動したまま日付/時刻をまたいでローテーションが
/// 発生していても、常に現在使われているはずの実際のファイル名を返す。
fn resolved_log_filename(rotation: &LogRotation) -> String {
    let now = chrono::Utc::now();
    match rotation {
        LogRotation::Never => "mcp-rs.log".to_string(),
        LogRotation::Daily => format!("mcp-rs.log.{}", now.format("%Y-%m-%d")),
        LogRotation::Hourly => format!("mcp-rs.log.{}", now.format("%Y-%m-%d-%H")),
    }
}

/// 起動時に確定した、実際に使われているログ出力先の状態。
/// `init_logging`がファイル出力有効時に一度だけ設定し、以後は
/// プロセス全体から`current_log_destination`で参照できる
/// （`wordpress_health_check`等の診断用途）。
#[derive(Debug, Clone)]
pub struct LogDestination {
    /// 実際に書き込みに使っているログディレクトリ
    /// （どの候補ディレクトリにも書き込めなかった場合はNone）
    pub dir: Option<PathBuf>,
    /// ファイル名の再計算に使うローテーション設定
    pub rotation: LogRotation,
    /// 第一候補への書き込みに失敗し、別の候補へフォールバックした場合、
    /// 最初に失敗した候補とその理由（フォールバックが発生しなかった
    /// 場合はNone）
    pub fallback_reason: Option<String>,
}

impl LogDestination {
    /// 現在書き込まれているはずの実際のログファイルパスを返す
    /// （ローテーションで日付/時刻サフィックスが変わるため、呼び出し
    /// 時点の時刻で再計算する）。
    pub fn current_file_path(&self) -> Option<PathBuf> {
        self.dir
            .as_ref()
            .map(|dir| dir.join(resolved_log_filename(&self.rotation)))
    }

    /// stderrへの起動時告知向けの短い要約文字列（決定内容のみ。
    /// ファイルの実在性等はまだ確認できていない起動直後の時点のため
    /// 含めない）。
    pub fn summary(&self) -> String {
        match (self.current_file_path(), &self.fallback_reason) {
            (Some(path), Some(reason)) => format!("{} (fallback: {})", path.display(), reason),
            (Some(path), None) => path.display().to_string(),
            (None, _) => {
                "disabled (no writable location found; logging to stderr only)".to_string()
            }
        }
    }

    /// `wordpress_health_check`向けの詳細な状態文字列。呼び出し時点で
    /// 実際にファイルシステムを確認し、実在性・サイズ・最終書き込み時刻を
    /// 含める。ファイルが見つからない場合は、書き込みは成功したように
    /// 見えてもOSレベルのパスリダイレクト（例: MSIXのパス仮想化）等で
    /// 実際には別の場所に書かれている可能性がある旨を明示する。
    pub fn detailed_status(&self) -> String {
        let Some(path) = self.current_file_path() else {
            return "disabled (no writable location found; logging to stderr only)".to_string();
        };

        let mut clauses = Vec::new();
        if let Some(reason) = &self.fallback_reason {
            clauses.push(format!("fallback: {}", reason));
        }

        match fs::metadata(&path) {
            Ok(metadata) => {
                clauses.push("exists".to_string());
                clauses.push(format!("{:.1} KB", metadata.len() as f64 / 1024.0));
                if let Ok(modified) = metadata.modified() {
                    let local: chrono::DateTime<chrono::Local> = modified.into();
                    clauses.push(format!("last write {}", local.format("%Y-%m-%d %H:%M:%S")));
                }
            }
            Err(e) => {
                clauses.push(format!(
                    "MISSING on disk ({}) — the process reported a successful write, \
                     but no file exists at this path; this can happen under OS-level path \
                     redirection (e.g. MSIX app-container virtualization of known folders)",
                    e
                ));
            }
        }

        format!("{} ({})", path.display(), clauses.join(", "))
    }
}

static LOG_DESTINATION: OnceLock<LogDestination> = OnceLock::new();

/// 現在のログ出力先の状態を取得する。ファイル出力が無効、または
/// `init_logging`がまだ実行されていない場合はNone。
pub fn current_log_destination() -> Option<LogDestination> {
    LOG_DESTINATION.get().cloned()
}

/// 実際に使うログ出力先を解決する。`config.log_dir`への書き込みに
/// 失敗した場合は`fallback_log_dir_candidates()`を順に試す。
/// 成功した候補には起動マーカー行（`write_startup_marker`）を書き込む。
/// 全候補が失敗した場合は`(None, ...)`を返し、呼び出し側はコンソール
/// 出力のみへ縮退する。
fn resolve_log_destination(
    config: &LogConfig,
) -> (Option<rolling::RollingFileAppender>, LogDestination) {
    let mut candidates = vec![config.log_dir.clone()];
    candidates.extend(fallback_log_dir_candidates());
    candidates.dedup();

    resolve_log_destination_from_candidates(&candidates, &config.rotation)
}

/// `resolve_log_destination`の候補探索ロジック本体。候補リストを引数で
/// 渡せるようにしてあるのは単体テストのため（本番では常に
/// `[config.log_dir] + fallback_log_dir_candidates()`を渡す）。
///
/// 各候補について、`create_dir_all` → ファイルをappendモードで開く →
/// 起動マーカー行を書いてflush → `fs::metadata`で実際にファイルが
/// 存在し中身が空でないことを確認、の全てに成功した場合のみ、その候補を
/// 採用する。途中の失敗（書き込み不可、またはOSは成功と報告したのに
/// ファイルが見当たらない）は次の候補へ進む判断材料とする。
fn resolve_log_destination_from_candidates(
    candidates: &[PathBuf],
    rotation: &LogRotation,
) -> (Option<rolling::RollingFileAppender>, LogDestination) {
    let mut first_failure: Option<String> = None;

    for (i, dir) in candidates.iter().enumerate() {
        if let Err(e) = ensure_log_dir(dir) {
            if first_failure.is_none() {
                first_failure = Some(format!("{} is not writable: {}", dir.display(), e));
            }
            continue;
        }

        let mut appender = match build_file_appender(dir, rotation, "mcp-rs.log") {
            Ok(appender) => appender,
            Err(e) => {
                if first_failure.is_none() {
                    first_failure = Some(format!("{} is not writable: {}", dir.display(), e));
                }
                continue;
            }
        };

        let filename = resolved_log_filename(rotation);
        if let Err(e) = write_startup_marker(&mut appender, dir, &filename) {
            if first_failure.is_none() {
                first_failure = Some(format!(
                    "{} accepted the write but it could not be verified: {}",
                    dir.display(),
                    e
                ));
            }
            continue;
        }

        let destination = LogDestination {
            dir: Some(dir.clone()),
            rotation: rotation.clone(),
            fallback_reason: if i == 0 { None } else { first_failure.clone() },
        };
        return (Some(appender), destination);
    }

    (
        None,
        LogDestination {
            dir: None,
            rotation: rotation.clone(),
            fallback_reason: first_failure,
        },
    )
}

/// 今回のプロセス起動を示すマーカー行をログファイルに書き込み、
/// `fs::metadata`で実際にディスク上にファイルが存在し中身が空でないかを
/// 確認する。ローテーションにより同じファイルへ複数回分の起動ログが
/// 積み重なるため、どこからが今回の起動分かを人間が目視で判別できる
/// ようにする（ファイルの先頭バイトという意味ではなく、今回の書き込みの
/// 先頭）。
///
/// 注意: このreadback確認は同一プロセス内で行うため、OSレベルで透過的に
/// 行われるパスリダイレクト（例: MSIXのパス仮想化）までは検出できない
/// （書き込みと読み込みの双方が同じようにリダイレクトされ、見かけ上は
/// 一貫して成功するため）。このケースへの対策は
/// `fallback_log_dir_candidates`側でAppData配下を避けること。
fn write_startup_marker(
    appender: &mut rolling::RollingFileAppender,
    dir: &Path,
    filename: &str,
) -> std::io::Result<()> {
    // 他のログ行と同じ表記（ローカル時刻・ミリ秒3桁・タイムゾーン付き）に
    // 揃える。`resolved_log_filename`のUTC基準の日付計算とは無関係の、
    // 人間が読むための表示用タイムスタンプ。
    let marker = format!(
        "=== mcp-rs {} started at {} / pid {} / log: {} ===\n",
        env!("CARGO_PKG_VERSION"),
        chrono::Local::now().format("%Y-%m-%dT%H:%M:%S%.3f%:z"),
        std::process::id(),
        dir.join(filename).display()
    );

    appender.write_all(marker.as_bytes())?;
    appender.flush()?;

    let metadata = fs::metadata(dir.join(filename))?;
    if metadata.len() == 0 {
        return Err(std::io::Error::other(
            "file exists but is empty after a successful write+flush",
        ));
    }

    Ok(())
}

/// ログディレクトリを確保
fn ensure_log_dir(dir: &Path) -> Result<()> {
    if !dir.exists() {
        fs::create_dir_all(dir)?;
    }
    Ok(())
}

/// ログファイル用の`RollingFileAppender`を構築する。
///
/// `rolling::daily()`等の便利関数は内部で`.expect()`しており、ログ
/// ディレクトリは存在するが書き込み権限がない場合（例:
/// `C:\Program Files\...\logs`を非昇格プロセスから使う場合）に初回の
/// ログファイル作成へ失敗し、サーバー全体がパニックで起動できなくなって
/// いた。`Builder::build()`は同じ初期化処理を行うが`Result`を返すため、
/// 呼び出し側で失敗を検出してコンソール出力への縮退にフォールバック
/// できる。
fn build_file_appender(
    log_dir: &Path,
    rotation: &LogRotation,
    filename_prefix: &str,
) -> Result<rolling::RollingFileAppender, rolling::InitError> {
    let tracing_rotation = match rotation {
        LogRotation::Daily => rolling::Rotation::DAILY,
        LogRotation::Hourly => rolling::Rotation::HOURLY,
        LogRotation::Never => rolling::Rotation::NEVER,
    };

    rolling::Builder::new()
        .rotation(tracing_rotation)
        .filename_prefix(filename_prefix)
        .build(log_dir)
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
/// ログシステムを初期化する。戻り値の`WorkerGuard`は、ファイル出力が
/// 非同期書き込み（`non_blocking`）で初期化された場合に`Some`になる。
/// **呼び出し側はこれをプロセス終了までどこかの変数に保持し続ける必要
/// がある**（`let _ = init_logging(...)?;`のように捨ててはならない）。
/// `WorkerGuard`がdropされるとバックグラウンドの書き込みスレッドが
/// 終了し、以後`tracing::info!`等の呼び出しは実際にはファイルへ
/// 書き込まれなくなる（チャンネルの受信側が既に存在しないため）。
pub fn init_logging(config: &LogConfig) -> Result<Option<WorkerGuard>> {
    // EnvFilterを作成
    let env_filter = EnvFilter::try_new(&config.level).unwrap_or_else(|_| EnvFilter::new("info"));

    // ファイル出力が有効な場合のみ、実際に書き込み可能なディレクトリを
    // フォールバック込みで解決する。結果はファイル書き込みに失敗する
    // ケースも含めて必ずstderrへ告知し（トレーシング初期化前なので
    // eprintln!で直接出す）、プロセス全体から参照できるグローバル状態
    // にも記録する（wordpress_health_check等の診断用途）。
    let file_appender = if config.file_enabled {
        let (appender, destination) = resolve_log_destination(config);
        eprintln!("[mcp-rs] log file: {}", destination.summary());
        let _ = LOG_DESTINATION.set(destination);
        appender
    } else {
        None
    };

    // ログ設定に基づいて初期化
    let guard = match (
        &config.console_enabled,
        &config.file_enabled,
        &config.module_separation,
    ) {
        (true, true, ModuleSeparation::Single) => {
            init_single_file_logging(file_appender, env_filter)?
        }
        (true, true, ModuleSeparation::Separated) => {
            init_separated_logging(file_appender, env_filter)?
        }
        (true, true, ModuleSeparation::Hybrid) => {
            init_hybrid_full_logging(file_appender, env_filter)?
        }
        (true, false, _) => {
            // コンソールのみ
            init_console_only_logging(env_filter)?;
            None
        }
        (false, true, separation) => {
            // ファイルのみ
            init_file_only_logging(file_appender, env_filter, separation)?
        }
        (false, false, _) => {
            // 最低限のコンソール出力
            tracing_subscriber::fmt()
                .with_max_level(tracing::Level::WARN)
                .event_format(pid_formatter())
                .init();
            None
        }
    };

    tracing::info!("📝 ログシステム初期化完了");
    match current_log_destination() {
        Some(destination) => tracing::info!("📂 ログ出力先: {}", destination.summary()),
        None => tracing::info!(
            "📂 ログディレクトリ（設定値）: {}",
            config.log_dir.display()
        ),
    }
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

    // ログ削除ポリシーを適用（External以外、かつ実際に書き込み先が
    // 解決できた場合のみ。解決できなかった場合は対象ディレクトリがない）
    if let Some(log_dir) = current_log_destination().and_then(|d| d.dir) {
        if let Err(e) = apply_retention_policy(config, &log_dir) {
            tracing::warn!("ログ保持ポリシー適用に失敗: {}", e);
        }
    }

    Ok(guard)
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

/// ログ保持ポリシーを適用する。`log_dir`には実際に書き込みに使われている
/// ディレクトリ（フォールバックが発生していればその先）を渡す必要がある
/// （`config.log_dir`は第一候補にすぎず、そこへの書き込みが失敗して
/// 他のディレクトリへフォールバックしている場合がある）。
fn apply_retention_policy(config: &LogConfig, log_dir: &Path) -> Result<()> {
    match &config.retention {
        LogRetention::External => {
            // 外部管理なので何もしない（推奨アプローチ）
            Ok(())
        }
        LogRetention::Days(days) => cleanup_old_logs_by_age(log_dir, *days),
        LogRetention::Count(max_count) => cleanup_old_logs_by_count(log_dir, *max_count),
        LogRetention::Size(max_bytes) => cleanup_old_logs_by_size(log_dir, *max_bytes),
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
    log_files.sort_by_key(|b| std::cmp::Reverse(b.1));

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
    log_files.sort_by_key(|b| std::cmp::Reverse(b.1));

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
/// `file_appender`は`init_logging`が`resolve_log_destination`で事前に
/// 解決済み（フォールバック・失敗告知込み）のものを受け取る。
fn init_single_file_logging(
    file_appender: Option<rolling::RollingFileAppender>,
    env_filter: EnvFilter,
) -> Result<Option<WorkerGuard>> {
    match file_appender {
        Some(file_appender) => {
            // `guard`はここで即座にdropしてはならない。dropするとバック
            // グラウンドの書き込みスレッドが終了し、以後のログ出力が
            // 実際にはファイルへ届かなくなる。呼び出し元（最終的には
            // `main`）がプロセス終了まで保持する。
            let (non_blocking, guard) = non_blocking(file_appender);

            // コンソール（stderr）とファイルを1つのfmt layerに
            // `.and()`で束ねると、ANSIカラー設定も両者に同じ値が
            // 適用されてしまう（ファイルにエスケープコードが混入する）。
            // そのため別々のlayerに分け、ファイル側だけ`.with_ansi(false)`
            // にする。
            let console_layer = fmt::layer()
                .with_writer(std::io::stderr)
                .event_format(pid_formatter());

            let file_layer = fmt::layer()
                .with_writer(non_blocking)
                .with_ansi(false)
                .event_format(pid_formatter());

            tracing_subscriber::registry()
                .with(env_filter)
                .with(console_layer)
                .with(file_layer)
                .init();

            Ok(Some(guard))
        }
        None => {
            // どの候補ディレクトリにも書き込めなかった場合
            // （resolve_log_destinationが既にstderrへ告知済み）。
            // 以前はrolling::daily()内部の.expect()でパニックし、サーバーが
            // 起動できなくなっていたため、コンソール出力のみへ縮退させる。
            init_console_only_logging(env_filter)?;
            Ok(None)
        }
    }
}

/// 分離ログ初期化（本番推奨）
fn init_separated_logging(
    file_appender: Option<rolling::RollingFileAppender>,
    env_filter: EnvFilter,
) -> Result<Option<WorkerGuard>> {
    // 暫定実装：コンソール＋単一ファイル
    // TODO: 真のモジュール別分離実装
    tracing::warn!("モジュール別分離ログは開発中です。暫定的に単一ファイルを使用します。");
    init_single_file_logging(file_appender, env_filter)
}

/// ハイブリッドログ初期化（概要＋詳細分離）
fn init_hybrid_full_logging(
    file_appender: Option<rolling::RollingFileAppender>,
    env_filter: EnvFilter,
) -> Result<Option<WorkerGuard>> {
    // 暫定実装：コンソール＋単一ファイル
    // TODO: 概要＋詳細分離実装
    tracing::warn!("ハイブリッドログは開発中です。暫定的に単一ファイルを使用します。");
    init_single_file_logging(file_appender, env_filter)
}

/// コンソールのみログ初期化
fn init_console_only_logging(env_filter: EnvFilter) -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .event_format(pid_formatter())
        .init();

    Ok(())
}

/// ファイルのみログ初期化
fn init_file_only_logging(
    file_appender: Option<rolling::RollingFileAppender>,
    env_filter: EnvFilter,
    separation: &ModuleSeparation,
) -> Result<Option<WorkerGuard>> {
    match separation {
        ModuleSeparation::Single => match file_appender {
            Some(file_appender) => {
                // guardの扱いは`init_single_file_logging`と同じ理由で
                // 呼び出し元へ返す（即dropしない）。
                let (non_blocking, guard) = non_blocking(file_appender);

                tracing_subscriber::fmt()
                    .with_env_filter(env_filter)
                    .with_writer(non_blocking)
                    .with_ansi(false)
                    .event_format(pid_formatter())
                    .init();

                Ok(Some(guard))
            }
            None => {
                // ファイルのみモードでもログ出力自体を失わないよう、
                // 書き込み失敗時はコンソール出力に縮退させる
                // （resolve_log_destinationが既にstderrへ告知済み）。
                init_console_only_logging(env_filter)?;
                Ok(None)
            }
        },
        _ => {
            tracing::warn!(
                "分離ログはファイルのみモードでは未実装です。単一ファイルを使用します。"
            );
            init_file_only_logging(file_appender, env_filter, &ModuleSeparation::Single)
        }
    }
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
        assert!(config.console_enabled);
        assert!(config.file_enabled);
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

    /// ログディレクトリが存在するが書き込めない場合（Windowsでは権限不足
    /// 等で再現されるが、ここでは「親パスがディレクトリでない」という
    /// 同種の書き込み不可条件で再現する）に、パニックせず`Err`を返す
    /// ことを確認する回帰テスト。以前は`rolling::daily()`等の便利関数が
    /// 内部で`.expect()`しており、この状況でサーバーごとパニックしていた。
    #[test]
    fn test_build_file_appender_returns_err_instead_of_panicking_when_unwritable() {
        let temp_dir = tempdir().unwrap();
        let not_a_dir = temp_dir.path().join("not_a_directory");
        fs::write(&not_a_dir, b"this is a file, not a directory").unwrap();

        let result = build_file_appender(&not_a_dir, &LogRotation::Daily, "mcp-rs.log");
        assert!(
            result.is_err(),
            "writing into a non-directory path must return Err, not panic"
        );
    }

    #[test]
    fn test_build_file_appender_succeeds_for_writable_directory() {
        let temp_dir = tempdir().unwrap();
        let log_dir = temp_dir.path().join("logs");
        fs::create_dir_all(&log_dir).unwrap();

        let result = build_file_appender(&log_dir, &LogRotation::Daily, "mcp-rs.log");
        assert!(result.is_ok());
    }

    #[test]
    fn test_ensure_log_dir() {
        let temp_dir = tempdir().unwrap();
        let log_dir = temp_dir.path().join("test_logs");

        assert!(ensure_log_dir(&log_dir).is_ok());
        assert!(log_dir.exists());
    }

    #[test]
    fn test_log_destination_summary_formats() {
        // パス区切り文字はプラットフォーム依存（Windowsでは`\`）のため、
        // 期待値はリテラル文字列ではなく`PathBuf::join`で組み立てる。
        let plain_dir = PathBuf::from("var").join("log").join("mcp-rs");
        let plain = LogDestination {
            dir: Some(plain_dir.clone()),
            rotation: LogRotation::Never,
            fallback_reason: None,
        };
        assert_eq!(
            plain.summary(),
            plain_dir.join("mcp-rs.log").display().to_string()
        );

        let fallback_dir = PathBuf::from("home")
            .join("user")
            .join(".local")
            .join("state")
            .join("mcp-rs")
            .join("logs");
        let fallback = LogDestination {
            dir: Some(fallback_dir.clone()),
            rotation: LogRotation::Never,
            fallback_reason: Some("/var/log/mcp-rs is not writable: Permission denied".to_string()),
        };
        assert_eq!(
            fallback.summary(),
            format!(
                "{} (fallback: /var/log/mcp-rs is not writable: Permission denied)",
                fallback_dir.join("mcp-rs.log").display()
            )
        );

        // 全候補失敗時は理由を問わず固定の文言にする（ユーザー要求の
        // 文言に合わせる）
        let disabled = LogDestination {
            dir: None,
            rotation: LogRotation::Never,
            fallback_reason: Some("ignored for disabled case".to_string()),
        };
        assert_eq!(
            disabled.summary(),
            "disabled (no writable location found; logging to stderr only)"
        );
    }

    #[test]
    fn test_log_destination_detailed_status_reports_size_and_missing_file() {
        let temp_dir = tempdir().unwrap();
        let log_dir = temp_dir.path().join("logs");
        fs::create_dir_all(&log_dir).unwrap();
        fs::write(log_dir.join("mcp-rs.log"), b"hello world").unwrap();

        let existing = LogDestination {
            dir: Some(log_dir.clone()),
            rotation: LogRotation::Never,
            fallback_reason: None,
        };
        let status = existing.detailed_status();
        assert!(status.contains("exists"));
        assert!(status.contains("KB"));
        assert!(status.contains("last write"));

        // 書き込みに「成功した」はずのディレクトリに実ファイルが無い場合
        // （例: MSIXのパス仮想化でリダイレクトされていた場合）を再現する。
        let missing_dir = temp_dir.path().join("never_actually_written");
        let missing = LogDestination {
            dir: Some(missing_dir),
            rotation: LogRotation::Never,
            fallback_reason: None,
        };
        assert!(missing.detailed_status().contains("MISSING on disk"));
    }

    /// 第一候補への書き込みが失敗した場合に、次の候補へ実際に
    /// フォールバックし、失敗理由を記録することを確認する回帰テスト。
    #[test]
    fn test_resolve_log_destination_falls_back_to_next_candidate_on_failure() {
        let temp_dir = tempdir().unwrap();
        let unwritable = temp_dir.path().join("not_a_directory");
        fs::write(&unwritable, b"not a directory").unwrap();
        let writable = temp_dir.path().join("writable_logs");

        let (appender, destination) = resolve_log_destination_from_candidates(
            &[unwritable.clone(), writable.clone()],
            &LogRotation::Never,
        );

        assert!(appender.is_some());
        assert_eq!(destination.dir, Some(writable.clone()));
        assert_eq!(
            destination.current_file_path(),
            Some(writable.join("mcp-rs.log"))
        );
        // 実際にファイルへ書き込まれ、readbackで確認できていること
        let status = destination.detailed_status();
        assert!(status.contains("exists"));
        assert!(!status.contains("MISSING"));

        let reason = destination
            .fallback_reason
            .expect("fallback to the 2nd candidate must record why the 1st one failed");
        assert!(reason.contains(&unwritable.display().to_string()));
    }

    #[test]
    fn test_resolve_log_destination_no_fallback_reason_when_first_candidate_succeeds() {
        let temp_dir = tempdir().unwrap();
        let writable = temp_dir.path().join("logs");

        let (appender, destination) =
            resolve_log_destination_from_candidates(&[writable.clone()], &LogRotation::Never);

        assert!(appender.is_some());
        assert!(destination.fallback_reason.is_none());
        assert_eq!(
            destination.summary(),
            writable.join("mcp-rs.log").display().to_string()
        );
    }

    #[test]
    fn test_resolve_log_destination_disabled_when_all_candidates_fail() {
        let temp_dir = tempdir().unwrap();
        let unwritable1 = temp_dir.path().join("bad1");
        let unwritable2 = temp_dir.path().join("bad2");
        fs::write(&unwritable1, b"not a directory").unwrap();
        fs::write(&unwritable2, b"not a directory").unwrap();

        let (appender, destination) = resolve_log_destination_from_candidates(
            &[unwritable1, unwritable2],
            &LogRotation::Never,
        );

        assert!(appender.is_none());
        assert_eq!(destination.dir, None);
        assert_eq!(
            destination.summary(),
            "disabled (no writable location found; logging to stderr only)"
        );
    }

    /// 起動マーカー行がバージョン・PIDを含んだ固定フォーマットで
    /// ログファイルへ書き込まれ、readback確認（`fs::metadata`）にも
    /// 成功することを確認する。
    #[test]
    fn test_write_startup_marker_contains_version_and_pid() {
        let temp_dir = tempdir().unwrap();
        let log_dir = temp_dir.path().join("logs");
        fs::create_dir_all(&log_dir).unwrap();

        let mut appender =
            build_file_appender(&log_dir, &LogRotation::Never, "mcp-rs.log").unwrap();
        let result = write_startup_marker(&mut appender, &log_dir, "mcp-rs.log");
        assert!(result.is_ok());
        drop(appender);

        let contents = fs::read_to_string(log_dir.join("mcp-rs.log")).unwrap();
        assert!(contents.starts_with("=== mcp-rs "));
        assert!(contents.contains(env!("CARGO_PKG_VERSION")));
        assert!(contents.contains(&std::process::id().to_string()));
    }

    /// `resolved_log_filename`が各ローテーション設定で期待する命名規則
    /// （tracing-appender内部の`Inner::join_date`と同じ形式）を返すこと
    /// を確認する。
    #[test]
    fn test_resolved_log_filename_matches_rotation_naming() {
        assert_eq!(resolved_log_filename(&LogRotation::Never), "mcp-rs.log");

        let daily = resolved_log_filename(&LogRotation::Daily);
        assert!(daily.starts_with("mcp-rs.log."));
        assert_eq!(daily.len(), "mcp-rs.log.".len() + "YYYY-MM-DD".len());

        let hourly = resolved_log_filename(&LogRotation::Hourly);
        assert!(hourly.starts_with("mcp-rs.log."));
        assert_eq!(hourly.len(), "mcp-rs.log.".len() + "YYYY-MM-DD-HH".len());
    }

    #[test]
    fn test_log_stats_format_size() {
        let mut stats = LogStats {
            total_size: 1024,
            ..Default::default()
        };
        assert_eq!(stats.format_size(), "1.00 KB");

        stats.total_size = 1024 * 1024;
        assert_eq!(stats.format_size(), "1.00 MB");

        stats.total_size = 1536; // 1.5 KB
        assert_eq!(stats.format_size(), "1.50 KB");
    }
}
