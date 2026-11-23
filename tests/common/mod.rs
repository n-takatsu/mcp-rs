//! サーバーが要る結合テスト（`#[ignore]`）の接続先（#513）。
//!
//! 接続先は環境変数で渡す。ローカルでもCI（`container-integration-tests.yml`）
//! でも同じ方法で実行する。
//!
//! | 変数 | 例 |
//! |---|---|
//! | `TEST_DATABASE_URL` | `postgres://testuser:testpass@localhost:5433/mcptest` |
//! | `TEST_REDIS_URL` | `redis://localhost:6380/0` |
//!
//! `--ignored` で実行したのに変数が無いときは、テストを失敗にする（黙って
//! 飛ばすと、CIで何も確かめていないのに成功に見えるため）。
//! 実行の方法は `docs/development/integration-tests.md` を参照。

#![allow(dead_code)]

pub const POSTGRES_ENV: &str = "TEST_DATABASE_URL";
pub const REDIS_ENV: &str = "TEST_REDIS_URL";

/// URLから取り出した接続先
#[derive(Debug, Clone)]
pub struct ServerUrl {
    pub host: String,
    pub port: u16,
    /// PostgreSQLはデータベース名、RedisはDB番号（省略時は`0`）
    pub database: String,
    pub username: String,
    pub password: String,
}

fn required_env(name: &str, example: &str) -> String {
    match std::env::var(name) {
        Ok(value) if !value.trim().is_empty() => value,
        _ => panic!(
            "{name} is not set. This test needs a running server; set e.g. {name}={example} \
             (see docs/development/integration-tests.md)"
        ),
    }
}

fn parse(name: &str, raw: &str, default_port: u16, default_database: &str) -> ServerUrl {
    let url = url::Url::parse(raw).unwrap_or_else(|e| panic!("{name} is not a valid URL: {e}"));
    // URLの`%40`等を戻す（#724。記号を含むパスワードで、認証に失敗した）
    let decode = |value: &str| {
        urlencoding::decode(value)
            .unwrap_or_else(|e| panic!("{name} has an invalid %-escape: {e}"))
            .into_owned()
    };
    let database = decode(url.path().trim_start_matches('/'));
    ServerUrl {
        host: url.host_str().unwrap_or("localhost").to_string(),
        port: url.port().unwrap_or(default_port),
        database: if database.is_empty() {
            default_database.to_string()
        } else {
            database
        },
        username: decode(url.username()),
        password: decode(url.password().unwrap_or("")),
    }
}

/// `TEST_DATABASE_URL`（PostgreSQL）の値そのもの
pub fn postgres_url() -> String {
    required_env(
        POSTGRES_ENV,
        "postgres://testuser:testpass@localhost:5433/mcptest",
    )
}

/// `TEST_DATABASE_URL`（PostgreSQL）を分解したもの
pub fn postgres() -> ServerUrl {
    parse(POSTGRES_ENV, &postgres_url(), 5432, "postgres")
}

/// `TEST_REDIS_URL`を分解したもの
pub fn redis() -> ServerUrl {
    let raw = required_env(REDIS_ENV, "redis://localhost:6380/0");
    // TLS（`rediss`）は扱えない。黙って暗号化せずにつながない（#724）。
    // 読んだ後のスキーム（小文字にそろう）で確かめる（PR #730のレビュー。
    // `REDISS://`・`rediss:`も断る）
    let scheme = url::Url::parse(&raw)
        .map(|url| url.scheme().to_string())
        .unwrap_or_default();
    assert!(
        scheme != "rediss",
        "{REDIS_ENV} uses rediss (TLS), which these tests do not support; use redis://"
    );
    parse(REDIS_ENV, &raw, 6379, "0")
}
