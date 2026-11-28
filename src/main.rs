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

    // mcp-rsの登録を外す（#636）
    if args.get(1).map(String::as_str) == Some(mcp_rs::uninstall::SUBCOMMAND) {
        match mcp_rs::uninstall::run(&args[2..]) {
            Ok(true) => return Ok(()),
            Ok(false) => std::process::exit(1),
            Err(e) => {
                eprintln!("Error: {e}");
                std::process::exit(2);
            }
        }
    }

    // データベースのパスワードを資格情報ストアに入れる等（#628）
    if args.get(1).map(String::as_str) == Some(mcp_rs::database_keyring::SUBCOMMAND) {
        if let Err(e) = mcp_rs::database_keyring::run(&args[2..]) {
            eprintln!("Error: {e}");
            std::process::exit(1);
        }
        return Ok(());
    }

    // 設定を確かめる（#565）。サーバーは起動しない
    if args.get(1).map(String::as_str) == Some(mcp_rs::check::SUBCOMMAND) {
        match mcp_rs::check::run(&args[2..]).await {
            Ok(true) => return Ok(()),
            Ok(false) => std::process::exit(1),
            Err(problem) => {
                eprintln!(
                    "error: {}
",
                    problem
                );
                eprint!("{}", mcp_rs::check::help_text());
                std::process::exit(2);
            }
        }
    }

    // 質問に答えて使い始める準備をする（#575）。サーバーは起動しない
    if args.get(1).map(String::as_str) == Some(mcp_rs::init::SUBCOMMAND) {
        match mcp_rs::init::run(&args[2..]).await {
            Ok(true) => return Ok(()),
            Ok(false) => std::process::exit(1),
            Err(problem) => {
                eprintln!("error: {problem}\n");
                eprint!("{}", mcp_rs::init::help_text());
                std::process::exit(2);
            }
        }
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

    // 設定読み込み（ログ設定のため最初に実行）。`MCP_RS_CONFIG`は`--config`
    // より優先する（Claude Desktopの拡張機能で選んだ設定ファイル、#632）
    let config_path = mcp_rs::config::config_path_from_env(
        std::env::var(mcp_rs::config::CONFIG_ENV_VAR)
            .ok()
            .as_deref(),
    )
    .or(run_args.config.clone());
    let config = load_config(config_path.as_deref()).await?;

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
    match &config.missing_config_file {
        // 設定なしで起動した（#667）。クライアントには「起動しない」としか
        // 出ないので、終わらずに、mcp_rs_statusでも直し方を伝える
        Some(message) => tracing::warn!(
            "{message}. Starting without a config file: only the mcp_rs_status tool is available"
        ),
        None => info!("✅ 設定読み込み完了"),
    }

    // どちらのモードも、ハンドラー（WordPress・プラグイン）を登録した
    // McpServerでリクエストを処理する。HTTPモードは以前core::Runtime経由
    // だったが、受信ループが動いておらずリクエストを処理できなかった
    // （#400。core::Runtimeは#409で削除）
    let transport_config = config.to_transport_config();
    // 終了の合図（Ctrl+C・SIGTERM）は、プラグインを起動する前から受ける
    // （#759。起動の途中で受けても、起動した後にプラグインを止める）
    let shutdown = ShutdownSignal::listen();
    match transport_config.transport_type {
        mcp_rs::transport::TransportType::Http { addr } => {
            let mut http_config = transport_config.http.clone();
            http_config.bind_addr = addr;
            run_http_server(&config, http_config, shutdown).await
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
            // 標準入力の読み込みは止められず、tokioの終了がそれを待つので、
            // 読み終える前に終わるときは、ログを書き終えてから、ここで終わる
            // （#617）。標準出力に書けずに終わったとき（クライアントが去った
            // 等）も、標準入力の終わりを待たずに、0以外で終わる（PR #765の
            // レビュー）。プラグインは`run_stdio_server`の中で止めてある
            match run_stdio_server(&config, shutdown).await {
                Ok(true) => {
                    drop(_log_guard);
                    std::process::exit(0);
                }
                // 標準入力の終わり（EOF）: 読み込みは終わっている
                Ok(false) => Ok(()),
                Err(e) => {
                    tracing::error!("MCP-RS stopped: {}", e);
                    drop(_log_guard);
                    eprintln!("Error: {e:?}");
                    std::process::exit(1);
                }
            }
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
        // 設定ファイルが無ければ、STDIOモードでは設定なしで起動する（#667）
        None => McpConfig::load_for_startup(),
    }
}

/// 指定パスから設定読み込み
async fn load_config_from_file(path: &str) -> Result<McpConfig, Box<dyn std::error::Error>> {
    // 誤りのある行（パスワードのこともある）は出さない（#543）。`mcp-rs
    // check`（#565）も、同じ関数を同じ順で呼ぶ
    let mut config = McpConfig::read_file(path)?;
    // Claude Desktopの拡張機能で、サイトがまだ入力されていなければ、WordPressを
    // 使わずに起動する（#632）
    if mcp_rs::config::started_by_extension() {
        config.prepare_for_extension();
    }
    // WordPressの`${VAR}`と資格情報。参照した環境変数が無ければ起動しない（#454）
    config.resolve_wordpress_credentials()?;
    // HTTPモードのAPIキーの`${VAR}`（#534）。変数が無ければ起動しない
    config.expand_http_api_keys().map_err(|e| e.to_string())?;

    info!("✅ カスタム設定読み込み: {}", path);
    Ok(config)
}

/// 設定に従ってハンドラー（WordPress・プラグイン）を登録したMCPサーバーを作る
async fn build_server(
    config: &McpConfig,
    http_mode: bool,
    shutdown: &mut ShutdownSignal,
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
            // サイトごとにハンドラーを作る（#640）。ほかのサイトが無ければ、
            // 今までどおり1つのハンドラーを登録する（引数`site`は足さない）
            let site_list = wp_config.site_list()?;
            let mut sites: Vec<(String, Arc<dyn mcp_rs::mcp::McpHandler>)> = Vec::new();
            for (name, site_config) in site_list {
                let wp_handler = WordPressHandler::try_new(site_config.clone())
                    .map_err(|e| format!("WordPress handler creation failed ({}): {}", name, e))?
                    .with_audit_logger(audit_logger.clone())
                    // 許可フォルダの一覧は、stdioのときだけエラーに出す（#819。
                    // HTTP・WebSocketの相手に、このサーバーのフォルダを見せない）
                    .with_upload_dirs_in_errors(!http_mode);

                // STDIOと違い、HTTP/WebSocketのクライアントは別のマシンにいる
                // ことがある。その場合、file_pathで読まれるのはクライアントの
                // 手元ではなく、このサーバー上のファイルになる
                if http_mode
                    && site_config
                        .upload_allowed_dirs
                        .as_ref()
                        .is_some_and(|dirs| !dirs.is_empty())
                {
                    tracing::warn!(
                        "upload_allowed_dirs is set: clients connected over HTTP/WebSocket can upload files from these folders on this server via upload_media file_path"
                    );
                }
                info!(
                    "✅ WordPressハンドラー登録完了: {} ({})",
                    site_config.url, name
                );
                sites.push((name, Arc::new(wp_handler)));
            }

            if sites.len() == 1 {
                let (_, handler) = sites.remove(0);
                server.add_handler("wordpress".to_string(), handler);
            } else {
                server.add_handler(
                    "wordpress".to_string(),
                    Arc::new(mcp_rs::handlers::wordpress_sites::WordPressSites::new(
                        sites,
                    )),
                );
            }
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
        // `database`機能の無いビルドでは、環境変数・資格情報を読む前に、
        // 使えないことを伝える（#737。変数を設定しても使えないため）
        let resolved = if cfg!(feature = "database") {
            McpConfig::resolve_database_credentials(&mut db_config)
        } else {
            Ok(())
        };
        let registered = match resolved {
            Ok(()) => register_database(&mut server, &db_config, audit_logger.clone()).await,
            Err(e) => Err(e),
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

    // プラグイン（#394）。起動できなかったプラグインは警告を出して飛ばす。
    // 起動の途中で終了の合図を受けたら、起動を待たずにやめ、後片付けを
    // 始める（#775。起動し終えたプラグインは、呼ぶ側が止める）
    let (plugins, plugin_failures) =
        mcp_rs::plugin_proxy::start_plugins_until(&config.plugins, audit_logger.clone(), async {
            shutdown.wait().await;
        })
        .await;
    if http_mode {
        warn_untrusted_read_only_plugins(config, &plugins).await;
    }
    for (name, handler) in plugins {
        server.add_handler(format!("plugin:{}", name), handler);
    }
    startup_failures.extend(plugin_failures);
    startup_failures.extend(config.startup_notes.iter().cloned());

    // mcp_rs_statusツール（#434）に出す情報
    server.set_status_info(mcp_rs::mcp::StatusInfo {
        mode: if http_mode { "http" } else { "stdio" }.to_string(),
        config_file: config.config_file.clone(),
        missing_config_file: config.missing_config_file.clone(),
        started_at: mcp_rs::mcp::process_started_at(),
        plugin_failures: startup_failures,
        // HTTPモードでは問い合わせない（#635）
        check_updates: !http_mode && config.server.check_updates != Some(false),
    });

    Ok(server)
}

/// `@read_only`の制限を書いたのに、`trust_annotations`の無いプラグインが
/// 「読み取りだけ」の印のツールを持つとき、ログに書く（PR #604のレビュー）。
/// v0.27.0から、そのツールは`@read_only`に含めないので、気づかずに
/// ツールが見えなくならないように
async fn warn_untrusted_read_only_plugins(
    config: &McpConfig,
    plugins: &[(String, Arc<mcp_rs::plugin_proxy::PluginHandler>)],
) {
    use mcp_rs::mcp::McpHandler;
    let Some(http) = &config.transport.http else {
        return;
    };
    if !http.uses_tool_preset(mcp_rs::mcp::READ_ONLY_PRESET) {
        return;
    }
    for (name, handler) in plugins {
        if handler.trusts_read_only_hints() {
            continue;
        }
        let marked = handler
            .list_tools()
            .await
            .map(|tools| {
                tools
                    .iter()
                    .filter(|tool| {
                        tool.annotations
                            .as_ref()
                            .and_then(|a| a.read_only_hint)
                            .unwrap_or(false)
                    })
                    .count()
            })
            .unwrap_or(0);
        if marked > 0 {
            tracing::warn!(
                "Plugin '{}' marks {} tool(s) as read-only, but API keys and OAuth scopes limited to {} do not get them, because trust_annotations is not set for this plugin (v0.27.0 and later). After checking its tools, set trust_annotations = true in its [[plugins]] section, or allow them by name",
                name,
                marked,
                mcp_rs::mcp::READ_ONLY_PRESET
            );
        }
    }
}

/// `[database]`のハンドラーを登録する（#453）。失敗の理由を返す
#[cfg(feature = "database")]
async fn register_database(
    server: &mut mcp_rs::mcp::McpServer,
    settings: &mcp_rs::config::DatabaseSettings,
    audit_logger: Arc<mcp_rs::security::AuditLogger>,
) -> Result<(), String> {
    // Redis（#529）は、Redisのツールのハンドラーにする
    let handler = mcp_rs::handlers::database::handler_from_settings(settings, audit_logger).await?;
    server.add_handler("database".to_string(), handler);
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

/// STDIOモードでMCPサーバーを実行（initialize / tools/list / tools/call 等に応答）。
/// 標準入力の終わり（EOF）か、終了の合図（Ctrl+C（#617）・SIGTERM（#759））で
/// 終わる。合図で終わったら`true`
async fn run_stdio_server(
    config: &McpConfig,
    mut shutdown: ShutdownSignal,
) -> Result<bool, Box<dyn std::error::Error>> {
    let server = build_server(config, false, &mut shutdown).await?;
    info!("🔌 STDIOモードで待機中");
    // 標準入出力の失敗（クライアントが去った等）で終わっても、プラグインを
    // 止める（#684）
    run_then_stop(
        server,
        async |server| -> Result<bool, Box<dyn std::error::Error>> {
            let interrupted = tokio::select! {
                // 起動の途中で合図を受けていたら、読み始めずに終わる
                biased;
                // Ctrl+Cを受けないと、Windowsが既定でプロセスを止め、終了コードが
                // 0以外（STATUS_CONTROL_C_EXIT）になる
                signal = shutdown.wait() => {
                    info!("👋 MCP-RS終了（{}）", signal);
                    true
                }
                result = server.run_stdio() => {
                    result?;
                    false
                }
            };
            Ok(interrupted)
        },
        stop_plugins,
    )
    .await
}

/// HTTPモードでMCPサーバーを実行する。`/mcp`（と`/`）へのPOST、および
/// `enable_websocket_upgrade`が有効なら`/ws`のWebSocketで、STDIOと同じ
/// 処理（McpServer）に応答させる（#400）。終了の合図（Ctrl+C・SIGTERM）で
/// 終了する
async fn run_http_server(
    config: &McpConfig,
    http_config: mcp_rs::transport::HttpConfig,
    mut shutdown: ShutdownSignal,
) -> Result<(), Box<dyn std::error::Error>> {
    let server = build_server(config, true, &mut shutdown).await?;

    // TLSを有効にしたら、起動のログのURLもhttps://にする（#750）
    let scheme = if http_config.tls_enabled {
        "https"
    } else {
        "http"
    };
    // HTTPサーバーを始められなかったときも、プラグインを止める（#684）
    run_then_stop(
        server,
        async |server| -> Result<(), Box<dyn std::error::Error>> {
            // 起動の途中で合図を受けていたら、HTTPサーバーを始めずに終わる
            if let Some(signal) = shutdown.received() {
                info!("👋 MCP-RS終了（{}）", signal);
                return Ok(());
            }
            // 認証の失敗も、ツール呼び出しと同じ監査ログに記録する（#55）
            let mut transport = mcp_rs::transport::HttpTransport::new(http_config)?;
            if let Some(audit_logger) = server.audit_logger() {
                transport = transport.with_audit_logger(audit_logger);
            }
            let transport = transport.with_request_handler(Arc::new(server.dispatcher()));
            transport.start_server().await?;
            if let Some(addr) = transport.bound_addr().await {
                info!("🌐 HTTPモードで待機中: {}://{}/mcp", scheme, addr);
            }
            info!("💡 Ctrl+C で終了");

            serve_until_shutdown(&transport, &mut shutdown).await;
            Ok(())
        },
        stop_plugins,
    )
    .await
}

/// 終了の合図を待ち、HTTPの待ち受けを止める。プラグインを止める前に呼ぶ。
/// `transport`を捨てるだけでは止まらず、止めたプラグインに要求が届きうる
/// （#774）
async fn serve_until_shutdown(
    transport: &mcp_rs::transport::HttpTransport,
    shutdown: &mut ShutdownSignal,
) {
    let signal = shutdown.wait().await;
    info!("👋 MCP-RS終了（{}）", signal);
    transport.stop_server().await;
}

/// 終了の合図（Ctrl+C・SIGTERM等）を受けたか（#759）。`listen`で受け始めると、
/// 合図でプロセスが既定のとおり止まらなくなり、待ち始める前に受けた合図も
/// 残る（プラグインの起動中に受けても、起動した後に止められる）
struct ShutdownSignal(tokio::sync::watch::Receiver<Option<&'static str>>);

impl ShutdownSignal {
    /// 合図を今から受ける。登録に失敗した合図は警告し、登録できた合図を待つ
    fn listen() -> Self {
        let (sender, receiver) = tokio::sync::watch::channel(None);
        match signals::register() {
            Ok(mut signals) => {
                tokio::spawn(async move {
                    let signal = signals.recv().await;
                    let _ = sender.send(Some(signal));
                });
            }
            Err(e) => tracing::warn!(
                "Could not listen for shutdown signals ({e}): plugins may be left running if mcp-rs is stopped by a signal"
            ),
        }
        Self(receiver)
    }

    /// 既に受けた合図の名前（まだ受けていなければNone）
    fn received(&self) -> Option<&'static str> {
        *self.0.borrow()
    }

    /// 合図を受けるまで待ち、その名前を返す。既に受けていれば、すぐ返す
    async fn wait(&mut self) -> &'static str {
        let received = self
            .0
            .wait_for(Option::is_some)
            .await
            .ok()
            .and_then(|signal| *signal);
        match received {
            Some(signal) => signal,
            // 合図を受けられない（`listen`で警告済み）
            None => std::future::pending().await,
        }
    }
}

fn optional_shutdown_listener<T>(name: &str, listener: std::io::Result<T>) -> Option<T> {
    listener
        .inspect_err(|e| {
            tracing::warn!(
                "Could not listen for {name} ({e}): plugins may be left running if mcp-rs is stopped by {name}"
            );
        })
        .ok()
}

/// 2つ目の合図（SIGTERM・Ctrl+Break）の登録に失敗しても、1つ目（Ctrl+C）は
/// 受け続ける（#759。登録済みのものを捨てて戻ると、Ctrl+Cでもプロセスが
/// 止まらなくなる）。受けられない合図は、待っても終わらない
async fn recv_optional<F: std::future::Future>(signal: Option<F>) {
    match signal {
        Some(signal) => {
            signal.await;
        }
        None => std::future::pending().await,
    }
}

/// Unix系: SIGINT（Ctrl+C）と、`docker stop`・`systemctl stop`等が送るSIGTERM
#[cfg(unix)]
mod signals {
    use super::optional_shutdown_listener;
    use tokio::signal::unix::{signal, Signal, SignalKind};

    pub(super) struct Signals {
        interrupt: Signal,
        terminate: Option<Signal>,
    }

    pub(super) fn register() -> std::io::Result<Signals> {
        let interrupt = signal(SignalKind::interrupt())?;
        let terminate = optional_shutdown_listener("SIGTERM", signal(SignalKind::terminate()));
        Ok(Signals {
            interrupt,
            terminate,
        })
    }

    impl Signals {
        pub(super) async fn recv(&mut self) -> &'static str {
            tokio::select! {
                _ = self.interrupt.recv() => "Ctrl+C",
                _ = super::recv_optional(self.terminate.as_mut().map(Signal::recv)) => "SIGTERM",
            }
        }
    }
}

/// Windows: Ctrl+CとCtrl+Break
#[cfg(windows)]
mod signals {
    use super::optional_shutdown_listener;
    use tokio::signal::windows::{ctrl_break, ctrl_c, CtrlBreak, CtrlC};

    pub(super) struct Signals {
        ctrl_c: CtrlC,
        ctrl_break: Option<CtrlBreak>,
    }

    pub(super) fn register() -> std::io::Result<Signals> {
        let ctrl_c = ctrl_c()?;
        let ctrl_break = optional_shutdown_listener("Ctrl+Break", ctrl_break());
        Ok(Signals { ctrl_c, ctrl_break })
    }

    impl Signals {
        pub(super) async fn recv(&mut self) -> &'static str {
            tokio::select! {
                _ = self.ctrl_c.recv() => "Ctrl+C",
                _ = super::recv_optional(self.ctrl_break.as_mut().map(CtrlBreak::recv)) => "Ctrl+Break",
            }
        }
    }
}

