//! 新しい版があるかを確かめる（#635）
//!
//! mcp-rsは、インストールスクリプト・Scoop・Homebrew・手で置いたファイルで
//! 入れられる（#576・#578）。どれも、新しい版が出たことを利用者に知らせる
//! 仕組みが無く、セキュリティの修正が届きにくい。`mcp-rs check`と、STDIO
//! モードの`mcp_rs_status`で、GitHubのリリースの最新の版と比べ、入れ方に
//! 合わせた更新のコマンドを示す。
//!
//! GitHubには、版の番号を読むためだけに問い合わせる。送るのは、mcp-rsの
//! 版を含む`User-Agent`だけ。設定の`[server] check_updates = false`で
//! 止められる。HTTPモードの`mcp_rs_status`では問い合わせない。

use std::path::Path;
use std::time::Duration;

/// 最新のリリースを返すGitHubのAPI
const LATEST_RELEASE_URL: &str = "https://api.github.com/repos/n-takatsu/mcp-rs/releases/latest";

/// 問い合わせの時間の上限
const TIMEOUT: Duration = Duration::from_secs(5);

/// mcp-rsの入れ方。実行ファイルの場所から判断する
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstallMethod {
    /// Scoop（`<scoop>\apps\mcp-rs\…`）
    Scoop,
    /// Homebrew（`<prefix>/Cellar/mcp-rs/…`・`<prefix>/opt/mcp-rs/…`）
    Homebrew,
    /// インストールスクリプト（`~/.mcp-rs/bin`）
    InstallScript,
    /// それ以外（手で置いた・ソースからビルドした等）
    Other,
}

impl InstallMethod {
    /// 実行ファイルのパスから判断する
    pub fn detect(exe: &Path) -> Self {
        let path = exe.to_string_lossy().replace('\\', "/").to_lowercase();
        if path.contains("/scoop/apps/mcp-rs/") {
            InstallMethod::Scoop
        } else if path.contains("/cellar/mcp-rs/")
            || (path.contains("/opt/mcp-rs/")
                && (path.contains("/homebrew/") || path.contains("/linuxbrew/")))
        {
            InstallMethod::Homebrew
        } else if path.contains("/.mcp-rs/bin/") {
            InstallMethod::InstallScript
        } else {
            InstallMethod::Other
        }
    }

    /// 今の実行ファイルの入れ方
    pub fn current() -> Self {
        std::env::current_exe()
            .map(|exe| Self::detect(&exe))
            .unwrap_or(InstallMethod::Other)
    }

    /// 更新のしかた
    pub fn update_hint(self) -> &'static str {
        match self {
            InstallMethod::Scoop => "scoop update mcp-rs",
            InstallMethod::Homebrew => "brew upgrade mcp-rs",
            InstallMethod::InstallScript => {
                if cfg!(windows) {
                    "run the install script again: irm https://raw.githubusercontent.com/n-takatsu/mcp-rs/main/install/install.ps1 | iex"
                } else {
                    "run the install script again: curl -fsSL https://raw.githubusercontent.com/n-takatsu/mcp-rs/main/install/install.sh | sh"
                }
            }
            InstallMethod::Other => {
                "download it from https://github.com/n-takatsu/mcp-rs/releases/latest"
            }
        }
    }
}

/// 確かめた結果
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateStatus {
    /// 今の版が最新（か、それより新しい）
    UpToDate { latest: String },
    /// 新しい版がある
    Available { latest: String },
    /// 確かめられなかった（理由）
    Unknown(String),
}

impl UpdateStatus {
    /// 今の版`current`と、最新のリリースのタグ`tag`（`v0.29.0`）を比べる
    pub fn compare(current: &str, tag: &str) -> Self {
        let latest = tag.trim().trim_start_matches('v').to_string();
        match (parse_version(current), parse_version(&latest)) {
            (Some(current), Some(newest)) if newest > current => UpdateStatus::Available { latest },
            (Some(_), Some(_)) => UpdateStatus::UpToDate { latest },
            _ => UpdateStatus::Unknown(format!("the latest release tag '{tag}' is not a version")),
        }
    }

    /// 1行の説明（`mcp_rs_status`・`mcp-rs check`に出す）
    pub fn describe(&self, method: InstallMethod) -> String {
        let current = env!("CARGO_PKG_VERSION");
        match self {
            UpdateStatus::UpToDate { latest } if latest == current => {
                format!("{current} is the latest release")
            }
            UpdateStatus::UpToDate { latest } => {
                format!("{current} (the latest release is {latest})")
            }
            UpdateStatus::Available { latest } => format!(
                "mcp-rs {latest} is available (this is {current}). To update, {}, then restart the MCP clients",
                method.update_hint()
            ),
            UpdateStatus::Unknown(reason) => format!("could not check for a new version: {reason}"),
        }
    }
}

