# WAF (Web Application Firewall) 実装設計書

**Issue**: #76  
**ブランチ**: feature/waf-implementation  
**優先度**: High  
**推定期間**: 12日間（2.5週間）

## 📋 概要

アプリケーション層攻撃から保護するための包括的なWAF実装。現在、XSS/SQLインジェクション検出のみが存在し、完全なWAF機能が不足しています。

## 🎯 実装スコープ

### Phase 1: CORS実装（2日間）

- オリジン検証
- プリフライトリクエスト処理
- 認証情報対応CORS
- 設定管理

### Phase 2: CSP実装（2日間）

- CSPヘッダー生成
- ディレクティブ管理
- インラインスクリプト用Nonce生成
- Report URI設定

### Phase 3: リクエスト検証（3日間）

- リクエストボディサイズ制限
- HTTPメソッド制限
- Content-Type検証
- ファイルアップロード検証
  - MIMEタイプチェック
  - ファイルサイズ制限
  - マルウェアスキャン統合フック

### Phase 4: セキュリティヘッダー（1日間）

- X-Content-Type-Options
- X-Frame-Options
- X-XSS-Protection
- Strict-Transport-Security (HSTS)
- Referrer-Policy

### Phase 5: 拡張レート制限（2日間）

- エンドポイント単位のレート制限
- IPベースのレート制限（拡張）
- ユーザーベースのレート制限
- 動的レート調整

### Phase 6: 統合・テスト（2日間）

- ミドルウェア統合
- 包括的テストスイート
- パフォーマンス検証
- ドキュメント作成

## 🏗️ アーキテクチャ

```text
src/security/
├── waf/
│   ├── mod.rs              # WAFメインモジュール
│   ├── cors.rs             # CORS機能
│   ├── csp.rs              # Content Security Policy
│   ├── request_validator.rs # リクエスト検証
│   ├── security_headers.rs  # セキュリティヘッダー管理
│   └── rate_limiter.rs     # 拡張レート制限
├── mod.rs                  # 再エクスポート
└── [既存モジュール]

src/server/
└── middleware/
    ├── mod.rs
    └── waf_middleware.rs   # WAFミドルウェア統合
```

## 📊 データ構造

