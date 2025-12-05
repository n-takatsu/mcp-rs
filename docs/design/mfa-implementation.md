# MFA (多要素認証) 実装設計書

## 概要

エンタープライズグレードのセキュリティのための包括的な多要素認証システム実装。Issue #75と#84に対応します。

## 優先度

🔴 **Critical (P0)** - 高優先度セキュリティ強化

## 推定タイムライン

### 合計: 2-3週間（12-15営業日）

- Phase 1: TOTP実装（3日間）
- Phase 2: バックアップコード（2日間）
- Phase 3: SMS認証（3日間）
- Phase 4: デバイス信頼（3日間）
- Phase 5: セッション統合（2日間）
- Phase 6: テスト・ドキュメント（2-3日間）

## アーキテクチャ

```
┌─────────────────────────────────────────────────────────────┐
│                    MFA System Architecture                   │
├─────────────────────────────────────────────────────────────┤
│                                                               │
│  ┌─────────────┐    ┌──────────────┐    ┌───────────────┐  │
│  │   TOTP      │    │   Backup     │    │      SMS      │  │
│  │  Generator  │───▶│    Codes     │◀───│  Verification │  │
│  └─────────────┘    └──────────────┘    └───────────────┘  │
│         │                   │                     │          │
│         └───────────────────┼─────────────────────┘          │
│                             ▼                                │
│                  ┌────────────────────┐                      │
│                  │   MFA Coordinator  │                      │
│                  └────────────────────┘                      │
│                             │                                │
│         ┌───────────────────┼───────────────────┐            │
│         ▼                   ▼                   ▼            │
│  ┌─────────────┐    ┌──────────────┐    ┌──────────┐       │
│  │   Device    │    │   Session    │    │  Audit   │       │
│  │    Trust    │    │  Management  │    │   Log    │       │
│  └─────────────┘    └──────────────┘    └──────────┘       │
│                                                               │
└─────────────────────────────────────────────────────────────┘
```

## Phase 1: TOTP実装（3日間）

### コンポーネント

#### 1.1 TOTPシークレット生成

```rust
pub struct TotpSecret {
    secret: Vec<u8>,          // 160-bit random secret
    algorithm: TotpAlgorithm, // SHA1, SHA256, SHA512
    digits: u32,              // 6 or 8 digits
    period: u32,              // Time step (default 30s)
}

impl TotpSecret {
    pub fn generate() -> Result<Self, MfaError>;
    pub fn to_uri(&self, issuer: &str, account: &str) -> String;
    pub fn to_qr_code(&self, issuer: &str, account: &str) -> Result<Vec<u8>, MfaError>;
}
```

#### 1.2 TOTP検証

```rust
pub struct TotpVerifier {
    time_window: u32, // ±1ステップ許可（デフォルト）
}

impl TotpVerifier {
    pub fn verify(&self, secret: &TotpSecret, code: &str) -> Result<bool, MfaError>;
    pub fn verify_with_timestamp(&self, secret: &TotpSecret, code: &str, timestamp: u64) -> Result<bool, MfaError>;
}
```

#### 1.3 QRコード生成

- `qrcode`クレートを使用してQRコード生成
- otpauth:// URI形式を生成
- PNG/SVG出力形式をサポート

### データ構造

```rust
pub enum TotpAlgorithm {
    Sha1,
    Sha256,
    Sha512,
}

pub struct TotpConfig {
    enabled: bool,
    algorithm: TotpAlgorithm,
    digits: u32,
    period: u32,
    time_window: u32,
}
```

### Phase 1 セキュリティ考慮事項

- シークレット生成には暗号学的に安全な乱数生成器を使用
- タイミング攻撃を防ぐため定数時間比較を実装
- シークレットは暗号化して保存（AES-GCM-256）
- 検証試行回数の制限（5分間に5回まで）

## Phase 2: バックアップコード(2日間)

### Phase 2 コンポーネント

#### 2.1 バックアップコード生成

```rust
pub struct BackupCodeGenerator {
    code_length: usize,  // 8 characters
    code_count: usize,   // 10 codes
}

impl BackupCodeGenerator {
    pub fn generate_codes(&self) -> Result<Vec<String>, MfaError>;
    pub fn format_code(code: &str) -> String; // XXXX-XXXX format
}
```

#### 2.2 バックアップコードマネージャー

