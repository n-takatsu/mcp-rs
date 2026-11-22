//! MySQLのエンジン（`[database]`の`engine = "mysql"`、#527）。MariaDB
//! （`engine = "mariadb"`）も、このエンジンを使う（`MariaDbEngine`）
//!
//! 接続のプールは、最初の`connect`で1つだけ作り、以後の接続で共有する
//! （以前は接続のたびにプールを作っていた）。接続の設定は、URLを組み立てずに
//! 項目ごとに渡すので、パスワードが記号を含んでいてもよい。

use super::{connection::ServerKind, MySqlConnection};
use crate::handlers::database::{
    engine::{DatabaseConnection, DatabaseEngine},
    engines::{OPTION_BYTE_LIMIT, OPTION_FETCH_LIMIT, OPTION_SSL_ROOT_CERT},
    types::{
        ConnectionConfig, DatabaseConfig, DatabaseError, DatabaseFeature, DatabaseType,
        HealthStatus,
    },
};
use async_trait::async_trait;
use mysql_async::{
    prelude::Queryable, Opts, OptsBuilder, Pool, PoolConstraints, PoolOpts, SslOpts,
};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

/// `ssl_mode`の値（PostgreSQLと同じ綴り。`allow`は無い）
pub const SSL_MODES: &[&str] = &["disable", "prefer", "require", "verify-ca", "verify-full"];

/// MySQL Database Engine
#[derive(Clone)]
pub struct MySqlEngine {
    config: DatabaseConfig,
    /// 接続のプールと、その設定と、つないだサーバーの種類。最初の
    /// `connect`で作る
    pool: Arc<tokio::sync::OnceCell<(Pool, Opts, ServerKind)>>,
}

impl MySqlEngine {
    /// Create new MySQL engine
    ///
    /// The engine does not check queries itself. It used to take a
    /// `DatabaseSecurity` that it never used, and had a `new_without_security`
    /// that behaved the same (#499).
    pub async fn new(config: DatabaseConfig) -> Result<Self, DatabaseError> {
        Ok(Self {
            config,
            pool: Arc::new(tokio::sync::OnceCell::new()),
        })
    }

    /// Get database config
    pub fn get_config(&self) -> &DatabaseConfig {
        &self.config
    }
}

/// `ssl_mode`の値（省略時は`prefer`）
fn ssl_mode(config: &ConnectionConfig) -> &str {
    config.ssl_mode.as_deref().unwrap_or("prefer")
}

/// `ssl_mode`から、TLSの設定を作る。`None`はTLSを使わない
///
/// - `prefer`・`require`: 暗号化するが、証明書は確かめない（`prefer`は、
///   サーバーがTLSを使えなければ暗号化せずにつなぐ）
/// - `verify-ca`: 証明書を確かめるが、ホスト名は確かめない。`root_cert`が
///   要る（#594）。公開のCAで確かめると、ホスト名を見ないので、公開のCAが
///   発行したどのホストの証明書でも受け入れてしまう
/// - `verify-full`: 証明書とホスト名を確かめる
///
/// `root_cert`は、証明書を確かめるCAのファイル（PEM）。書いたときは、その
/// CAだけを信頼する（PostgreSQLと同じ）。無ければ、公開のCA（Mozillaの一覧）
pub fn ssl_opts(mode: &str, root_cert: Option<&str>) -> Result<Option<SslOpts>, DatabaseError> {
    let with_root = |opts: SslOpts| match root_cert {
        Some(path) => opts
            .with_root_certs(vec![PathBuf::from(path).into()])
            .with_disable_built_in_roots(true),
        None => opts,
    };
    match mode {
        "disable" => Ok(None),
        "prefer" | "require" => Ok(Some(
            SslOpts::default()
                .with_danger_accept_invalid_certs(true)
                .with_danger_skip_domain_validation(true),
        )),
        "verify-ca" if root_cert.is_none() => Err(DatabaseError::ConfigurationError(
            VERIFY_CA_NEEDS_ROOT_CERT.to_string(),
        )),
        "verify-ca" => Ok(Some(
            with_root(SslOpts::default()).with_danger_skip_domain_validation(true),
        )),
        "verify-full" => Ok(Some(with_root(SslOpts::default()))),
        other => Err(DatabaseError::ConfigurationError(format!(
            "invalid ssl_mode '{}' for MySQL (use one of: {})",
            other,
            SSL_MODES.join(", ")
        ))),
    }
}