/// `major.minor.patch`（後ろの`-…`・`+…`は見ない）
fn parse_version(version: &str) -> Option<(u64, u64, u64)> {
    let core = version.trim().split(['-', '+']).next()?;
    let mut parts = core.split('.').map(|part| part.parse::<u64>().ok());
    let version = (parts.next()??, parts.next()??, parts.next()??);
    parts.next().is_none().then_some(version)
}

/// GitHubに、最新のリリースを問い合わせる
pub async fn check() -> UpdateStatus {
    check_at(LATEST_RELEASE_URL).await
}

/// GitHubの応答の本文の上限
const MAX_BODY_BYTES: usize = 1024 * 1024;

/// `url`（テストでは手元のサーバー）に問い合わせる
async fn check_at(url: &str) -> UpdateStatus {
    let client = match reqwest::Client::builder().timeout(TIMEOUT).build() {
        Ok(client) => client,
        Err(e) => return UpdateStatus::Unknown(e.to_string()),
    };
    let response = client
        .get(url)
        .header(
            reqwest::header::USER_AGENT,
            concat!("mcp-rs/", env!("CARGO_PKG_VERSION")),
        )
        .header(reqwest::header::ACCEPT, "application/vnd.github+json")
        .send()
        .await;
    let response = match response {
        Ok(response) if response.status().is_success() => response,
        Ok(response) => {
            return UpdateStatus::Unknown(format!("GitHub answered {}", response.status()))
        }
        Err(e) if e.is_timeout() => {
            return UpdateStatus::Unknown(format!(
                "GitHub did not answer within {} seconds",
                TIMEOUT.as_secs()
            ))
        }
        Err(_) => return UpdateStatus::Unknown("could not reach GitHub".to_string()),
    };
    // 応答は小さい（数KB）が、大きすぎる応答は読まない。全部を読んでから
    // 大きさを確かめるのではなく、読みながら上限で止める（#692）
    let too_large = || UpdateStatus::Unknown("the answer from GitHub is too large".to_string());
    if response
        .content_length()
        .is_some_and(|length| length > MAX_BODY_BYTES as u64)
    {
        return too_large();
    }
    let mut response = response;
    let mut body = Vec::new();
    loop {
        match response.chunk().await {
            Ok(Some(chunk)) => {
                if body.len() + chunk.len() > MAX_BODY_BYTES {
                    return too_large();
                }
                body.extend_from_slice(&chunk);
            }
            Ok(None) => break,
            Err(_) => {
                return UpdateStatus::Unknown("could not read the answer from GitHub".to_string())
            }
        }
    }
    let tag = serde_json::from_slice::<serde_json::Value>(&body)
        .ok()
        .and_then(|value| value.get("tag_name")?.as_str().map(str::to_string));
    match tag {
        Some(tag) => UpdateStatus::compare(env!("CARGO_PKG_VERSION"), &tag),
        None => UpdateStatus::Unknown("the answer from GitHub has no release tag".to_string()),
    }
}

