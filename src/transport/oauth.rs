//! OAuth 2.1のリソースサーバー（#420）
//!
//! HTTPモードで、外部の認可サーバー（Keycloak・Okta・Microsoft Entra ID等）が
//! 発行したアクセストークン（JWT）を検証する。MCPの仕様（2025-06-18の
//! 「Authorization」）に沿って、次を行う。
//!
//! - 署名を、認可サーバーの公開鍵（JWKS）で検証する。HMAC（共有鍵）と
//!   `none`は受け付けない
//! - 発行者（`iss`）・有効期限（`exp`・`nbf`）・対象（`aud`、RFC 8707）を
//!   確かめる。対象が違うトークン（別のサービス向けのもの）は受け付けない
//! - Protected Resource Metadata（RFC 9728）で、どの認可サーバーを使うかを
//!   クライアントに知らせる
//!
//! トークンイントロスペクション（RFC 7662、JWTでないトークン）には対応しない。

use jsonwebtoken::{
    decode, decode_header,
    jwk::{AlgorithmParameters, Jwk},
    Algorithm, DecodingKey, Validation,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::{Duration, Instant};
use tokio::sync::{Mutex, RwLock};
use tracing::{debug, info, warn};
use url::Url;

/// Protected Resource Metadata（RFC 9728）の場所
pub const PROTECTED_RESOURCE_METADATA_PATH: &str = "/.well-known/oauth-protected-resource";

/// 受け付ける署名アルゴリズム（公開鍵方式だけ）
const ALLOWED_ALGORITHMS: &[Algorithm] = &[
    Algorithm::RS256,
    Algorithm::RS384,
    Algorithm::RS512,
    Algorithm::PS256,
    Algorithm::PS384,
    Algorithm::PS512,
    Algorithm::ES256,
    Algorithm::ES384,
    Algorithm::EdDSA,
];

/// 知らない鍵ID（`kid`）のトークンが来たときに、JWKSを取り直す最短の間隔。
/// 偽のトークンで認可サーバーへのリクエストを増やされないようにする
const MIN_REFRESH_INTERVAL: Duration = Duration::from_secs(30);
/// 取得したJWKSを使い続ける長さ。過ぎたら取り直す（失効した鍵を外すため）
const KEYS_MAX_AGE: Duration = Duration::from_secs(60 * 60);
/// 認可サーバーへのリクエストのタイムアウト
const FETCH_TIMEOUT: Duration = Duration::from_secs(10);
/// 認可サーバーの応答（メタデータ・JWKS）の大きさの上限
const MAX_FETCH_BYTES: usize = 1024 * 1024;

fn default_leeway_seconds() -> u64 {
    60
}

/// `[transport.http.oauth]`の設定
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OAuthConfig {
    /// 認可サーバーの発行者（`iss`）。例: `https://auth.example.com/realms/mcp`
    pub issuer: String,
    /// mcp-rs自身のURL（クライアントが接続するURL）。例:
    /// `https://mcp.example.com/mcp`。トークンの対象（`aud`）と、Protected
    /// Resource Metadataの`resource`に使う
    pub resource: String,
    /// トークンの対象（`aud`）として受け付ける値。省略時は`resource`だけ
    #[serde(default)]
    pub audiences: Vec<String>,
    /// 認可サーバーの公開鍵（JWKS）のURL。省略時は、認可サーバーの
    /// メタデータ（RFC 8414・OpenID Connect Discovery）から探す
    #[serde(default)]
    pub jwks_uri: Option<String>,
    /// トークンに必要なスコープ。すべてが無いトークンは403にする
    #[serde(default)]
    pub required_scopes: Vec<String>,
    /// 有効期限の確認で許す時計のずれ（秒）。既定60
    #[serde(default = "default_leeway_seconds")]
    pub leeway_seconds: u64,
    /// スコープ → 使えるツール（#52）。APIキーの`tools`と同じ書き方。
    /// トークンは、持っているスコープに対応するツールをすべて使える。
    /// 設定すると、対応するスコープを1つも持たないトークンは、どのツールも
    /// 使えない。空（既定）なら、どのトークンもすべてのツールを使える
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub scope_tools: std::collections::BTreeMap<String, Vec<String>>,
}

impl OAuthConfig {
    /// 設定を確かめる。認可サーバーとの通信には、HTTPSを使う（ループ
    /// バックのアドレスだけは、試験のためにHTTPを許す）
    pub fn validate(&self) -> Result<(), String> {
        check_server_url("issuer", &self.issuer)?;
        if let Some(jwks_uri) = &self.jwks_uri {
            check_server_url("jwks_uri", jwks_uri)?;
        }
        let resource = Url::parse(&self.resource)
            .map_err(|e| format!("oauth.resource must be an absolute URL: {}", e))?;
        if !matches!(resource.scheme(), "https" | "http") || resource.host().is_none() {
            return Err("oauth.resource must be an http(s) URL".to_string());
        }
        if resource.fragment().is_some() {
            return Err("oauth.resource must not contain a fragment".to_string());
        }
        if self.audiences.iter().any(|a| a.trim().is_empty()) {
            return Err("oauth.audiences must not contain an empty value".to_string());
        }
        if self.leeway_seconds > 600 {
            return Err("oauth.leeway_seconds must be 600 or less".to_string());
        }
        for (scope, tools) in &self.scope_tools {
            crate::mcp::ToolPatternSet::parse(tools)
                .map_err(|e| format!("oauth.scope_tools.\"{}\" is invalid: {}", scope, e))?;
        }
        Ok(())
    }

    /// トークンの対象（`aud`）として受け付ける値
    fn accepted_audiences(&self) -> Vec<String> {
        if self.audiences.is_empty() {
            vec![self.resource.clone()]
        } else {
            self.audiences.clone()
        }
    }

    /// Protected Resource Metadataのパス（RFC 9728）。`resource`にパスが
    /// あれば、`/.well-known/oauth-protected-resource`の後ろに付ける
    pub fn resource_metadata_path(&self) -> String {
        let path = Url::parse(&self.resource)
            .map(|u| u.path().trim_end_matches('/').to_string())
            .unwrap_or_default();
        format!("{}{}", PROTECTED_RESOURCE_METADATA_PATH, path)
    }

    /// Protected Resource MetadataのURL（`WWW-Authenticate`で知らせる）
    pub fn resource_metadata_url(&self) -> String {
        let origin = Url::parse(&self.resource)
            .map(|u| u.origin().ascii_serialization())
            .unwrap_or_default();
        format!("{}{}", origin, self.resource_metadata_path())
    }

    /// Protected Resource Metadataの内容
    pub fn resource_metadata(&self) -> Value {
        let mut metadata = serde_json::json!({
            "resource": self.resource,
            "authorization_servers": [self.issuer],
            "bearer_methods_supported": ["header"],
            "resource_name": "mcp-rs",
        });
        if !self.required_scopes.is_empty() {
            metadata["scopes_supported"] = serde_json::json!(self.required_scopes);
        }
        metadata
    }
}

/// 認可サーバーのURLを確かめる。HTTPSか、ループバックへのHTTP
fn check_server_url(name: &str, value: &str) -> Result<(), String> {
    let url =
        Url::parse(value).map_err(|e| format!("oauth.{} must be an absolute URL: {}", name, e))?;
    match url.scheme() {
        "https" => Ok(()),
        "http" if is_loopback(&url) => Ok(()),
        _ => Err(format!(
            "oauth.{} must use https (http is allowed only for localhost)",
            name
        )),
    }
}

fn is_loopback(url: &Url) -> bool {
    match url.host() {
        Some(url::Host::Domain(domain)) => domain.eq_ignore_ascii_case("localhost"),
        Some(url::Host::Ipv4(ip)) => ip.is_loopback(),
        Some(url::Host::Ipv6(ip)) => ip.is_loopback(),
        None => false,
    }
}

/// 検証できたトークンの内容
#[derive(Debug, Clone, PartialEq)]
pub struct TokenInfo {
    /// トークンの主体（`sub`）
    pub subject: Option<String>,
    /// トークンを受け取ったクライアント（`azp`か`client_id`）
    pub client_id: Option<String>,
    /// トークンのスコープ（`scope`か`scp`）
    pub scopes: Vec<String>,
}

/// トークンを受け付けない理由
#[derive(Debug, Clone, PartialEq)]
pub enum TokenError {
    /// 無効なトークン（401、`error="invalid_token"`）。理由はログにだけ出す
    Invalid(String),
    /// 必要なスコープが無い（403、`error="insufficient_scope"`）
    InsufficientScope,
}

struct CachedKeys {
    keys: Vec<Jwk>,
    fetched_at: Instant,
}

/// アクセストークンを検証する。認可サーバーの公開鍵（JWKS）は、取得して
/// 使い回す
pub struct OAuthValidator {
    config: OAuthConfig,
    /// `scope_tools`を読んだもの
    scope_tools: Vec<(String, crate::mcp::ToolPatternSet)>,
    client: reqwest::Client,
    keys: RwLock<Option<CachedKeys>>,
    /// JWKSのURL（設定か、メタデータから見つけたもの）
    jwks_uri: RwLock<Option<String>>,
    /// 最後にJWKSを取りに行った時刻（取り直しを1件ずつにするロックも兼ねる）
    last_fetch: Mutex<Option<Instant>>,
}

impl std::fmt::Debug for OAuthValidator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OAuthValidator")
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

impl OAuthValidator {
    pub fn new(config: OAuthConfig) -> Result<Self, String> {
        config.validate()?;
        let client = reqwest::Client::builder()
            .timeout(FETCH_TIMEOUT)
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|e| format!("failed to create the HTTP client for OAuth: {}", e))?;
        let scope_tools = config
            .scope_tools
            .iter()
            .map(|(scope, tools)| {
                crate::mcp::ToolPatternSet::parse(tools).map(|set| (scope.clone(), set))
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            scope_tools,
            jwks_uri: RwLock::new(config.jwks_uri.clone()),
            config,
            client,
            keys: RwLock::new(None),
            last_fetch: Mutex::new(None),
        })
    }

    pub fn config(&self) -> &OAuthConfig {
        &self.config
    }

    /// 検証できたトークンの呼び出し元（名前と使えるツール、#52）
    pub fn caller_for(&self, info: &TokenInfo) -> crate::mcp::Caller {
        let name = info
            .client_id
            .as_deref()
            .or(info.subject.as_deref())
            .unwrap_or("unknown");
        let access = if self.scope_tools.is_empty() {
            crate::mcp::ToolAccess::All
        } else {
            crate::mcp::ToolAccess::any_of(
                self.scope_tools
                    .iter()
                    .filter(|(scope, _)| info.scopes.contains(scope))
                    .map(|(_, set)| set.clone())
                    .collect(),
            )
        };
        crate::mcp::Caller::new(format!("oauth:{}", name), access)
    }

    /// 起動時に、公開鍵を取得しておく。失敗しても、最初のリクエストで
    /// 取り直す
    pub async fn prefetch(&self) {
        match self.refresh_keys(true).await {
            Ok(count) => info!(
                "🔐 OAuth: loaded {} signing key(s) from the authorization server {}",
                count, self.config.issuer
            ),
            Err(e) => warn!(
                "OAuth: could not load signing keys from the authorization server {} ({}). Requests with access tokens are rejected until they can be loaded",
                self.config.issuer, e
            ),
        }
    }

    /// アクセストークンを検証する
    pub async fn validate(&self, token: &str) -> Result<TokenInfo, TokenError> {
        let header = decode_header(token)
            .map_err(|e| TokenError::Invalid(format!("malformed token: {}", e)))?;
        if !ALLOWED_ALGORITHMS.contains(&header.alg) {
            return Err(TokenError::Invalid(format!(
                "signing algorithm {:?} is not accepted",
                header.alg
            )));
        }
        let jwk = self.find_key(header.kid.as_deref(), header.alg).await?;
        let key = DecodingKey::from_jwk(&jwk)
            .map_err(|e| TokenError::Invalid(format!("unusable signing key: {}", e)))?;

        let mut validation = Validation::new(header.alg);
        validation.set_issuer(&[&self.config.issuer]);
        validation.set_audience(&self.config.accepted_audiences());
        validation.set_required_spec_claims(&["exp", "iss", "aud"]);
        validation.validate_nbf = true;
        validation.leeway = self.config.leeway_seconds;
        let claims = decode::<serde_json::Map<String, Value>>(token, &key, &validation)
            .map_err(|e| TokenError::Invalid(e.to_string()))?
            .claims;

        let info = TokenInfo {
            subject: claims
                .get("sub")
                .and_then(Value::as_str)
                .map(str::to_string),
            client_id: claims
                .get("azp")
                .or_else(|| claims.get("client_id"))
                .and_then(Value::as_str)
                .map(str::to_string),
            scopes: token_scopes(&claims),
        };
        if !self
            .config
            .required_scopes
            .iter()
            .all(|s| info.scopes.contains(s))
        {
            return Err(TokenError::InsufficientScope);
        }
        Ok(info)
    }

    /// トークンのヘッダーに合う公開鍵を探す。見つからなければ、JWKSを取り
    /// 直して（鍵の更新に追従するため）もう一度探す
    async fn find_key(&self, kid: Option<&str>, alg: Algorithm) -> Result<Jwk, TokenError> {
        let stale = {
            let cached = self.keys.read().await;
            if let Some(cached) = cached.as_ref() {
                if let Some(jwk) = select_key(&cached.keys, kid, alg) {
                    if cached.fetched_at.elapsed() < KEYS_MAX_AGE {
                        return Ok(jwk);
                    }
                }
                true
            } else {
                true
            }
        };
        if stale {
            if let Err(e) = self.refresh_keys(false).await {
                warn!("OAuth: failed to refresh signing keys: {}", e);
            }
        }
        let cached = self.keys.read().await;
        cached
            .as_ref()
            .and_then(|cached| select_key(&cached.keys, kid, alg))
            .ok_or_else(|| {
                TokenError::Invalid(match kid {
                    Some(kid) => format!("no signing key with kid '{}'", kid),
                    None => "no matching signing key".to_string(),
                })
            })
    }

    /// JWKSを取り直す。`force`でなければ、前回から`MIN_REFRESH_INTERVAL`
    /// 経っていない場合は何もしない。取得できた鍵の数を返す
    async fn refresh_keys(&self, force: bool) -> Result<usize, String> {
        let mut last_fetch = self.last_fetch.lock().await;
        if !force && last_fetch.is_some_and(|t| t.elapsed() < MIN_REFRESH_INTERVAL) {
            return Ok(self
                .keys
                .read()
                .await
                .as_ref()
                .map_or(0, |cached| cached.keys.len()));
        }
        *last_fetch = Some(Instant::now());

        // 読み取りのロックは、ここで手放す（下で書き込むため）
        let configured = self.jwks_uri.read().await.clone();
        let jwks_uri = match configured {
            Some(uri) => uri,
            None => {
                let uri = self.discover_jwks_uri().await?;
                *self.jwks_uri.write().await = Some(uri.clone());
                uri
            }
        };
        let jwks = self.fetch_json(&jwks_uri).await?;
        let keys = parse_jwks(&jwks);
        if keys.is_empty() {
            return Err(format!("no usable signing keys in {}", jwks_uri));
        }
        debug!(
            "OAuth: loaded {} signing key(s) from {}",
            keys.len(),
            jwks_uri
        );
        let count = keys.len();
        *self.keys.write().await = Some(CachedKeys {
            keys,
            fetched_at: Instant::now(),
        });
        Ok(count)
    }

    /// 認可サーバーのメタデータから、JWKSのURLを探す。RFC 8414
    /// （`/.well-known/oauth-authorization-server`）と、OpenID Connect
    /// Discovery（`/.well-known/openid-configuration`）の両方を試す
    async fn discover_jwks_uri(&self) -> Result<String, String> {
        let mut errors = Vec::new();
        for url in metadata_urls(&self.config.issuer) {
            match self.fetch_json(&url).await {
                Ok(metadata) => {
                    let issuer = metadata.get("issuer").and_then(Value::as_str);
                    if issuer != Some(self.config.issuer.as_str()) {
                        errors.push(format!(
                            "{}: issuer {:?} does not match the configured issuer",
                            url, issuer
                        ));
                        continue;
                    }
                    let Some(jwks_uri) = metadata.get("jwks_uri").and_then(Value::as_str) else {
                        errors.push(format!("{}: no jwks_uri", url));
                        continue;
                    };
                    check_server_url("jwks_uri", jwks_uri)?;
                    return Ok(jwks_uri.to_string());
                }
                Err(e) => errors.push(e),
            }
        }
        Err(format!(
            "could not find the authorization server metadata ({}). Set oauth.jwks_uri",
            errors.join("; ")
        ))
    }

    async fn fetch_json(&self, url: &str) -> Result<Value, String> {
        let mut response = self
            .client
            .get(url)
            .header("Accept", "application/json")
            .send()
            .await
            .map_err(|e| format!("{}: {}", url, e))?;
        if !response.status().is_success() {
            return Err(format!("{}: HTTP {}", url, response.status()));
        }
        let mut body = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|e| format!("{}: {}", url, e))?
        {
            if body.len() + chunk.len() > MAX_FETCH_BYTES {
                return Err(format!("{}: response is too large", url));
            }
            body.extend_from_slice(&chunk);
        }
        serde_json::from_slice(&body).map_err(|e| format!("{}: invalid JSON: {}", url, e))
    }
}

