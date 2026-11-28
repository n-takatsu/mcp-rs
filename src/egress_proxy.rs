//! プラグインのコンテナの外への通信を、許可した行き先にだけ中継する
//! プロキシ（#615、`mcp-rs egress-proxy`）。
//!
//! `[plugins.container]`の`network = "allowlist"`で、mcp-rsは、外に出られない
//! 内部のネットワーク（`docker network create --internal`）にプラグインの
//! コンテナを入れ、このプロキシを、そのネットワークと外に出られる
//! ネットワークの両方につないだコンテナ（mcp-rsのDockerイメージ）で動かす。
//! プラグインには`HTTPS_PROXY`等でプロキシを教える。
//!
//! - `CONNECT host:port`（HTTPSのプロキシの方式）だけを中継する。外側の平文の
//!   HTTP要求（`GET http://...`）は断る。つないだ後は、許可した行き先への
//!   TCPのトンネルとしてバイト列をそのまま渡す（中身は復号も確認もしない。
//!   許可したポートへなら、TLSでない通信も届く。守るのは行き先の名前とポート）
//! - 行き先は、`--allow host:port`の名前とポートで決める（`*.example.com`は
//!   サブドメイン）。IPアドレスでの指定は受け付けない
//! - 名前はプロキシの側で引く。引いたアドレスが、ループバック・
//!   プライベート・リンクローカル等の公開でないアドレスなら、つながない（許可した
//!   名前がそうしたアドレスを指していても届かない）。Dockerのホスト等の公開の
//!   アドレスを指す名前なら、許可したポートにはつながる
//! - 許可した行き先そのものへの持ち出しは防げない（その行き先に載せて送れる）

use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

/// `mcp-rs egress-proxy`
pub const SUBCOMMAND: &str = "egress-proxy";

/// プロキシが待ち受けるポート（コンテナの中）
pub const LISTEN_PORT: u16 = 3128;

/// 待ち受けを始めたら、標準出力に1行書く。mcp-rsは、これを待ってから
/// プラグインを起動する
pub const READY_LINE: &str = "mcp-rs egress proxy listening";

/// 要求の行とヘッダーの大きさの上限（バイト）
const MAX_REQUEST_HEAD_BYTES: usize = 8 * 1024;

/// 要求のヘッダーを読む・名前を引く・行き先につなぐ、それぞれの時間の上限
const STEP_TIMEOUT: Duration = Duration::from_secs(10);

/// 同時に中継する接続の数の上限
const MAX_CONNECTIONS: usize = 256;

/// 許可した行き先（`host:port`。`*.example.com:443`はサブドメイン）
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AllowedHost {
    /// 小文字。`*.`で始まる指定なら、`*.`を除いた名前
    host: String,
    /// `*.example.com`（`example.com`自体は含まない）
    subdomains: bool,
    port: u16,
}

impl AllowedHost {
    /// `host:port`を読む。名前は英数字・`-`・`.`だけ（IPアドレスは不可）
    pub fn parse(text: &str) -> Result<Self, String> {
        let text = text.trim();
        let (host, port) = text
            .rsplit_once(':')
            .ok_or_else(|| format!("allowed host '{text}' needs a port (host:port)"))?;
        let port: u16 = port
            .parse()
            .ok()
            .filter(|port| *port != 0)
            .ok_or_else(|| format!("allowed host '{text}' has an invalid port"))?;
        let (host, subdomains) = match host.strip_prefix("*.") {
            Some(rest) => (rest, true),
            None => (host, false),
        };
        let host = host.to_ascii_lowercase();
        let valid_label = |label: &str| {
            !label.is_empty()
                && label.len() <= 63
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
        };
        if host.is_empty() || host.len() > 253 || !host.split('.').all(valid_label) {
            return Err(format!(
                "allowed host '{text}' must be a host name (letters, digits, '-' and '.'), optionally starting with '*.'"
            ));
        }
        // IPアドレスは名前で書く（プロキシの側で引いたアドレスを確かめるため）
        if host.parse::<IpAddr>().is_ok()
            || host
                .split('.')
                .all(|l| l.chars().all(|c| c.is_ascii_digit()))
        {
            return Err(format!(
                "allowed host '{text}' must be a host name, not an IP address"
            ));
        }
        Ok(Self {
            host,
            subdomains,
            port,
        })
    }

    fn matches(&self, host: &str, port: u16) -> bool {
        if port != self.port {
            return false;
        }
        let host = host.trim_end_matches('.').to_ascii_lowercase();
        if self.subdomains {
            host.strip_suffix(self.host.as_str())
                .is_some_and(|prefix| prefix.len() > 1 && prefix.ends_with('.'))
        } else {
            host == self.host
        }
    }
}