/// 接続のセッションの`sql_mode`から外すモード（#574）。
///
/// mcp-rsのSQLの検査（sqlparserの`MySqlDialect`）は、文字列の中の`\'`を
/// 引用符のエスケープと読み、`"…"`を文字列と読む。サーバーの読み方が違うと、
/// 検査では文字列やコメントに見えるところを、サーバーはSQLとして実行する。
///
/// - `NO_BACKSLASH_ESCAPES`: `'\'`で文字列が終わる。
///   `SELECT '\' /*M! INTO OUTFILE '/tmp/x' */ -- '`が通ってしまう
/// - `ANSI_QUOTES`: `"…"`が識別子になり、中の`\`はエスケープにならない。
///   `SELECT 1 AS "a\" INTO OUTFILE '/tmp/x' -- "`が通ってしまう（PR #585の
///   レビュー）
/// - `ANSI_QUOTES`を含む、まとめたモード（MySQL・MariaDBの`ANSI`、MariaDBの
///   `DB2`・`MSSQL`・`ORACLE`・`POSTGRESQL`・`MAXDB`）。`@@sql_mode`には、
///   中身のモードと一緒に、まとめた名前も残る。名前を残すと、設定し直した
///   ときに`ANSI_QUOTES`がまた入る。中身のほかのモード（`PIPES_AS_CONCAT`等）は
///   残る
pub const STRIPPED_SQL_MODES: &[&str] = &[
    "NO_BACKSLASH_ESCAPES",
    "ANSI_QUOTES",
    "ANSI",
    "DB2",
    "MSSQL",
    "ORACLE",
    "POSTGRESQL",
    "MAXDB",
];

/// 接続を作ったとき・プールに戻してリセットしたときに、毎回実行するSQL（#574）。
/// セッションの`sql_mode`から`STRIPPED_SQL_MODES`を外し、サーバーの読み方を
/// 検査とそろえる。ほかのモードは変えない。前後にカンマを付けてから
/// `,<モード>,`を置き換えるので、先頭・途中・末尾のどこにあっても外れる
pub fn session_setup_sql() -> String {
    let stripped = STRIPPED_SQL_MODES.iter().fold(
        "CONCAT(',', @@SESSION.sql_mode, ',')".to_string(),
        |expr, mode| format!("REPLACE({expr}, ',{mode},', ',')"),
    );
    format!("SET SESSION sql_mode = TRIM(BOTH ',' FROM {stripped})")
}

/// 設定から、接続の設定を作る。`tls`は`ssl_opts`の結果
pub fn connect_opts(config: &DatabaseConfig, tls: Option<SslOpts>) -> Opts {
    let connection = &config.connection;
    let constraints =
        PoolConstraints::new(0, config.pool.max_connections.max(1) as usize).unwrap_or_default();
    let builder = OptsBuilder::default()
        .ip_or_hostname(connection.host.clone())
        .tcp_port(connection.port)
        .user(Some(connection.username.clone()))
        .pass((!connection.password.is_empty()).then(|| connection.password.clone()))
        .db_name(Some(connection.database.clone()))
        // localhostでも、書いたとおりTCPでつなぐ（Unixソケットに切り替えない）
        .prefer_socket(false)
        // 新しい接続と、リセットした接続で、毎回実行する（#574）
        .setup(vec![session_setup_sql()])
        .pool_opts(
            PoolOpts::default()
                .with_constraints(constraints)
                // プールに戻す接続のセッション（変数・ロック・一時表）を戻す
                .with_reset_connection(true)
                .with_inactive_connection_ttl(Duration::from_secs(u64::from(
                    config.pool.idle_timeout.max(1),
                ))),
        )
        .ssl_opts(tls);
    Opts::from(builder)
}

/// TLSが、通信の盗み見・なりすましを防げない設定のとき、その理由を返す。
/// ループバック（`localhost`・`127.0.0.1`・`::1`）では返さない
/// （PostgreSQLの`weak_tls_warning`と同じ考え方）
pub fn weak_tls_warning(config: &ConnectionConfig) -> Option<String> {
    let host = config.host.as_str();
    let loopback = host.eq_ignore_ascii_case("localhost")
        || host
            .trim_start_matches('[')
            .trim_end_matches(']')
            .parse::<std::net::IpAddr>()
            .is_ok_and(|ip| ip.is_loopback());
    if loopback {
        return None;
    }
    let mode = ssl_mode(config);
    let risk = match mode {
        "verify-full" => return None,
        "verify-ca" => "the server's host name is not checked",
        "require" => "the server's certificate is not checked",
        "prefer" => {
            "the connection may not be encrypted, and the server's certificate is not checked"
        }
        _ => "the connection is not encrypted",
    };
    Some(format!(
        "database ssl_mode is \"{mode}\" for {host}: {risk}. Use ssl_mode = \"verify-full\" \
         (with ssl_root_cert for a private CA)"
    ))
}

