//! MCP-RS Transport統合型エントリポイント
//!
//! 新しいTransport抽象化を使用したクリーンなアーキテクチャ

use mcp_rs::config::McpConfig;
use mcp_rs::handlers::WordPressHandler;
use mcp_rs::logging::{init_logging, LogConfig};
use std::sync::Arc;
use tracing::info;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // mcp_rs_statusの起動からの時間の起点（#434）
    mcp_rs::mcp::process_started_at();
    // サブコマンドは設定ファイルを読み込む前に処理する（設定ファイルが
    // まだ無い状態でencrypted_credentialsを作るためのコマンドなので）
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some(mcp_rs::security::credential_tool::SUBCOMMAND) {
        // Box<dyn Error>をmainから返すとDebug表記（"..."付き）で出るため、
        // ここでDisplay表記で出して終了コード1にする
        if let Err(e) = mcp_rs::security::credential_tool::run(&args[2..]) {
            eprintln!("Error: {e}");
            std::process::exit(1);
        }
        return Ok(());
    }
    // OSの資格情報ストアへの登録等（#533）。同じく設定ファイルより前に処理する
    if args.get(1).map(String::as_str) == Some(mcp_rs::security::keyring_tool::SUBCOMMAND) {
        if let Err(e) = mcp_rs::security::keyring_tool::run(&args[2..]) {
            eprintln!("Error: {e}");
            std::process::exit(1);
        }
        return Ok(());
    }

    // --version・--help（#431）。サーバーは起動せず、設定・ログにも触れない
    let run_args = match mcp_rs::cli_args::parse_args(&args) {
        mcp_rs::cli_args::Command::Version => {
            println!("{}", mcp_rs::cli_args::version_text());
            return Ok(());
        }
        mcp_rs::cli_args::Command::Help => {
            print!("{}", mcp_rs::cli_args::help_text());
            return Ok(());
        }
        mcp_rs::cli_args::Command::Run(run_args) => run_args,
        // 知らない引数等（#437）。打ち間違いに気づかず別の設定で起動しない
        // よう、起動せずに終わる
        mcp_rs::cli_args::Command::Invalid(problem) => {
            eprintln!("error: {}\n", problem);
            eprint!("{}", mcp_rs::cli_args::help_text());
            std::process::exit(2);
        }
    };

    // 設定読み込み（ログ設定のため最初に実行）
    let config = load_config(run_args.config.as_deref()).await?;

    // ログシステム初期化
    // `_log_guard`はプロセス終了まで保持する必要がある（dropすると
    // バックグラウンドの書き込みスレッドが終了し、以後のログが
    // 実際にはファイルへ届かなくなる）。`let _ = ...`で捨てないこと。
    let log_config = LogConfig::from_server_config(&config.server);
    let _log_guard = init_logging(&log_config)?;

    info!(
        "🚀 MCP-RS v{} - Transport統合アーキテクチャ",
        env!("CARGO_PKG_VERSION")
    );
    // 実際のログ出力先（フォールバック解決後）はinit_logging内部で
    // 既に1箇所だけ表示済み（"📂 ログ出力先: ..."）。ここで
    // log_config.log_dir（未解決の設定値）を別に表示すると、
    // フォールバックが発生した場合に2つの異なるパスが出て矛盾する。
    info!("✅ 設定読み込み完了");

    // どちらのモードも、ハンドラー（WordPress・プラグイン）を登録した
    // McpServerでリクエストを処理する。HTTPモードは以前core::Runtime経由
    // だったが、受信ループが動いておらずリクエストを処理できなかった
    // （#400。core::Runtimeは#409で削除）
    let transport_config = config.to_transport_config();
    match transport_config.transport_type {
        mcp_rs::transport::TransportType::Http { addr } => {
            let mut http_config = transport_config.http.clone();
            http_config.bind_addr = addr;
            run_http_server(&config, http_config).await
        }
        _ => {
            if let Some(other) = config
                .transport
                .transport_type
                .as_deref()
                .filter(|t| *t != "stdio")
            {
                tracing::warn!(
                    "Unknown transport type '{}': running in STDIO mode. Use \"stdio\" or \"http\"",
                    other
                );
            }
            run_stdio_server(&config).await
        }
    }
}