/// 許可した行き先の一覧を読む。空なら誤り
pub fn parse_allowed_hosts(list: &[String]) -> Result<Vec<AllowedHost>, String> {
    if list.is_empty() {
        return Err("allowed_hosts is empty".to_string());
    }
    list.iter().map(|text| AllowedHost::parse(text)).collect()
}

/// どの行き先に中継するか
#[derive(Debug, Clone)]
pub struct Policy {
    allowed: Vec<AllowedHost>,
    /// ループバック・プライベート等のアドレスにもつなぐ（テスト用）
    allow_private: bool,
}

impl Policy {
    pub fn new(allowed: Vec<AllowedHost>) -> Self {
        Self {
            allowed,
            allow_private: false,
        }
    }

    fn allows(&self, host: &str, port: u16) -> bool {
        self.allowed
            .iter()
            .any(|allowed| allowed.matches(host, port))
    }
}

/// 外の（公開の）アドレスか。ループバック・プライベート・リンクローカル・
/// CGNAT・文書用・マルチキャスト等は`false`
fn is_public(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            let [a, b, c, _] = v4.octets();
            !(v4.is_loopback()
                || v4.is_private()
                || v4.is_link_local()
                || v4.is_broadcast()
                || v4.is_documentation()
                || v4.is_unspecified()
                || v4.is_multicast()
                || a == 0
                || (a == 100 && (64..128).contains(&b))
                // 192.0.0.0/24（IETFの用途）だけ。192.0.64.0/18等は公開（PR #828の
                // レビュー）
                || (a == 192 && b == 0 && c == 0)
                || (a == 198 && (18..20).contains(&b))
                || a >= 240)
        }
        IpAddr::V6(v6) => {
            if let Some(v4) = v6.to_ipv4_mapped() {
                return is_public(IpAddr::V4(v4));
            }
            let first = v6.segments()[0];
            !(v6.is_loopback()
                || v6.is_unspecified()
                || v6.is_multicast()
                || (first & 0xfe00) == 0xfc00
                || (first & 0xffc0) == 0xfe80
                // 廃止されたサイトローカル（fec0::/10）（PR #828のレビュー）
                || (first & 0xffc0) == 0xfec0
                || first == 0x2001 && v6.segments()[1] == 0x0db8
                || first == 0x0064 && v6.segments()[1] == 0xff9b)
        }
    }
}

/// `mcp-rs egress-proxy --listen <addr> --allow <host:port>...`
pub async fn run(args: &[String]) -> Result<(), String> {
    let mut listen: SocketAddr = ([0, 0, 0, 0], LISTEN_PORT).into();
    let mut allow = Vec::new();
    let mut rest = args.iter();
    while let Some(arg) = rest.next() {
        match arg.as_str() {
            "--listen" => {
                listen = rest
                    .next()
                    .and_then(|addr| addr.parse().ok())
                    .ok_or("--listen needs an address such as 0.0.0.0:3128")?;
            }
            "--allow" => allow.push(rest.next().ok_or("--allow needs host:port")?.clone()),
            other => return Err(format!("unknown argument '{other}'")),
        }
    }
    let policy = Policy::new(parse_allowed_hosts(&allow)?);
    let listener = TcpListener::bind(listen)
        .await
        .map_err(|e| format!("could not listen on {listen}: {e}"))?;
    println!("{READY_LINE}");
    use std::io::Write;
    let _ = std::io::stdout().flush();
    serve(listener, policy).await;
    Ok(())
}

/// 待ち受けて、接続ごとに中継する
pub async fn serve(listener: TcpListener, policy: Policy) {
    let policy = Arc::new(policy);
    let slots = Arc::new(tokio::sync::Semaphore::new(MAX_CONNECTIONS));
    loop {
        let Ok((client, _)) = listener.accept().await else {
            continue;
        };
        let Ok(slot) = slots.clone().try_acquire_owned() else {
            // 多すぎる接続は、すぐ閉じる
            drop(client);
            continue;
        };
        let policy = policy.clone();
        tokio::spawn(async move {
            handle(client, &policy).await;
            drop(slot);
        });
    }
}

/// 中継を断った理由（応答の状態コードと、ログの文）
struct Refusal {
    status: &'static str,
    reason: String,
}

fn refuse(status: &'static str, reason: impl Into<String>) -> Refusal {
    Refusal {
        status,
        reason: reason.into(),
    }
}

async fn handle(mut client: TcpStream, policy: &Policy) {
    match connect(&mut client, policy).await {
        Ok((target, mut upstream, leftover)) => {
            if client
                .write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n")
                .await
                .is_err()
            {
                return;
            }
            if !leftover.is_empty() && upstream.write_all(&leftover).await.is_err() {
                return;
            }
            eprintln!("allowed CONNECT {target}");
            let _ = tokio::io::copy_bidirectional(&mut client, &mut upstream).await;
        }
        Err(refusal) => {
            eprintln!("refused: {}", refusal.reason);
            let body = format!("{}\n", refusal.reason);
            let response = format!(
                "HTTP/1.1 {}\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                refusal.status,
                body.len(),
                body
            );
            let _ = client.write_all(response.as_bytes()).await;
        }
    }
}