/// `ssl_root_cert`（証明書を確かめるCAのファイル）
fn ssl_root_cert(connection: &ConnectionConfig) -> Option<&str> {
    connection
        .options
        .get(OPTION_SSL_ROOT_CERT)
        .map(String::as_str)
}

/// `verify-ca`で`ssl_root_cert`が無いときのエラー（#594）
const VERIFY_CA_NEEDS_ROOT_CERT: &str = "database.ssl_mode = \"verify-ca\" needs ssl_root_cert \
     (the CA that signed the server certificate): verify-ca does not check the host name, so \
     with the public CAs it would accept a certificate for any host. Set ssl_root_cert, or use \
     ssl_mode = \"verify-full\"";

/// `ssl_root_cert`を書いたのに、`ssl_mode`が証明書を確かめない設定なら、
/// 起動しない理由を返す（PostgreSQLと同じ）。`verify-ca`で`ssl_root_cert`が
/// 無いときも、起動しない（#594）
pub fn check_ssl_root_cert(config: &ConnectionConfig) -> Result<(), String> {
    let mode = ssl_mode(config);
    if mode == "verify-ca" && !config.options.contains_key(OPTION_SSL_ROOT_CERT) {
        return Err(VERIFY_CA_NEEDS_ROOT_CERT.to_string());
    }
    if !config.options.contains_key(OPTION_SSL_ROOT_CERT)
        || matches!(mode, "verify-ca" | "verify-full")
    {
        return Ok(());
    }
    Err(format!(
        "database.ssl_root_cert is set, but ssl_mode is \"{mode}\", which does not check the \
         server certificate, so the CA file would not be used. Set ssl_mode = \"verify-full\" \
         (or \"verify-ca\") in [database]"
    ))
}

/// プールを作り、つながるかを確かめる。`prefer`で、サーバーがTLSを使えない
/// ときは、暗号化せずにつなぎ直す。サーバーの版から、MySQLかMariaDBかを
/// 決める（`engine`の書き方ではなく、実際のサーバーに合わせる）
async fn connect_pool(config: &DatabaseConfig) -> Result<(Pool, Opts, ServerKind), DatabaseError> {
    // rustlsの暗号の実装（プロバイダー）を決める。依存クレートの都合で、
    // ringとaws-lc-rsの両方が入っているので、決めないとTLSの接続でパニック
    // する（CIのtls_modesで分かった）。HTTPのサーバー（`build_tls_acceptor`）
    // と同じringにする。STDIOモードでは、HTTPのサーバーが決めないので、
    // ここで決める。既に決まっていれば何もしない
    let _ = rustls::crypto::ring::default_provider().install_default();
    let connection = &config.connection;
    // CAとTLSモードの組み合わせを、プールを作る前に確かめる（#717。ライブラリ
    // として直接つなぐ場合も。`require`とCAを書くと、CAを使わずにつないでいた）
    check_ssl_root_cert(connection).map_err(DatabaseError::ConfigurationError)?;
    let mode = ssl_mode(connection);
    let root_cert = ssl_root_cert(connection);
    let timeout = Duration::from_secs(u64::from(connection.timeout_seconds).max(1));
    let try_connect = |opts: Opts| async move {
        let pool = Pool::new(opts.clone());
        let conn = tokio::time::timeout(timeout, pool.get_conn())
            .await
            .map_err(|_| {
                mysql_async::Error::Other(
                    format!("no connection within {} seconds", timeout.as_secs()).into(),
                )
            })?;
        let mut conn = conn?;
        let version: Option<String> = conn.query_first("SELECT VERSION()").await?;
        drop(conn);
        let server = ServerKind::from_version(version.as_deref().unwrap_or_default());
        Ok::<_, mysql_async::Error>((pool, opts, server))
    };
    let opts = connect_opts(config, ssl_opts(mode, root_cert)?);
    match try_connect(opts).await {
        Ok(connected) => Ok(connected),
        Err(mysql_async::Error::Driver(mysql_async::DriverError::NoClientSslFlagFromServer))
            if mode == "prefer" =>
        {
            tracing::info!(
                "The database server at {} does not support TLS: connecting without encryption (ssl_mode = \"prefer\")",
                connection.host
            );
            try_connect(connect_opts(config, None))
                .await
                .map_err(|e| DatabaseError::ConnectionFailed(e.to_string()))
        }
        Err(e) => Err(DatabaseError::ConnectionFailed(e.to_string())),
    }
}