/// 設定ファイル読み込み。`--config`の指定があればそのファイル、無ければ
/// 既定の場所から探す
async fn load_config(config_path: Option<&str>) -> Result<McpConfig, Box<dyn std::error::Error>> {
    match config_path {
        Some(path) => {
            // `~/`はホームフォルダに展開する（シェルを通さないクライアントでは
            // そのまま届く）。相対パスは、起動したフォルダから読むので警告する
            // （PR #551のレビュー）。ログはまだ無いので、標準エラーに書く
            // （標準出力はSTDIOの通信用）
            let path = mcp_rs::config::expand_config_path(path);
            if let Some(warning) = mcp_rs::config::relative_config_path_warning(&path) {
                eprintln!("[mcp-rs] warning: {warning}");
            }
            load_config_from_file(&path).await
        }
        None => McpConfig::load(),
    }
}

/// 指定パスから設定読み込み
async fn load_config_from_file(path: &str) -> Result<McpConfig, Box<dyn std::error::Error>> {
    if !std::path::Path::new(path).exists() {
        return Err(format!("設定ファイルが存在しません: {}", path).into());
    }

    let content = tokio::fs::read_to_string(path).await?;
    let mut config: McpConfig =
        toml::from_str(&content).map_err(|e| mcp_rs::config::with_config_hint(e, &content))?;
    // 監査ログの既定の書き込み先（設定ファイルと同じフォルダ）に使う（#416）
    config.config_file = std::path::absolute(path).ok();

    // WordPressConfig の環境変数展開を適用（McpConfig::load() と同様の処理）
    if let Some(ref mut wp_config) = config.handlers.wordpress {
        // 参照した環境変数が無ければ起動しない（#454）。項目と変数名を出す
        McpConfig::expand_wordpress_config(wp_config).map_err(|e| e.to_string())?;
        wp_config.resolve_credentials()?;
    }
    // HTTPモードのAPIキーの`${VAR}`（#534）。変数が無ければ起動しない
    config.expand_http_api_keys().map_err(|e| e.to_string())?;

    info!("✅ カスタム設定読み込み: {}", path);
    Ok(config)
}

/// 設定に従ってハンドラー（WordPress・プラグイン）を登録したMCPサーバーを作る
async fn build_server(
    config: &McpConfig,
    http_mode: bool,
) -> Result<mcp_rs::mcp::McpServer, Box<dyn std::error::Error>> {
    let mut server = mcp_rs::mcp::McpServer::new();

    // 監査ログ（#416）。WordPressとプラグインで同じものを使う。HTTPモードで
    // 呼び出しを拒否したツール（#52）も記録する
    let audit_logger = Arc::new(mcp_rs::security::AuditLogger::new(config.audit_config()));
    server.set_audit_logger(audit_logger.clone());
    match audit_logger.file_path() {
        Some(path) => match audit_logger.check_file_output().await {
            Ok(()) => info!("📝 監査ログ: {}", path),
            Err(e) => tracing::warn!(
                "Audit log file cannot be written ({}). Tool calls are kept in memory only. Set [audit] file_path to a writable location",
                e
            ),
        },
        None if config.audit.as_ref().and_then(|a| a.enabled) == Some(false) => {
            info!("📝 監査ログ: ファイルには書かない（[audit] enabled = false）")
        }
        None => tracing::warn!(
            "Audit log is kept in memory only: no folder for it could be found (USERPROFILE/HOME is not set and no config file was loaded). Set [audit] file_path to an absolute path to write it to a file"
        ),
    }
    // 以前の既定の場所（設定ファイルのフォルダ）に残っている監査ログ（#474）。
    // 移さずに残すので、どこにあるかを知らせる
    if let Some(legacy) = config.legacy_audit_log_path() {
        info!(
            "📝 Audit log entries written by older versions remain in {}. New entries go to the audit log shown above; the old file was not moved",
            legacy.display()
        );
    }

    if let Some(wp_config) = &config.handlers.wordpress {
        if wp_config.enabled.unwrap_or(false) {
            let wp_handler = WordPressHandler::try_new(wp_config.clone())
                .map_err(|e| format!("WordPress handler creation failed: {}", e))?
                .with_audit_logger(audit_logger.clone());

            // STDIOと違い、HTTP/WebSocketのクライアントは別のマシンにいる
            // ことがある。その場合、file_pathで読まれるのはクライアントの
            // 手元ではなく、このサーバー上のファイルになる
            if http_mode && !wp_config.upload_allowed_dirs.is_empty() {
                tracing::warn!(
                    "upload_allowed_dirs is set: clients connected over HTTP/WebSocket can upload files from these folders on this server via upload_media file_path"
                );
            }

            server.add_handler("wordpress".to_string(), Arc::new(wp_handler));
            info!("✅ WordPressハンドラー登録完了: {}", wp_config.url);
        }
    }

    // データベース（#453）。つながらなければ、プラグインと同じく警告を出して
    // 飛ばし、理由をmcp_rs_statusに出す
    let mut startup_failures = Vec::new();
    if let Some(db_config) = config.database.as_ref().filter(|db| db.is_enabled()) {
        // ${VAR}はここで展開する。環境変数が無くても、データベースだけを飛ばし、
        // WordPress等は動かす（#453のレビュー）
        let mut db_config = db_config.clone();
        db_config.resolve_paths(
            config
                .config_file
                .as_deref()
                .and_then(std::path::Path::parent),
        );
        let registered = match McpConfig::expand_database_config(&mut db_config) {
            Ok(()) => register_database(&mut server, &db_config, audit_logger.clone()).await,
            Err(e) => Err(e.to_string()),
        };
        match registered {
            Ok(()) => info!(
                "✅ データベースハンドラー登録完了: {}",
                db_config.describe()
            ),
            Err(reason) => {
                tracing::warn!("Database handler was not started: {}", reason);
                startup_failures.push(("database".to_string(), reason));
            }
        }
    }

    // プラグイン（#394）。起動できなかったプラグインは警告を出して飛ばす
    let (plugins, plugin_failures) = mcp_rs::plugin_proxy::start_plugins_reporting_failures(
        &config.plugins,
        audit_logger.clone(),
    )
    .await;
    for (name, handler) in plugins {
        server.add_handler(format!("plugin:{}", name), handler);
    }
    startup_failures.extend(plugin_failures);

    // mcp_rs_statusツール（#434）に出す情報
    server.set_status_info(mcp_rs::mcp::StatusInfo {
        mode: if http_mode { "http" } else { "stdio" }.to_string(),
        config_file: config.config_file.clone(),
        started_at: mcp_rs::mcp::process_started_at(),
        plugin_failures: startup_failures,
    });

    Ok(server)
}