/// 認可サーバーのメタデータのURLの候補
fn metadata_urls(issuer: &str) -> Vec<String> {
    let mut urls = Vec::new();
    if let Ok(url) = Url::parse(issuer) {
        let origin = url.origin().ascii_serialization();
        let path = url.path().trim_end_matches('/');
        // RFC 8414: パスは.well-knownの後ろに付ける
        urls.push(format!(
            "{}/.well-known/oauth-authorization-server{}",
            origin, path
        ));
        // OpenID Connect Discovery: 発行者の後ろに付ける（Keycloak等）
        urls.push(format!(
            "{}/.well-known/openid-configuration",
            issuer.trim_end_matches('/')
        ));
    }
    urls
}

/// JWKSから、署名の検証に使える鍵を取り出す。暗号化用（`use: enc`）と
/// 共有鍵（`kty: oct`）、読めない鍵は飛ばす
fn parse_jwks(jwks: &Value) -> Vec<Jwk> {
    jwks.get("keys")
        .and_then(Value::as_array)
        .map(|keys| {
            keys.iter()
                .filter(|k| k.get("use").and_then(Value::as_str) != Some("enc"))
                .filter(|k| k.get("kty").and_then(Value::as_str) != Some("oct"))
                .filter_map(|k| serde_json::from_value::<Jwk>(k.clone()).ok())
                .filter(|k| !matches!(k.algorithm, AlgorithmParameters::OctetKey(_)))
                .collect()
        })
        .unwrap_or_default()
}