#[async_trait]
impl DatabaseEngine for MySqlEngine {
    fn engine_type(&self) -> DatabaseType {
        DatabaseType::MySQL
    }

    async fn connect(
        &self,
        config: &DatabaseConfig,
    ) -> Result<Box<dyn DatabaseConnection>, DatabaseError> {
        let (pool, opts, server) = self
            .pool
            .get_or_try_init(|| connect_pool(config))
            .await?
            .clone();
        let limit = |key: &str| {
            config
                .connection
                .options
                .get(key)
                .and_then(|v| v.parse::<usize>().ok())
        };
        let conn = MySqlConnection::with_pool(
            pool,
            opts,
            config.connection.database.clone(),
            config.connection.username.clone(),
        )
        .with_statement_timeout(Duration::from_secs(u64::from(
            config.features.query_timeout,
        )))
        .with_fetch_limit(limit(OPTION_FETCH_LIMIT))
        .with_byte_limit(limit(OPTION_BYTE_LIMIT))
        .with_server(server)
        .with_connect_timeout(Duration::from_secs(u64::from(
            config.connection.timeout_seconds,
        )));
        Ok(Box::new(conn))
    }

    async fn health_check(&self) -> Result<HealthStatus, DatabaseError> {
        use crate::handlers::database::types::HealthStatusType;
        use chrono::Utc;

        Ok(HealthStatus {
            status: HealthStatusType::Healthy,
            last_check: Utc::now(),
            response_time_ms: 0,
            error_message: None,
            connection_count: 1,
            active_transactions: 0,
        })
    }

    fn supported_features(&self) -> Vec<DatabaseFeature> {
        vec![
            DatabaseFeature::Transactions,
            DatabaseFeature::StoredProcedures,
            DatabaseFeature::PreparedStatements,
        ]
    }

    fn validate_config(&self, config: &DatabaseConfig) -> Result<(), DatabaseError> {
        // CAとTLSモードの組み合わせも、接続と同じく確かめる（#717）
        check_ssl_root_cert(&config.connection).map_err(DatabaseError::ConfigurationError)?;
        // 接続と同じCAを渡す（#705。以前は`None`を渡したので、CAを書いた
        // `verify-ca`も断った）
        ssl_opts(
            ssl_mode(&config.connection),
            ssl_root_cert(&config.connection),
        )
        .map(|_| ())
    }