```rust
pub struct BackupCodeManager {
    codes: Vec<HashedBackupCode>,
}

pub struct HashedBackupCode {
    hash: String,        // Argon2idハッシュ
    used: bool,
    used_at: Option<DateTime<Utc>>,
}

impl BackupCodeManager {
    pub fn verify_code(&mut self, code: &str) -> Result<bool, MfaError>;
    pub fn regenerate_codes(&mut self) -> Result<Vec<String>, MfaError>;
    pub fn remaining_codes(&self) -> usize;
}
```

### Phase 2 セキュリティ要件

- 暗号学的に安全なRNGを使用してコードを生成
- 保存前にArgon2idでコードをハッシュ化
- ワンタイム使用の強制を実装
- 監査ログ付きで再生成を許可

## Phase 3: SMS認証(3日間)

### Phase 3 コンポーネント

#### 3.1 SMSプロバイダーインターフェース

```rust
pub trait SmsProvider: Send + Sync {
    async fn send_code(&self, phone: &str, code: &str) -> Result<(), MfaError>;
    fn provider_name(&self) -> &str;
}

pub struct TwilioSmsProvider {
    account_sid: String,
    auth_token: String,
    from_number: String,
}

pub struct AwsSnsSmsProvider {
    client: aws_sdk_sns::Client,
}
```

#### 3.2 SMS検証

```rust
pub struct SmsVerifier {
    provider: Box<dyn SmsProvider>,
    code_expiry: Duration,      // 5分
    rate_limit: Duration,        // 送信間隔1分
}

impl SmsVerifier {
    pub async fn send_code(&self, phone: &str) -> Result<String, MfaError>;
    pub fn verify_code(&self, session_id: &str, code: &str) -> Result<bool, MfaError>;
}
```

### Phase 3 データ構造

```rust
pub struct SmsSession {
    enabled: bool,
    provider: SmsProviderType,
    rate_limit_seconds: u32,
    code_expiry_seconds: u32,
    max_attempts: u32,
}

pub enum SmsProviderType {
    Twilio,
    AwsSns,
    Mock, // For testing
}
```

### Phase 3 セキュリティ・コスト考慮事項

- 厳格なレート制限を実装（ユーザーあたり1時間に最大3通のSMS）
- SMSコストを追跡し、予算アラートを実装
- 送信前に電話番号を検証
- 6桁の数字コードを使用
- 5分後にコードを期限切れにする
- すべてのSMS送信試行をログに記録

## Phase 4: デバイス信頼(3日間)

### Phase 4 コンポーネント

#### 4.1 デバイスフィンガープリント

```rust
pub struct DeviceFingerprint {
    user_agent: String,
    ip_address: String,
    accept_language: String,
    screen_resolution: Option<String>,
    timezone: Option<String>,
    hash: String,  // SHA-256 hash of combined data
}

impl DeviceFingerprint {
    pub fn from_request(req: &HttpRequest) -> Self;
    pub fn calculate_hash(&self) -> String;
}
```

#### 4.2 デバイス信頼マネージャー

```rust
pub struct DeviceTrustManager {
    trusted_devices: HashMap<String, TrustedDevice>,
}

pub struct TrustedDevice {
    device_id: String,
    fingerprint: DeviceFingerprint,
    trust_score: f32,        // 0.0 - 1.0
    first_seen: DateTime<Utc>,
    last_seen: DateTime<Utc>,
    login_count: u32,
}

impl DeviceTrustManager {
    pub fn evaluate_trust(&self, fingerprint: &DeviceFingerprint) -> f32;
    pub fn add_trusted_device(&mut self, fingerprint: DeviceFingerprint);
    pub fn is_trusted(&self, fingerprint: &DeviceFingerprint, threshold: f32) -> bool;
}
```

### 信頼スコアリングアルゴリズム

```rust
fn calculate_trust_score(device: &TrustedDevice) -> f32 {
    let age_score = min(device.login_count as f32 / 10.0, 1.0) * 0.4;
    let recency_score = if device.last_seen > now - 7.days() { 0.3 } else { 0.0 };
    let consistency_score = 0.3; // Based on fingerprint consistency
    
    age_score + recency_score + consistency_score
}
```

### Phase 4 設定

```rust
pub struct DeviceTrustConfig {
    enabled: bool,
    trust_threshold: f32,      // デフォルト0.7
    learning_period_days: u32, // 7日間
    max_trusted_devices: u32,  // 5デバイス
}
```

## Phase 5: セッション統合(2日間)

### Phase 5 コンポーネント

#### 5.1 MFAセッション拡張