/// トークンのヘッダー（`kid`・`alg`）に合う鍵を選ぶ。`kid`が無い場合は、
/// 合う鍵が1つだけのときに限って使う
fn select_key(keys: &[Jwk], kid: Option<&str>, alg: Algorithm) -> Option<Jwk> {
    let compatible = |jwk: &&Jwk| {
        let family_ok = match &jwk.algorithm {
            AlgorithmParameters::RSA(_) => matches!(
                alg,
                Algorithm::RS256
                    | Algorithm::RS384
                    | Algorithm::RS512
                    | Algorithm::PS256
                    | Algorithm::PS384
                    | Algorithm::PS512
            ),
            AlgorithmParameters::EllipticCurve(_) => {
                matches!(alg, Algorithm::ES256 | Algorithm::ES384)
            }
            AlgorithmParameters::OctetKeyPair(_) => alg == Algorithm::EdDSA,
            AlgorithmParameters::OctetKey(_) => false,
        };
        let alg_ok = jwk
            .common
            .key_algorithm
            // KeyAlgorithmとAlgorithmは、同じ名前の値を持つ
            .is_none_or(|key_alg| format!("{:?}", key_alg) == format!("{:?}", alg));
        family_ok && alg_ok
    };
    match kid {
        Some(kid) => keys
            .iter()
            .filter(compatible)
            .find(|jwk| jwk.common.key_id.as_deref() == Some(kid))
            .cloned(),
        None => {
            let mut matching = keys.iter().filter(compatible);
            match (matching.next(), matching.next()) {
                (Some(jwk), None) => Some(jwk.clone()),
                _ => None,
            }
        }
    }
}