/// プロセスで最初の1回だけ問い合わせ、結果を覚えておく（`mcp_rs_status`用）
pub async fn check_once() -> UpdateStatus {
    static RESULT: tokio::sync::OnceCell<UpdateStatus> = tokio::sync::OnceCell::const_new();
    RESULT.get_or_init(check).await.clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_install_method_detect() {
        for (path, method) in [
            (
                r"C:\Users\me\scoop\apps\mcp-rs\current\mcp-rs.exe",
                InstallMethod::Scoop,
            ),
            (
                "/opt/homebrew/Cellar/mcp-rs/0.29.0/bin/mcp-rs",
                InstallMethod::Homebrew,
            ),
            (
                "/opt/homebrew/opt/mcp-rs/bin/mcp-rs",
                InstallMethod::Homebrew,
            ),
            (
                "/home/linuxbrew/.linuxbrew/opt/mcp-rs/bin/mcp-rs",
                InstallMethod::Homebrew,
            ),
            (
                r"C:\Users\me\.mcp-rs\bin\mcp-rs.exe",
                InstallMethod::InstallScript,
            ),
            ("/home/me/.mcp-rs/bin/mcp-rs", InstallMethod::InstallScript),
            ("/opt/mcp-rs/bin/mcp-rs", InstallMethod::Other),
            (
                r"C:\Users\me\GitHub\mcp-rs\target\release\mcp-rs.exe",
                InstallMethod::Other,
            ),
        ] {
            assert_eq!(InstallMethod::detect(Path::new(path)), method, "{path}");
        }
    }

    #[test]
    fn test_compare_versions() {
        assert_eq!(
            UpdateStatus::compare("0.29.0", "v0.29.1"),
            UpdateStatus::Available {
                latest: "0.29.1".to_string()
            }
        );
        assert_eq!(
            UpdateStatus::compare("0.29.0", "v0.30.0"),
            UpdateStatus::Available {
                latest: "0.30.0".to_string()
            }
        );
        // 文字列ではなく、数で比べる
        assert_eq!(
            UpdateStatus::compare("0.9.0", "v0.10.0"),
            UpdateStatus::Available {
                latest: "0.10.0".to_string()
            }
        );
        assert_eq!(
            UpdateStatus::compare("0.29.0", "v0.29.0"),
            UpdateStatus::UpToDate {
                latest: "0.29.0".to_string()
            }
        );
        // ソースからビルドした新しい版
        assert_eq!(
            UpdateStatus::compare("0.30.0", "v0.29.0"),
            UpdateStatus::UpToDate {
                latest: "0.29.0".to_string()
            }
        );
        assert!(matches!(
            UpdateStatus::compare("0.29.0", "latest"),
            UpdateStatus::Unknown(_)
        ));
        assert_eq!(parse_version("1.2.3-rc.1"), Some((1, 2, 3)));
        assert_eq!(parse_version("1.2"), None);
        assert_eq!(parse_version("1.2.3.4"), None);
    }

    #[test]
    fn test_describe() {
        let available = UpdateStatus::Available {
            latest: "99.0.0".to_string(),
        };
        let text = available.describe(InstallMethod::Scoop);
        assert!(text.starts_with("mcp-rs 99.0.0 is available"), "{text}");
        assert!(text.contains("scoop update mcp-rs"), "{text}");
        assert!(available
            .describe(InstallMethod::Homebrew)
            .contains("brew upgrade mcp-rs"));
        let current = env!("CARGO_PKG_VERSION").to_string();
        assert_eq!(
            UpdateStatus::UpToDate {
                latest: current.clone()
            }
            .describe(InstallMethod::Other),
            format!("{current} is the latest release")
        );
    }

    /// 手元のサーバーを、GitHubの代わりにする
    async fn serve_once(status: &'static str, body: String) -> String {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            if let Ok((mut socket, _)) = listener.accept().await {
                let mut buf = [0u8; 4096];
                let _ = socket.read(&mut buf).await;
                let response = format!(
                    "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = socket.write_all(response.as_bytes()).await;
            }
        });
        format!("http://{addr}/repos/n-takatsu/mcp-rs/releases/latest")
    }

    #[tokio::test]
    async fn test_check_at() {
        let url = serve_once("200 OK", r#"{"tag_name":"v99.0.0","name":"x"}"#.to_string()).await;
        assert_eq!(
            check_at(&url).await,
            UpdateStatus::Available {
                latest: "99.0.0".to_string()
            }
        );
        let url = serve_once("403 Forbidden", "{}".to_string()).await;
        assert!(matches!(check_at(&url).await, UpdateStatus::Unknown(r) if r.contains("403")));
        let url = serve_once("200 OK", r#"{"message":"x"}"#.to_string()).await;
        assert!(
            matches!(check_at(&url).await, UpdateStatus::Unknown(r) if r.contains("no release tag"))
        );
        // つながらない
        assert!(matches!(
            check_at("http://127.0.0.1:1/").await,
            UpdateStatus::Unknown(_)
        ));
    }

    /// 長さを言わずに送り続ける応答も、上限まで読んで止める（全部を読み
    /// 込まない。#692）
    #[tokio::test]
    async fn test_check_at_stops_reading_a_large_answer() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            if let Ok((mut socket, _)) = listener.accept().await {
                let mut buf = [0u8; 4096];
                let _ = socket.read(&mut buf).await;
                let _ = socket
                    .write_all(
                        b"HTTP/1.1 200 OK
Content-Type: application/json
Connection: close

",
                    )
                    .await;
                // 相手が読むのをやめる（つながりを切る）まで送り続ける
                let chunk = vec![b' '; 64 * 1024];
                while socket.write_all(&chunk).await.is_ok() {}
            }
        });
        let url = format!("http://{addr}/");
        let status = tokio::time::timeout(TIMEOUT / 2, check_at(&url))
            .await
            .expect("check_at did not stop reading");
        assert!(
            matches!(&status, UpdateStatus::Unknown(r) if r.contains("too large")),
            "{status:?}"
        );
    }
}