```rust
pub struct MfaSession {
    session_id: String,
    user_id: String,
    mfa_verified: bool,
    verified_at: Option<DateTime<Utc>>,
    method: Option<MfaMethod>,
    device_trusted: bool,
}

pub enum MfaMethod {
    Totp,
    Sms,
    BackupCode,
}
```

#### 5.2 デバイス記憶機能

```rust
pub struct RememberDeviceToken {
    device_id: String,
    user_id: String,
    expires_at: DateTime<Utc>,
    encrypted_token: String,
}

impl RememberDeviceToken {
    pub fn generate(user_id: &str, device_id: &str, duration: Duration) -> Self;
    pub fn validate(&self) -> Result<bool, MfaError>;
}
```

### 統合ポイント

- 既存のセッション管理を拡張
- セッションにMFA検証フラグを追加
- 「このデバイスを30日間信頼する」機能を実装
- 閾値を超える信頼されたデバイスのMFAスキップをサポート

## Phase 6: コアMFAコーディネーター

### メインコーディネーター

```rust
pub struct MultiFactorAuth {
    config: MfaConfig,
    totp_verifier: TotpVerifier,
    backup_manager: BackupCodeManager,
    sms_verifier: Option<SmsVerifier>,
    device_trust: DeviceTrustManager,
}

impl MultiFactorAuth {
    pub fn new(config: MfaConfig) -> Self;
    
    // TOTP
    pub fn generate_totp_secret(&self, user_id: &str) -> Result<(TotpSecret, Vec<u8>), MfaError>;
    pub fn verify_totp(&self, user_id: &str, code: &str) -> Result<bool, MfaError>;
    
    // Backup Codes
    pub fn generate_backup_codes(&mut self, user_id: &str) -> Result<Vec<String>, MfaError>;
    pub fn verify_backup_code(&mut self, user_id: &str, code: &str) -> Result<bool, MfaError>;
    
    // SMS
    pub async fn send_sms_code(&self, user_id: &str, phone: &str) -> Result<String, MfaError>;
    pub fn verify_sms_code(&self, session_id: &str, code: &str) -> Result<bool, MfaError>;
    
    // Device Trust
    pub fn evaluate_device(&self, fingerprint: &DeviceFingerprint) -> f32;
    pub fn should_require_mfa(&self, user_id: &str, fingerprint: &DeviceFingerprint) -> bool;
}
```

### Phase 6 設定

```rust
pub struct MfaConfig {
    enabled: bool,
    required_for_all: bool,
    required_roles: Vec<String>,
    totp: TotpConfig,
    sms: SmsConfig,
    backup_codes: BackupCodeConfig,
    device_trust: DeviceTrustConfig,
}

pub struct BackupCodeConfig {
    enabled: bool,
    code_length: usize,
    code_count: usize,
}
```

## エラーハンドリング

```rust
#[derive(Debug, thiserror::Error)]
pub enum MfaError {
    #[error("Invalid MFA code")]
    InvalidCode,
    
    #[error("MFA code expired")]
    CodeExpired,
    
    #[error("Too many verification attempts")]
    TooManyAttempts,
    
    #[error("MFA not configured for user")]
    NotConfigured,
    
    #[error("SMS sending failed: {0}")]
    SmsSendFailed(String),
    
    #[error("QR code generation failed: {0}")]
    QrCodeError(String),
    
    #[error("Cryptographic error: {0}")]
    CryptoError(String),
    
    #[error("Configuration error: {0}")]
    ConfigError(String),
}
```

## 依存関係

### Cargo.toml追加項目

```toml
[dependencies]
# TOTP
totp-rs = "5.6"          # TOTP generation and verification
base32 = "0.5"           # Base32 encoding for secrets

# QR Code
qrcode = "0.14"          # QR code generation

# SMS Providers (optional features)
twilio = { version = "0.16", optional = true }
aws-sdk-sns = { version = "1.0", optional = true }

# Crypto
argon2 = "0.5"           # Password hashing for backup codes
rand = "0.8"             # Cryptographically secure RNG

[features]
sms-twilio = ["twilio"]
sms-aws = ["aws-sdk-sns"]
```

## テスト戦略

### 単体テスト

- TOTP生成と検証
- バックアップコード生成と検証
- デバイスフィンガープリント精度
- 信頼スコア計算
- レート制限の強制

### 統合テスト

- 完全なMFAフロー（登録→検証）
- SMS送信と検証（モック使用）
- セッション統合
- デバイス信頼学習

### セキュリティテスト

- タイミング攻撃耐性
- ブルートフォース保護
- コード再利用防止
- トークン有効期限