/// 要求を読み、許可した行き先につなぐ。つないだ行き先（表示用）と、接続と、
/// ヘッダーの後に読んでしまった分を返す
async fn connect(
    client: &mut TcpStream,
    policy: &Policy,
) -> Result<(String, TcpStream, Vec<u8>), Refusal> {
    let (head, leftover) = tokio::time::timeout(STEP_TIMEOUT, read_head(client))
        .await
        .map_err(|_| {
            refuse(
                "408 Request Timeout",
                "the request was not received in time",
            )
        })??;
    let request_line = head.lines().next().unwrap_or("");
    let mut parts = request_line.split(' ');
    let (method, target) = (parts.next().unwrap_or(""), parts.next().unwrap_or(""));
    if method != "CONNECT" {
        return Err(refuse(
            "405 Method Not Allowed",
            format!("{method} is not relayed: only CONNECT is allowed (for example HTTPS through HTTPS_PROXY)"),
        ));
    }
    let (host, port) = target
        .rsplit_once(':')
        .and_then(|(host, port)| Some((host, port.parse::<u16>().ok()?)))
        .filter(|(host, _)| !host.is_empty() && !host.starts_with('['))
        .ok_or_else(|| {
            refuse(
                "400 Bad Request",
                format!("invalid CONNECT target '{target}'"),
            )
        })?;
    if !policy.allows(host, port) {
        return Err(refuse(
            "403 Forbidden",
            format!("{host}:{port} is not in allowed_hosts"),
        ));
    }
    let addresses: Vec<SocketAddr> =
        tokio::time::timeout(STEP_TIMEOUT, tokio::net::lookup_host((host, port)))
            .await
            .map_err(|_| {
                refuse(
                    "504 Gateway Timeout",
                    format!("looking up {host} timed out"),
                )
            })?
            .map_err(|e| refuse("502 Bad Gateway", format!("could not look up {host}: {e}")))?
            .filter(|address| policy.allow_private || is_public(address.ip()))
            .collect();
    if addresses.is_empty() {
        return Err(refuse(
            "403 Forbidden",
            format!("{host} does not resolve to a public address"),
        ));
    }
    let mut last_error = String::new();
    for address in addresses {
        match tokio::time::timeout(STEP_TIMEOUT, TcpStream::connect(address)).await {
            Ok(Ok(upstream)) => return Ok((format!("{host}:{port}"), upstream, leftover)),
            Ok(Err(e)) => last_error = e.to_string(),
            Err(_) => last_error = "timed out".to_string(),
        }
    }
    Err(refuse(
        "502 Bad Gateway",
        format!("could not connect to {host}:{port}: {last_error}"),
    ))
}