### WAF設定

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WafConfig {
    pub enabled: bool,
    pub cors: CorsConfig,
    pub csp: CspConfig,
    pub request_limits: RequestLimitsConfig,
    pub security_headers: SecurityHeadersConfig,
    pub rate_limiting: RateLimitingConfig,
    pub audit_logging: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CorsConfig {
    pub enabled: bool,
    pub allowed_origins: Vec<String>,
    pub allowed_methods: Vec<String>,
    pub allowed_headers: Vec<String>,
    pub exposed_headers: Vec<String>,
    pub allow_credentials: bool,
    pub max_age: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CspConfig {
    pub enabled: bool,
    pub default_src: Vec<String>,
    pub script_src: Vec<String>,
    pub style_src: Vec<String>,
    pub img_src: Vec<String>,
    pub connect_src: Vec<String>,
    pub font_src: Vec<String>,
    pub object_src: Vec<String>,
    pub media_src: Vec<String>,
    pub frame_src: Vec<String>,
    pub report_uri: Option<String>,
    pub use_nonce: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestLimitsConfig {
    pub max_body_size: usize,          // bytes
    pub allowed_methods: Vec<String>,
    pub allowed_content_types: Vec<String>,
    pub file_upload: FileUploadConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileUploadConfig {
    pub enabled: bool,
    pub max_file_size: usize,          // bytes
    pub allowed_mime_types: Vec<String>,
    pub scan_for_malware: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityHeadersConfig {
    pub x_content_type_options: bool,
    pub x_frame_options: String,       // DENY, SAMEORIGIN, ALLOW-FROM
    pub x_xss_protection: String,      // 0, 1, 1; mode=block
    pub strict_transport_security: Option<HstsConfig>,
    pub referrer_policy: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HstsConfig {
    pub max_age: u32,
    pub include_subdomains: bool,
    pub preload: bool,
}
```

## 🔒 セキュリティ考慮事項

### CORS

- 厳格なオリジン検証
- ワイルドカード制限
- 認証情報処理
- キャッシュ制御

### CSP

- リクエストごとのNonceローテーション
- 厳格なデフォルトポリシー
- テスト用レポートオンリーモード
- 違反レポート

### リクエスト検証

- 過大リクエストの早期拒否
- 大容量ボディのストリーム処理
- 非同期検証
- DoS保護

### レート制限

- 分散レート制限サポート
- スライディングウィンドウアルゴリズム
- バースト処理
- IPホワイトリスト/ブラックリスト

## 🎨 実装詳細

### CORSハンドラー

```rust
pub struct CorsHandler {
    config: CorsConfig,
}

impl CorsHandler {
    pub fn new(config: CorsConfig) -> Self {
        Self { config }
    }

    pub fn validate_origin(&self, origin: &str) -> Result<bool, WafError> {
        // Validate origin against allowed list
    }

    pub fn handle_preflight(&self, request: &Request) -> Response {
        // Handle OPTIONS preflight request
    }

    pub fn add_cors_headers(&self, response: &mut Response, origin: &str) {
        // Add appropriate CORS headers
    }
}
```

### CSPジェネレーター

```rust
pub struct CspGenerator {
    config: CspConfig,
}

impl CspGenerator {
    pub fn generate_nonce(&self) -> String {
        // 暗号学的に安全なNonceを生成
    }

    pub fn build_header(&self, nonce: Option<&str>) -> String {
        // CSPヘッダー文字列を構築
    }

    pub fn parse_violation_report(&self, report: &str) -> CspViolation {
        // CSP違反レポートを解析
    }
}
```

### リクエストバリデーター

```rust
pub struct RequestValidator {
    config: RequestLimitsConfig,
}

impl RequestValidator {
    pub async fn validate_request(&self, request: &Request) -> Result<(), WafError> {
        self.validate_method(request)?;
        self.validate_content_type(request)?;
        self.validate_body_size(request).await?;
        Ok(())
    }

    pub async fn validate_file_upload(&self, file: &UploadedFile) -> Result<(), WafError> {
        self.validate_file_size(file)?;
        self.validate_mime_type(file)?;
        if self.config.file_upload.scan_for_malware {
            self.scan_file(file).await?;
        }
        Ok(())
    }
}
```

## 📈 パフォーマンス目標

- **CORS検証**: < 0.1ms/リクエスト
- **CSP生成**: < 0.5ms/リクエスト
- **リクエスト検証**: < 1ms/リクエスト
- **WAF全体のオーバーヘッド**: < 5ms/リクエスト
- **メモリオーバーヘッド**: < 10MB/インスタンス

## ✅ テスト戦略

### 単体テスト

- [ ] CORSオリジン検証
- [ ] CSPヘッダー生成
- [ ] Nonce生成の一意性
- [ ] リクエストサイズ検証
- [ ] ファイルアップロード検証
- [ ] セキュリティヘッダー生成

### 統合テスト

- [ ] 完全なリクエスト/レスポンスサイクル
- [ ] プリフライト処理
- [ ] マルチオリジンシナリオ
- [ ] 大容量ファイルアップロード
- [ ] レート制限統合

### セキュリティテスト

- [ ] CORSバイパス試行
- [ ] CSPポリシー違反
- [ ] 過大リクエスト処理
- [ ] 悪意あるファイルアップロード
- [ ] ヘッダーインジェクション試行

### パフォーマンステスト

- [ ] CORS検証ベンチマーク
- [ ] CSP生成ベンチマーク
- [ ] リクエスト検証ベンチマーク
- [ ] WAF有効時の負荷テスト

## 📝 Configuration Example

```toml
[waf]
enabled = true
audit_logging = true

[waf.cors]
enabled = true
allowed_origins = ["https://example.com", "https://app.example.com"]
allowed_methods = ["GET", "POST", "PUT", "DELETE"]
allowed_headers = ["Content-Type", "Authorization"]
exposed_headers = ["X-Request-ID"]
allow_credentials = true
max_age = 86400

[waf.csp]
enabled = true
default_src = ["'self'"]
script_src = ["'self'", "'nonce-{NONCE}'"]
style_src = ["'self'", "'unsafe-inline'"]
img_src = ["'self'", "data:", "https:"]
connect_src = ["'self'"]
report_uri = "/csp-violation-report"
use_nonce = true

[waf.request_limits]
max_body_size = 10485760  # 10MB
allowed_methods = ["GET", "POST", "PUT", "PATCH", "DELETE", "OPTIONS"]
allowed_content_types = ["application/json", "application/x-www-form-urlencoded", "multipart/form-data"]

[waf.request_limits.file_upload]
enabled = true
max_file_size = 5242880  # 5MB
allowed_mime_types = ["image/jpeg", "image/png", "image/gif", "application/pdf"]
scan_for_malware = false  # Requires external integration

[waf.security_headers]
x_content_type_options = true
x_frame_options = "SAMEORIGIN"
x_xss_protection = "1; mode=block"
referrer_policy = "strict-origin-when-cross-origin"

[waf.security_headers.strict_transport_security]
max_age = 31536000
include_subdomains = true
preload = false
```

## 🚀 デプロイチェックリスト

- [ ] 設定の検証完了
- [ ] 本番環境用CORSオリジン設定
- [ ] CSPポリシーをレポートオンリーモードでテスト
- [ ] 予想トラフィックに応じたレート制限調整
- [ ] 監視アラート設定
- [ ] ドキュメント更新
- [ ] セキュリティチームレビュー完了

## 📚 参考資料

- [OWASP WAF ベストプラクティス](https://owasp.org/www-community/Web_Application_Firewall)
- [MDN CORS ドキュメント](https://developer.mozilla.org/en-US/docs/Web/HTTP/CORS)
- [CSP Level 3 仕様](https://www.w3.org/TR/CSP3/)
- [OWASP セキュアヘッダープロジェクト](https://owasp.org/www-project-secure-headers/)