### パフォーマンステスト

- TOTP検証 < 50ms
- QRコード生成 < 100ms
- デバイスフィンガープリント計算 < 10ms

## セキュリティ要件

1. **シークレット保存**
   - すべてのTOTPシークレットをAES-GCM-256で暗号化
   - 安全な鍵導出を使用（PBKDF2またはArgon2）
   - 暗号化キーを定期的にローテーション

2. **ブルートフォース保護**
   - レート制限: ユーザーあたり5分間に5回まで
   - 指数バックオフを実装
   - 10回連続失敗後にアカウントをロック

3. **タイミング攻撃防止**
   - コードの定数時間比較を使用
   - 検証にランダム遅延を追加

4. **監査ログ**
   - すべてのMFAイベントをログに記録（セットアップ、検証、失敗）
   - ログにデバイスフィンガープリントを含める
   - バックアップコード使用を追跡

5. **OWASPコンプライアンス**
   - OWASP認証チートシートに従う
   - 安全なセッション管理を実装
   - 安全な乱数生成を使用

## 設定例

```toml
[security.mfa]
enabled = true
required_for_all = false
required_roles = ["admin", "developer"]

[security.mfa.totp]
enabled = true
algorithm = "SHA256"
digits = 6
period = 30
time_window = 1

[security.mfa.sms]
enabled = false
provider = "twilio"
rate_limit_seconds = 60
code_expiry_seconds = 300
max_attempts = 3

[security.mfa.backup_codes]
enabled = true
code_length = 8
code_count = 10

[security.mfa.device_trust]
enabled = true
trust_threshold = 0.7
learning_period_days = 7
max_trusted_devices = 5
```

## API使用例

### 登録フロー

```rust
// TOTPシークレットとQRコードを生成
let mfa = MultiFactorAuth::new(config);
let (secret, qr_code) = mfa.generate_totp_secret("user@example.com")?;

// ユーザーにQRコードを表示
display_qr_code(&qr_code);

// バックアップコードを生成
let backup_codes = mfa.generate_backup_codes("user@example.com")?;
display_backup_codes(&backup_codes);
```

### ログインフロー

```rust
// MFAが必要かチェック
let fingerprint = DeviceFingerprint::from_request(&req);
if mfa.should_require_mfa(user_id, &fingerprint) {
    // TOTPコードを検証
    let is_valid = mfa.verify_totp(user_id, &user_input_code)?;
    
    if is_valid {
        // セッションを更新
        session.mfa_verified = true;
        session.verified_at = Some(Utc::now());
        
        // オプションでデバイスを信頼
        if remember_device {
            mfa.add_trusted_device(fingerprint);
        }
    }
}
```

### バックアップコード回復

```rust
// ユーザーがTOTPデバイスを紛失
let is_valid = mfa.verify_backup_code(user_id, &backup_code)?;

if is_valid {
    // アクセスを許可し、新しいTOTPセットアップを促す
    session.mfa_verified = true;
    prompt_totp_setup();
}
```

## 成功基準

- [x] TOTP検証成功率 > 99.9%
- [x] 検証処理時間 < 100ms
- [x] SMS送信成功率 > 95%（有効時）
- [x] セキュリティ脆弱性ゼロ（OWASP基準）
- [x] テストカバレッジ > 85%
- [x] 完全なドキュメント
- [x] 本番環境対応のエラーハンドリング

## ドキュメント成果物

1. **APIドキュメント**
   - すべてのパブリックAPI用の完全なrustdoc
   - 各コンポーネントの使用例

2. **ユーザーガイド**
   - MFAセットアップ手順
   - バックアップコード使用方法
   - デバイス信頼の説明

3. **管理者ガイド**
   - 設定オプション
   - SMSプロバイダーセットアップ
   - セキュリティベストプラクティス

4. **トラブルシューティングガイド**
   - 一般的な問題と解決策
   - デバッグログ
   - パフォーマンスチューニング

## 移行計画

1. オプション機能としてMFAを追加（デフォルトで無効）
2. 特定のユーザーロールへ段階的にロールアウト
3. 採用率と失敗率を監視
4. 検証期間後に全体的に有効化

## 将来の機能拡張

- WebAuthn/FIDO2サポート
- メールベース検証
- プッシュ通知検証
- リスクスコアリングに基づく適応型MFA
- MFA監視用管理者ダッシュボード

---

**関連Issue**: #75, #84  
**優先度**: P0（Critical）  
**完了予定**: 2-3週間