/// `[database]`のハンドラーを登録する（#453）。失敗の理由を返す
#[cfg(feature = "database")]
async fn register_database(
    server: &mut mcp_rs::mcp::McpServer,
    settings: &mcp_rs::config::DatabaseSettings,
    audit_logger: Arc<mcp_rs::security::AuditLogger>,
) -> Result<(), String> {
    let handler = mcp_rs::handlers::database::handler::DatabaseHandler::from_settings(settings)
        .await?
        .with_audit_logger(audit_logger);
    server.add_handler("database".to_string(), Arc::new(handler));
    Ok(())
}

/// `database`機能を含めずにビルドしたmcp-rsでは、`[database]`を使えない
#[cfg(not(feature = "database"))]
async fn register_database(
    _server: &mut mcp_rs::mcp::McpServer,
    _settings: &mcp_rs::config::DatabaseSettings,
    _audit_logger: Arc<mcp_rs::security::AuditLogger>,
) -> Result<(), String> {
    Err(
        "this build of mcp-rs does not include database support (it was built with --no-default-features; build without it, or add --features database)"
            .to_string(),
    )
}

/// STDIOモードでMCPサーバーを実行（initialize / tools/list / tools/call 等に応答）
async fn run_stdio_server(config: &McpConfig) -> Result<(), Box<dyn std::error::Error>> {
    let server = build_server(config, false).await?;
    info!("🔌 STDIOモードで待機中");
    server.run_stdio().await
}

/// HTTPモードでMCPサーバーを実行する。`/mcp`（と`/`）へのPOST、および
/// `enable_websocket_upgrade`が有効なら`/ws`のWebSocketで、STDIOと同じ
/// 処理（McpServer）に応答させる（#400）。Ctrl+Cで終了する。
async fn run_http_server(
    config: &McpConfig,
    http_config: mcp_rs::transport::HttpConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    let server = build_server(config, true).await?;

    // 認証の失敗も、ツール呼び出しと同じ監査ログに記録する（#55）
    let mut transport = mcp_rs::transport::HttpTransport::new(http_config)?;
    if let Some(audit_logger) = server.audit_logger() {
        transport = transport.with_audit_logger(audit_logger);
    }
    let transport = transport.with_request_handler(Arc::new(server.dispatcher()));
    transport.start_server().await?;
    if let Some(addr) = transport.bound_addr().await {
        info!("🌐 HTTPモードで待機中: http://{}/mcp", addr);
    }
    info!("💡 Ctrl+C で終了");

    tokio::signal::ctrl_c().await?;
    info!("👋 MCP-RS終了");
    // serverを破棄すると、プラグインの子プロセスも終了する
    drop(server);
    Ok(())
}