/// トークンのスコープ。`scope`（空白区切りの文字列、RFC 8693等）か、`scp`
/// （配列か文字列、Microsoft Entra ID等）
fn token_scopes(claims: &serde_json::Map<String, Value>) -> Vec<String> {
    let split = |s: &str| s.split_whitespace().map(str::to_string).collect::<Vec<_>>();
    match claims.get("scope").or_else(|| claims.get("scp")) {
        Some(Value::String(s)) => split(s),
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect(),
        _ => Vec::new(),
    }
}

/// `WWW-Authenticate`ヘッダーの値（RFC 6750・RFC 9728）
pub fn www_authenticate(config: &OAuthConfig, error: Option<&TokenError>) -> String {
    let metadata = format!("resource_metadata=\"{}\"", config.resource_metadata_url());
    match error {
        None => format!("Bearer {}", metadata),
        Some(TokenError::Invalid(_)) => format!(
            "Bearer error=\"invalid_token\", error_description=\"The access token is invalid or expired\", {}",
            metadata
        ),
        Some(TokenError::InsufficientScope) => format!(
            "Bearer error=\"insufficient_scope\", scope=\"{}\", {}",
            config.required_scopes.join(" "),
            metadata
        ),
    }
}

#[cfg(test)]
pub(crate) mod test_support {
    //! テスト用の認可サーバー（ES256の鍵で署名し、JWKS・メタデータを返す）