/// 要求の行とヘッダー（空行まで）を読む。空行の後に読んだ分も返す
async fn read_head(client: &mut TcpStream) -> Result<(String, Vec<u8>), Refusal> {
    let mut buffer = Vec::new();
    let mut chunk = [0u8; 1024];
    loop {
        if let Some(end) = buffer.windows(4).position(|w| w == b"\r\n\r\n") {
            let leftover = buffer.split_off(end + 4);
            let head = String::from_utf8(buffer)
                .map_err(|_| refuse("400 Bad Request", "the request is not text"))?;
            return Ok((head, leftover));
        }
        if buffer.len() > MAX_REQUEST_HEAD_BYTES {
            return Err(refuse(
                "431 Request Header Fields Too Large",
                "the request headers are too large",
            ));
        }
        let read = client.read(&mut chunk).await.map_err(|e| {
            refuse(
                "400 Bad Request",
                format!("could not read the request: {e}"),
            )
        })?;
        if read == 0 {
            return Err(refuse("400 Bad Request", "the connection was closed"));
        }
        buffer.extend_from_slice(&chunk[..read]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_allowed_hosts() {
        let host = AllowedHost::parse("API.Example.com:443").unwrap();
        assert!(host.matches("api.example.com", 443));
        assert!(host.matches("API.EXAMPLE.COM.", 443));
        assert!(!host.matches("api.example.com", 80));
        assert!(!host.matches("x.api.example.com", 443));

        let wildcard = AllowedHost::parse("*.example.com:443").unwrap();
        assert!(wildcard.matches("a.example.com", 443));
        assert!(wildcard.matches("a.b.example.com", 443));
        assert!(!wildcard.matches("example.com", 443));
        assert!(!wildcard.matches("badexample.com", 443));

        for bad in [
            "api.example.com",
            "api.example.com:0",
            "api.example.com:99999",
            ":443",
            "1.2.3.4:443",
            "[::1]:443",
            "*.:443",
            "a b.com:443",
            "-a.com:443",
            "a..com:443",
        ] {
            assert!(AllowedHost::parse(bad).is_err(), "{bad}");
        }
        assert!(parse_allowed_hosts(&[]).is_err());
    }

    #[test]
    fn test_is_public() {
        for private in [
            "127.0.0.1",
            "10.1.2.3",
            "172.17.0.1",
            "192.168.1.1",
            "169.254.169.254",
            "100.64.0.1",
            "0.0.0.0",
            "255.255.255.255",
            "::1",
            "fd00::1",
            "fe80::1",
            "fec0::1",
            "feff::1",
            "::ffff:127.0.0.1",
            "64:ff9b::7f00:1",
            "192.0.0.1",
        ] {
            assert!(!is_public(private.parse().unwrap()), "{private}");
        }
        for public in ["93.184.216.34", "1.1.1.1", "192.0.64.1", "2606:4700::1111"] {
            assert!(is_public(public.parse().unwrap()), "{public}");
        }
    }

    /// 1行を受けて、そのまま返すサーバー
    async fn echo_server() -> SocketAddr {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move {
            while let Ok((mut stream, _)) = listener.accept().await {
                tokio::spawn(async move {
                    let mut buf = [0u8; 64];
                    while let Ok(n) = stream.read(&mut buf).await {
                        if n == 0 || stream.write_all(&buf[..n]).await.is_err() {
                            break;
                        }
                    }
                });
            }
        });
        address
    }

    async fn proxy(allow: &[&str], allow_private: bool) -> SocketAddr {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let allowed: Vec<String> = allow.iter().map(|s| s.to_string()).collect();
        let mut policy = Policy::new(parse_allowed_hosts(&allowed).unwrap());
        policy.allow_private = allow_private;
        tokio::spawn(serve(listener, policy));
        address
    }

    /// 要求を送り、応答の最初の行を返す。つながったら、その接続も返す
    async fn send(proxy: SocketAddr, request: &str) -> (String, TcpStream) {
        let mut stream = TcpStream::connect(proxy).await.unwrap();
        stream.write_all(request.as_bytes()).await.unwrap();
        let mut response = Vec::new();
        let mut byte = [0u8; 1];
        while !response.ends_with(b"\r\n\r\n") {
            if stream.read(&mut byte).await.unwrap() == 0 {
                break;
            }
            response.push(byte[0]);
        }
        let text = String::from_utf8(response).unwrap();
        (text.lines().next().unwrap_or("").to_string(), stream)
    }

    #[tokio::test]
    async fn test_relays_only_allowed_connect() {
        let echo = echo_server().await;
        let allowed = format!("localhost:{}", echo.port());
        let proxy = proxy(&[&allowed], true).await;

        // 許可した行き先には中継する（ヘッダーの後に続けて送った分も届く）
        let (status, mut stream) = send(
            proxy,
            &format!("CONNECT {allowed} HTTP/1.1\r\nHost: {allowed}\r\n\r\nhello"),
        )
        .await;
        assert_eq!(status, "HTTP/1.1 200 Connection Established");
        let mut buf = [0u8; 5];
        stream.read_exact(&mut buf).await.unwrap();
        assert_eq!(&buf, b"hello");

        // 許可していないポート・名前
        let (status, _) = send(proxy, "CONNECT localhost:1 HTTP/1.1\r\n\r\n").await;
        assert_eq!(status, "HTTP/1.1 403 Forbidden");
        let (status, _) = send(
            proxy,
            &format!("CONNECT 127.0.0.1:{} HTTP/1.1\r\n\r\n", echo.port()),
        )
        .await;
        assert_eq!(status, "HTTP/1.1 403 Forbidden");
        // 平文のHTTPは中継しない
        let (status, _) = send(
            proxy,
            &format!("GET http://{allowed}/ HTTP/1.1\r\nHost: {allowed}\r\n\r\n"),
        )
        .await;
        assert_eq!(status, "HTTP/1.1 405 Method Not Allowed");
    }

    /// 許可した名前でも、内部のアドレスを指していれば、つながない
    #[tokio::test]
    async fn test_refuses_private_addresses() {
        let echo = echo_server().await;
        let allowed = format!("localhost:{}", echo.port());
        let proxy = proxy(&[&allowed], false).await;
        let (status, _) = send(proxy, &format!("CONNECT {allowed} HTTP/1.1\r\n\r\n")).await;
        assert_eq!(status, "HTTP/1.1 403 Forbidden");
    }
}