/// `server`で`run`を実行し、その結果に関わらず`stop`で止めてから、結果を
/// 返す（#684）。`run`がエラーで終わっても、プラグインの後片付けを飛ばさない
async fn run_then_stop<S, T, E>(
    server: S,
    run: impl AsyncFnOnce(&S) -> Result<T, E>,
    stop: impl AsyncFnOnce(S),
) -> Result<T, E> {
    let result = run(&server).await;
    stop(server).await;
    result
}

/// プラグインの子プロセス・コンテナを止め、後片付けの終了を待つ（#673）。
/// HTTPモードでは、ハンドラーをHTTPの処理と共有しているので、`server`の
/// 破棄だけでは止まらない
async fn stop_plugins(server: mcp_rs::mcp::McpServer) {
    server.shutdown().await;
    drop(server);
    wait_for_plugin_cleanups().await;
}

/// コンテナ・Kubernetesのプラグインの後片付け（`docker rm -f`等）の終了を
/// 待つ（PR #672のレビュー）。待たずに終わると、コンテナ等が残ることがある。
/// 後片付けのコマンドが失敗したら、そのコマンドと理由を知らせる（#684）
async fn wait_for_plugin_cleanups() {
    let timeout = std::time::Duration::from_secs(10);
    let report =
        tokio::task::spawn_blocking(move || mcp_rs::plugin_proxy::wait_for_cleanups(timeout))
            .await
            .unwrap_or_else(|_| mcp_rs::plugin_proxy::CleanupReport {
                timed_out: true,
                failures: Vec::new(),
            });
    for failure in &report.failures {
        tracing::warn!(
            "Could not remove a plugin container or Kubernetes job: {}. Run the command again by hand, or check for any left over (docker ps -a --filter label=mcp-rs.plugin / kubectl get jobs,secrets -l mcp-rs.plugin, with the --context and --namespace of the plugin; if the plugin sets docker_command or kubectl_command, such as podman, use that command instead)",
            failure
        );
    }
    if report.timed_out {
        tracing::warn!(
            "Stopping plugin containers or Kubernetes jobs did not finish within {} seconds: remove any left over by hand (docker ps -a --filter label=mcp-rs.plugin / kubectl get jobs,secrets -l mcp-rs.plugin, with the --context and --namespace of the plugin; if the plugin sets docker_command or kubectl_command, such as podman, use that command instead)",
            timeout.as_secs()
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::{Cell, RefCell};

    /// `run`がエラーで終わっても、`stop`を実行してからエラーを返す（#684）
    #[tokio::test]
    async fn test_run_then_stop_stops_after_error() {
        let stopped = Cell::new(false);
        let result: Result<(), String> = run_then_stop(
            "server",
            async |_| Err("broken pipe".to_string()),
            async |_| stopped.set(true),
        )
        .await;
        assert_eq!(result, Err("broken pipe".to_string()));
        assert!(stopped.get());
    }

    /// 待ち始める前に受けた終了の合図も失わない（#759。プラグインの起動中に
    /// 受けた合図）
    #[tokio::test]
    async fn test_shutdown_signal_received_before_waiting_is_kept() {
        let (sender, receiver) = tokio::sync::watch::channel(None);
        let mut shutdown = ShutdownSignal(receiver);
        assert_eq!(shutdown.received(), None);
        sender.send(Some("SIGTERM")).unwrap();
        assert_eq!(shutdown.received(), Some("SIGTERM"));
        assert_eq!(shutdown.wait().await, "SIGTERM");
    }

    /// 登録できなかった2つ目の合図は、待っても終わらない。登録できた合図は
    /// 受けられる（#759。2つ目の登録の失敗で、1つ目を捨てない）
    #[tokio::test]
    async fn test_recv_optional_waits_only_for_registered_signals() {
        let missing: Option<std::future::Ready<()>> = None;
        let waited =
            tokio::time::timeout(std::time::Duration::from_millis(50), recv_optional(missing))
                .await;
        assert!(waited.is_err());
        recv_optional(Some(std::future::ready(()))).await;
    }

    /// 合図を受けたら、HTTPの待ち受けを止めてから戻る（#774。この後に
    /// プラグインを止める）
    #[tokio::test]
    async fn test_serve_until_shutdown_stops_http_server() {
        use mcp_rs::transport::Transport as _;
        let transport = mcp_rs::transport::HttpTransport::new(mcp_rs::transport::HttpConfig {
            bind_addr: "127.0.0.1:0".parse().unwrap(),
            ..Default::default()
        })
        .unwrap();
        transport.start_server().await.unwrap();
        assert!(transport.is_connected());
        let (sender, receiver) = tokio::sync::watch::channel(None);
        let mut shutdown = ShutdownSignal(receiver);
        sender.send(Some("SIGTERM")).unwrap();
        serve_until_shutdown(&transport, &mut shutdown).await;
        assert!(!transport.is_connected());
        assert_eq!(transport.bound_addr().await, None);
    }

    /// 合図を受けられないときは、待っても終わらない（終わったと扱わない）
    #[tokio::test]
    async fn test_shutdown_signal_without_listener_never_fires() {
        let (sender, receiver) = tokio::sync::watch::channel(None);
        drop(sender);
        let mut shutdown = ShutdownSignal(receiver);
        let waited =
            tokio::time::timeout(std::time::Duration::from_millis(50), shutdown.wait()).await;
        assert!(waited.is_err());
    }

    /// `run`が成功したら、`stop`を実行してから結果をそのまま返す
    #[tokio::test]
    async fn test_run_then_stop_returns_result() {
        let order = RefCell::new(Vec::new());
        let result: Result<bool, String> = run_then_stop(
            "server",
            async |server| {
                order.borrow_mut().push(format!("run {}", server));
                Ok(true)
            },
            async |server| order.borrow_mut().push(format!("stop {}", server)),
        )
        .await;
        assert_eq!(result, Ok(true));
        assert_eq!(*order.borrow(), vec!["run server", "stop server"]);
    }
}