    use super::*;
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
    use jsonwebtoken::{encode, EncodingKey, Header};
    use ring::{
        rand::SystemRandom,
        signature::{EcdsaKeyPair, KeyPair, ECDSA_P256_SHA256_FIXED_SIGNING},
    };
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };

    pub struct TestAuthServer {
        pub issuer: String,
        pub jwks_requests: Arc<AtomicUsize>,
        encoding_key: EncodingKey,
        kid: String,
    }

    pub fn now() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
    }

    fn new_key() -> (EncodingKey, Value) {
        let rng = SystemRandom::new();
        let pkcs8 = EcdsaKeyPair::generate_pkcs8(&ECDSA_P256_SHA256_FIXED_SIGNING, &rng).unwrap();
        let pair = EcdsaKeyPair::from_pkcs8(&ECDSA_P256_SHA256_FIXED_SIGNING, pkcs8.as_ref(), &rng)
            .unwrap();
        let public = pair.public_key().as_ref(); // 0x04 || x || y
        let jwk = serde_json::json!({
            "kty": "EC",
            "crv": "P-256",
            "x": URL_SAFE_NO_PAD.encode(&public[1..33]),
            "y": URL_SAFE_NO_PAD.encode(&public[33..65]),
        });
        (EncodingKey::from_ec_der(pkcs8.as_ref()), jwk)
    }

    impl TestAuthServer {
        /// 127.0.0.1で起動する。`/.well-known/openid-configuration`と
        /// `/jwks`を返す。JWKSには、暗号化用の鍵と共有鍵も混ぜる
        pub async fn start() -> Self {
            use axum::{routing::get, Json, Router};

            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let base = format!("http://{}", listener.local_addr().unwrap());
            let issuer = format!("{}/realms/test", base);
            let (encoding_key, mut jwk) = new_key();
            let kid = "test-key".to_string();
            jwk["kid"] = serde_json::json!(kid);
            jwk["use"] = serde_json::json!("sig");
            jwk["alg"] = serde_json::json!("ES256");
            let (_, mut enc_jwk) = new_key();
            enc_jwk["kid"] = serde_json::json!("enc-key");
            enc_jwk["use"] = serde_json::json!("enc");
            let jwks = serde_json::json!({ "keys": [
                enc_jwk,
                { "kty": "oct", "kid": "hmac", "k": "c2VjcmV0" },
                jwk,
            ]});
            let metadata = serde_json::json!({
                "issuer": issuer,
                "jwks_uri": format!("{}/jwks", base),
            });
            let jwks_requests = Arc::new(AtomicUsize::new(0));
            let counter = jwks_requests.clone();
            let issuer_path = "/realms/test/.well-known/openid-configuration";
            let app = Router::new()
                .route(issuer_path, get(move || async move { Json(metadata) }))
                .route(
                    "/jwks",
                    get(move || async move {
                        counter.fetch_add(1, Ordering::SeqCst);
                        Json(jwks)
                    }),
                );
            tokio::spawn(async move {
                axum::serve(listener, app).await.unwrap();
            });
            Self {
                issuer,
                jwks_requests,
                encoding_key,
                kid,
            }
        }

        pub fn config(&self, resource: &str) -> OAuthConfig {
            OAuthConfig {
                issuer: self.issuer.clone(),
                resource: resource.to_string(),
                audiences: Vec::new(),
                jwks_uri: None,
                required_scopes: Vec::new(),
                leeway_seconds: 0,
                scope_tools: Default::default(),
            }
        }

        /// 標準的なクレーム（`iss`・`aud`・`exp`・`sub`）に`extra`を重ねて署名する
        pub fn token(&self, audience: &str, extra: Value) -> String {
            let mut claims = serde_json::json!({
                "iss": self.issuer,
                "aud": audience,
                "sub": "user-1",
                "azp": "test-client",
                "exp": now() + 300,
                "iat": now(),
            });
            if let (Value::Object(claims), Value::Object(extra)) = (&mut claims, extra) {
                for (k, v) in extra {
                    if v.is_null() {
                        claims.remove(&k);
                    } else {
                        claims.insert(k, v);
                    }
                }
            }
            self.sign(&claims, Some(&self.kid))
        }

        pub fn sign(&self, claims: &Value, kid: Option<&str>) -> String {
            let mut header = Header::new(Algorithm::ES256);
            header.kid = kid.map(str::to_string);
            encode(&header, claims, &self.encoding_key).unwrap()
        }

        /// 別の鍵（JWKSに無い鍵）で署名する
        pub fn sign_with_unknown_key(&self, claims: &Value) -> String {
            let (key, _) = new_key();
            let mut header = Header::new(Algorithm::ES256);
            header.kid = Some(self.kid.clone());
            encode(&header, claims, &key).unwrap()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::*;
    use super::*;
    use std::sync::atomic::Ordering;

    const RESOURCE: &str = "https://mcp.example.com/mcp";

    #[test]
    fn test_config_validation() {
        let ok = OAuthConfig {
            issuer: "https://auth.example.com/realms/mcp".to_string(),
            resource: RESOURCE.to_string(),
            audiences: Vec::new(),
            jwks_uri: None,
            required_scopes: Vec::new(),
            leeway_seconds: 60,
            scope_tools: Default::default(),
        };
        assert!(ok.validate().is_ok());
        // 認可サーバーへのHTTPは、ループバックだけ許す
        for issuer in [
            "http://localhost:8080/realms/mcp",
            "http://127.0.0.1:8080/x",
        ] {
            let c = OAuthConfig {
                issuer: issuer.to_string(),
                ..ok.clone()
            };
            assert!(c.validate().is_ok(), "{}", issuer);
        }
        for issuer in [
            "http://auth.example.com",
            "not a url",
            "ftp://auth.example.com",
        ] {
            let c = OAuthConfig {
                issuer: issuer.to_string(),
                ..ok.clone()
            };
            assert!(c.validate().is_err(), "{}", issuer);
        }
        let c = OAuthConfig {
            jwks_uri: Some("http://auth.example.com/jwks".to_string()),
            ..ok.clone()
        };
        assert!(c.validate().is_err());
        let c = OAuthConfig {
            resource: "mcp.example.com".to_string(),
            ..ok.clone()
        };
        assert!(c.validate().is_err());
    }

    #[test]
    fn test_resource_metadata() {
        let config = OAuthConfig {
            issuer: "https://auth.example.com".to_string(),
            resource: RESOURCE.to_string(),
            audiences: Vec::new(),
            jwks_uri: None,
            required_scopes: vec!["mcp:tools".to_string()],
            leeway_seconds: 60,
            scope_tools: Default::default(),
        };
        assert_eq!(
            config.resource_metadata_path(),
            "/.well-known/oauth-protected-resource/mcp"
        );
        assert_eq!(
            config.resource_metadata_url(),
            "https://mcp.example.com/.well-known/oauth-protected-resource/mcp"
        );
        let metadata = config.resource_metadata();
        assert_eq!(metadata["resource"], serde_json::json!(RESOURCE));
        assert_eq!(
            metadata["authorization_servers"],
            serde_json::json!(["https://auth.example.com"])
        );
        assert_eq!(
            metadata["scopes_supported"],
            serde_json::json!(["mcp:tools"])
        );

        let root = OAuthConfig {
            resource: "https://mcp.example.com/".to_string(),
            ..config.clone()
        };
        assert_eq!(
            root.resource_metadata_path(),
            "/.well-known/oauth-protected-resource"
        );

        assert_eq!(
            www_authenticate(&config, None),
            "Bearer resource_metadata=\"https://mcp.example.com/.well-known/oauth-protected-resource/mcp\""
        );
        assert!(
            www_authenticate(&config, Some(&TokenError::InsufficientScope))
                .starts_with("Bearer error=\"insufficient_scope\", scope=\"mcp:tools\"")
        );
    }

    #[test]
    fn test_metadata_urls() {
        assert_eq!(
            metadata_urls("https://auth.example.com/realms/mcp"),
            vec![
                "https://auth.example.com/.well-known/oauth-authorization-server/realms/mcp",
                "https://auth.example.com/realms/mcp/.well-known/openid-configuration",
            ]
        );
        assert_eq!(
            metadata_urls("https://auth.example.com/"),
            vec![
                "https://auth.example.com/.well-known/oauth-authorization-server",
                "https://auth.example.com/.well-known/openid-configuration",
            ]
        );
    }

    #[test]
    fn test_token_scopes() {
        let claims = |v: Value| v.as_object().unwrap().clone();
        assert_eq!(
            token_scopes(&claims(serde_json::json!({ "scope": "a b  c" }))),
            vec!["a", "b", "c"]
        );
        assert_eq!(
            token_scopes(&claims(serde_json::json!({ "scp": ["a", "b"] }))),
            vec!["a", "b"]
        );
        assert!(token_scopes(&claims(serde_json::json!({}))).is_empty());
    }

    #[tokio::test]
    async fn test_valid_token_is_accepted() {
        let server = TestAuthServer::start().await;
        let validator = OAuthValidator::new(server.config(RESOURCE)).unwrap();
        let token = server.token(RESOURCE, serde_json::json!({ "scope": "openid mcp" }));
        let info = validator.validate(&token).await.unwrap();
        assert_eq!(info.subject.as_deref(), Some("user-1"));
        assert_eq!(info.client_id.as_deref(), Some("test-client"));
        assert_eq!(info.scopes, vec!["openid", "mcp"]);
        // 2回目は、取得済みの鍵を使う
        validator.validate(&token).await.unwrap();
        assert_eq!(server.jwks_requests.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_invalid_tokens_are_rejected() {
        let server = TestAuthServer::start().await;
        let validator = OAuthValidator::new(server.config(RESOURCE)).unwrap();
        let rejected = |token: String, why: &'static str| {
            let validator = &validator;
            async move {
                let result = validator.validate(&token).await;
                assert!(
                    matches!(result, Err(TokenError::Invalid(_))),
                    "{}: {:?}",
                    why,
                    result
                );
            }
        };

        rejected(
            server.token("https://other.example.com/mcp", serde_json::json!({})),
            "other audience",
        )
        .await;
        rejected(
            server.token(
                RESOURCE,
                serde_json::json!({ "iss": "https://evil.example.com" }),
            ),
            "other issuer",
        )
        .await;
        rejected(
            server.token(RESOURCE, serde_json::json!({ "exp": now() - 10 })),
            "expired",
        )
        .await;
        rejected(
            server.token(RESOURCE, serde_json::json!({ "nbf": now() + 300 })),
            "not yet valid",
        )
        .await;
        rejected(
            server.token(RESOURCE, serde_json::json!({ "exp": null })),
            "no exp",
        )
        .await;
        rejected(
            server.token(RESOURCE, serde_json::json!({ "aud": null })),
            "no aud",
        )
        .await;
        let claims = serde_json::json!({
            "iss": server.issuer, "aud": RESOURCE, "exp": now() + 300
        });
        rejected(server.sign_with_unknown_key(&claims), "unknown key").await;
        rejected(server.sign(&claims, Some("enc-key")), "encryption key").await;
        rejected("not-a-jwt".to_string(), "malformed").await;

        // HMAC（JWKSに共有鍵があっても）と`none`は受け付けない
        let hmac = jsonwebtoken::encode(
            &{
                let mut h = jsonwebtoken::Header::new(Algorithm::HS256);
                h.kid = Some("hmac".to_string());
                h
            },
            &claims,
            &jsonwebtoken::EncodingKey::from_secret(b"secret"),
        )
        .unwrap();
        rejected(hmac, "hmac").await;
        let header = base64::Engine::encode(
            &base64::engine::general_purpose::URL_SAFE_NO_PAD,
            br#"{"alg":"none","typ":"JWT"}"#,
        );
        let payload = base64::Engine::encode(
            &base64::engine::general_purpose::URL_SAFE_NO_PAD,
            claims.to_string(),
        );
        rejected(format!("{}.{}.", header, payload), "alg none").await;
    }

    #[tokio::test]
    async fn test_unknown_kid_refetch_is_rate_limited() {
        let server = TestAuthServer::start().await;
        let validator = OAuthValidator::new(server.config(RESOURCE)).unwrap();
        validator.prefetch().await;
        assert_eq!(server.jwks_requests.load(Ordering::SeqCst), 1);
        let claims = serde_json::json!({
            "iss": server.issuer, "aud": RESOURCE, "exp": now() + 300
        });
        for _ in 0..5 {
            let token = server.sign(&claims, Some("rotated-key"));
            assert!(validator.validate(&token).await.is_err());
        }
        // 知らない鍵IDが何度来ても、取り直しは間隔を空ける
        assert_eq!(server.jwks_requests.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_required_scopes_and_audiences() {
        let server = TestAuthServer::start().await;
        let config = OAuthConfig {
            required_scopes: vec!["mcp:tools".to_string()],
            audiences: vec!["mcp-rs".to_string()],
            ..server.config(RESOURCE)
        };
        let validator = OAuthValidator::new(config).unwrap();
        let with_scope = server.token("mcp-rs", serde_json::json!({ "scope": "mcp:tools" }));
        assert!(validator.validate(&with_scope).await.is_ok());
        let without = server.token("mcp-rs", serde_json::json!({ "scope": "openid" }));
        assert_eq!(
            validator.validate(&without).await,
            Err(TokenError::InsufficientScope)
        );
        // audiencesを指定した場合、resourceは対象として受け付けない
        let resource_aud = server.token(RESOURCE, serde_json::json!({ "scope": "mcp:tools" }));
        assert!(matches!(
            validator.validate(&resource_aud).await,
            Err(TokenError::Invalid(_))
        ));
    }

    #[tokio::test]
    async fn test_discovery_failure_is_reported() {
        let config = OAuthConfig {
            issuer: "http://127.0.0.1:9/nowhere".to_string(),
            resource: RESOURCE.to_string(),
            audiences: Vec::new(),
            jwks_uri: None,
            required_scopes: Vec::new(),
            leeway_seconds: 60,
            scope_tools: Default::default(),
        };
        let validator = OAuthValidator::new(config).unwrap();
        let err = validator.refresh_keys(true).await.unwrap_err();
        assert!(err.contains("Set oauth.jwks_uri"), "{}", err);
    }
}