    async fn get_version(&self) -> Result<String, DatabaseError> {
        Ok("MySQL".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn connection(host: &str, mode: Option<&str>, root_cert: bool) -> ConnectionConfig {
        let mut options = HashMap::new();
        if root_cert {
            options.insert(OPTION_SSL_ROOT_CERT.to_string(), "ca.pem".to_string());
        }
        ConnectionConfig {
            host: host.to_string(),
            port: 3306,
            database: "db".to_string(),
            username: "reader".to_string(),
            password: "p@ss:w/rd?".to_string(),
            ssl_mode: mode.map(str::to_string),
            timeout_seconds: 10,
            retry_attempts: 0,
            options,
        }
    }

    #[test]
    fn test_ssl_opts() {
        assert!(ssl_opts("disable", None).unwrap().is_none());
        let require = ssl_opts("require", None).unwrap().unwrap();
        assert!(require.accept_invalid_certs());
        assert!(require.skip_domain_validation());
        let verify_ca = ssl_opts("verify-ca", Some("ca.pem")).unwrap().unwrap();
        assert!(!verify_ca.accept_invalid_certs());
        assert!(verify_ca.skip_domain_validation());
        assert_eq!(verify_ca.root_certs().len(), 1);
        // CAのファイルを書いたら、そのCAだけを信頼する
        assert!(verify_ca.disable_built_in_roots());
        // ssl_root_cert無しのverify-caは、どのホストの公開の証明書でも
        // 受け入れるので、使えない（#594）
        assert!(ssl_opts("verify-ca", None).is_err());
        let verify_full = ssl_opts("verify-full", None).unwrap().unwrap();
        assert!(!verify_full.accept_invalid_certs());
        assert!(!verify_full.skip_domain_validation());
        assert!(!verify_full.disable_built_in_roots());
        // PostgreSQLの`allow`は、MySQLには無い
        assert!(ssl_opts("allow", None).is_err());
    }

    /// `validate_config`は、接続と同じCAで確かめる（#705。以前はCAを渡さず、
    /// CAを書いた`verify-ca`も断った）
    #[tokio::test]
    async fn test_validate_config_uses_the_root_cert() {
        let engine = MySqlEngine::new(DatabaseConfig::default()).await.unwrap();
        let config = |root_cert: bool| DatabaseConfig {
            database_type: DatabaseType::MySQL,
            connection: connection("db.example.com", Some("verify-ca"), root_cert),
            ..DatabaseConfig::default()
        };
        assert!(engine.validate_config(&config(true)).is_ok());
        assert!(engine.validate_config(&config(false)).is_err());
        // CAを書いた`require`（CAを使わない）は、確かめて断る（#717）
        let require_with_ca = DatabaseConfig {
            database_type: DatabaseType::MySQL,
            connection: connection("db.example.com", Some("require"), true),
            ..DatabaseConfig::default()
        };
        assert!(engine.validate_config(&require_with_ca).is_err());
    }

    /// ライブラリとして直接つなぐときも、CAとTLSモードの組み合わせを、
    /// つなぐ前に確かめる（#717）
    #[tokio::test]
    async fn test_connect_checks_the_root_cert_before_connecting() {
        let config = DatabaseConfig {
            database_type: DatabaseType::MySQL,
            connection: connection("db.example.invalid", Some("require"), true),
            ..DatabaseConfig::default()
        };
        let engine = MySqlEngine::new(config.clone()).await.unwrap();
        let error = engine.connect(&config).await.err().unwrap();
        assert!(
            matches!(error, DatabaseError::ConfigurationError(_)),
            "{error:?}"
        );
    }

    #[test]
    fn test_connect_opts_keep_the_password_as_written() {
        let config = DatabaseConfig {
            database_type: DatabaseType::MySQL,
            connection: connection("db.example.com", None, false),
            ..DatabaseConfig::default()
        };
        let opts = connect_opts(&config, None);
        // URLを組み立てないので、記号を含むパスワードもそのまま
        assert_eq!(opts.pass(), Some("p@ss:w/rd?"));
        assert_eq!(opts.ip_or_hostname(), "db.example.com");
        assert_eq!(opts.tcp_port(), 3306);
        assert!(!opts.prefer_socket());
        assert!(opts.pool_opts().reset_connection());
        // 新しい接続・リセットした接続で、検査と読み方が違うモードを外す（#574）
        assert_eq!(opts.setup(), [session_setup_sql()]);
    }

    /// 外すモードを、カンマで区切った名前として置き換える（`ANSI`で
    /// `ANSI_QUOTES`の一部を消さない）
    #[test]
    fn test_session_setup_sql() {
        let sql = session_setup_sql();
        assert!(sql.starts_with("SET SESSION sql_mode = TRIM(BOTH ',' FROM "));
        for mode in STRIPPED_SQL_MODES {
            assert!(sql.contains(&format!("',{mode},', ','")), "{mode}: {sql}");
        }
        assert!(sql.contains("CONCAT(',', @@SESSION.sql_mode, ',')"));
    }

    #[test]
    fn test_weak_tls_warning() {
        assert!(weak_tls_warning(&connection("localhost", Some("disable"), false)).is_none());
        assert!(weak_tls_warning(&connection("127.0.0.1", None, false)).is_none());
        assert!(weak_tls_warning(&connection("db", Some("verify-full"), false)).is_none());
        let prefer = weak_tls_warning(&connection("db", None, false)).unwrap();
        assert!(prefer.contains("\"prefer\""), "{prefer}");
        let verify_ca = weak_tls_warning(&connection("db", Some("verify-ca"), false)).unwrap();
        assert!(verify_ca.contains("host name"), "{verify_ca}");
    }

    #[test]
    fn test_check_ssl_root_cert() {
        assert!(check_ssl_root_cert(&connection("db", Some("require"), false)).is_ok());
        assert!(check_ssl_root_cert(&connection("db", Some("verify-full"), true)).is_ok());
        assert!(check_ssl_root_cert(&connection("db", Some("verify-ca"), true)).is_ok());
        // verify-caは、ssl_root_certが要る（#594）
        let error = check_ssl_root_cert(&connection("db", Some("verify-ca"), false)).unwrap_err();
        assert!(error.contains("needs ssl_root_cert"), "{error}");
        let error = check_ssl_root_cert(&connection("db", Some("require"), true)).unwrap_err();
        assert!(error.contains("would not be used"), "{error}");
        // 省略時は`prefer`で、CAのファイルを使わない
        assert!(check_ssl_root_cert(&connection("db", None, true)).is_err());
    }
}
