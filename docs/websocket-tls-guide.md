# WebSocket TLS/WSS Support

WebSocket TLS/WSS機能の実装ガイドとリファレンス

## 概要

MCP-RSのWebSocketトランスポートは、TLS (Transport Layer Security) による安全な通信をサポートしています。WSS (WebSocket Secure) プロトコルを使用することで、通信データの暗号化と認証を実現します。

## 主な機能

- **サーバー側TLS**: PEM形式の証明書と秘密鍵を使用したTLSサーバー
- **クライアント側TLS**: サーバー証明書の検証を含むTLSクライアント
- **自己署名証明書サポート**: テスト環境での自己署名証明書の使用
- **CA証明書**: カスタムCA証明書による検証
- **柔軟な設定**: 本番環境とテスト環境で異なる設定をサポート

## クイックスタート

### 基本的なTLS設定

```rust
use mcp_rs::transport::websocket::{WebSocketConfig, TlsConfig};
use std::path::PathBuf;

// TLS設定
let tls_config = TlsConfig {
    cert_path: Some(PathBuf::from("/etc/ssl/certs/server.crt")),
    key_path: Some(PathBuf::from("/etc/ssl/private/server.key")),
    ca_cert_path: None,
    verify_server: true,
    accept_invalid_certs: false,
};

// WebSocket設定
let ws_config = WebSocketConfig {
    url: "wss://example.com:8443".to_string(),
    server_mode: false, // クライアントモード
    use_tls: true,
    tls_config: Some(tls_config),
    ..Default::default()
};
```

## 設定ガイド

### TlsConfig構造

```rust
pub struct TlsConfig {
    /// 証明書ファイルパス (PEM形式)
    pub cert_path: Option<PathBuf>,
    
    /// 秘密鍵ファイルパス (PEM形式)
    pub key_path: Option<PathBuf>,
    
    /// CA証明書パス (クライアント検証用)
    pub ca_cert_path: Option<PathBuf>,
    
    /// サーバー証明書を検証するか (クライアントモード)
    pub verify_server: bool,
    
    /// 無効な証明書を受け入れるか (テスト専用)
    pub accept_invalid_certs: bool,
}
```

### フィールド詳細

#### cert_path (サーバーモード必須)
サーバー証明書ファイルのパス。PEM形式である必要があります。

**例:**
```rust
cert_path: Some(PathBuf::from("/etc/ssl/certs/server.crt"))
```

#### key_path (サーバーモード必須)
秘密鍵ファイルのパス。PEM形式で、証明書と対応している必要があります。

**例:**
```rust
key_path: Some(PathBuf::from("/etc/ssl/private/server.key"))
```

#### ca_cert_path (オプション)
カスタムCA証明書のパス。クライアント証明書の検証や、特定のCAを信頼する場合に使用します。

**例:**
```rust
ca_cert_path: Some(PathBuf::from("/etc/ssl/certs/custom-ca.crt"))
```

#### verify_server (クライアントモード)
クライアントがサーバー証明書を検証するかどうか。

- `true` (推奨): サーバー証明書を検証
- `false` (テスト専用): 検証をスキップ

**セキュリティ警告**: 本番環境では必ず `true` に設定してください。

#### accept_invalid_certs (テスト専用)
無効な証明書（自己署名、期限切れなど）を受け入れるかどうか。

- `false` (推奨): 無効な証明書を拒否
- `true` (テスト専用): 無効な証明書を受け入れる

**セキュリティ警告**: 本番環境では絶対に `true` にしないでください。

## 使用例

### 例1: 本番環境サーバー (CA署名証明書)

```rust
use mcp_rs::transport::websocket::{WebSocketConfig, TlsConfig, WebSocketTransport};
use mcp_rs::transport::Transport;
use std::path::PathBuf;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // TLS設定 (本番環境)
    let tls_config = TlsConfig {
        cert_path: Some(PathBuf::from("/etc/ssl/certs/server.crt")),
        key_path: Some(PathBuf::from("/etc/ssl/private/server.key")),
        ca_cert_path: Some(PathBuf::from("/etc/ssl/certs/ca-bundle.crt")),
        verify_server: true,  // 本番環境では必須
        accept_invalid_certs: false,  // 本番環境では必須
    };

    // WebSocket設定
    let ws_config = WebSocketConfig {
        url: "0.0.0.0:8443".to_string(),  // すべてのインターフェースでリッスン
        server_mode: true,  // サーバーモード
        use_tls: true,
        tls_config: Some(tls_config),
        max_connections: 1000,
        ..Default::default()
    };

    // トランスポート作成と開始
    let mut transport = WebSocketTransport::new(ws_config)?;
    transport.start().await?;

    println!("TLS WebSocket server started on wss://0.0.0.0:8443");

    Ok(())
}
```

### 例2: 開発環境サーバー (自己署名証明書)

```rust
use mcp_rs::transport::websocket::{WebSocketConfig, TlsConfig, WebSocketTransport};
use mcp_rs::transport::Transport;
use std::path::PathBuf;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // TLS設定 (開発環境 - 自己署名証明書)
    let tls_config = TlsConfig {
        cert_path: Some(PathBuf::from("./dev-certs/localhost.crt")),
        key_path: Some(PathBuf::from("./dev-certs/localhost.key")),
        ca_cert_path: None,  // 自己署名証明書ではCA不要
        verify_server: false,  // 開発環境では無効化可能
        accept_invalid_certs: true,  // 自己署名証明書を許可
    };

    // WebSocket設定
    let ws_config = WebSocketConfig {
        url: "127.0.0.1:8443".to_string(),  // ローカルのみ
        server_mode: true,
        use_tls: true,
        tls_config: Some(tls_config),
        ..Default::default()
    };

    let mut transport = WebSocketTransport::new(ws_config)?;
    transport.start().await?;

    println!("Development TLS WebSocket server started on wss://127.0.0.1:8443");
    println!("WARNING: Using self-signed certificate - for development only!");

    Ok(())
}
```

### 例3: TLSクライアント (CA検証あり)

```rust
use mcp_rs::transport::websocket::{WebSocketConfig, TlsConfig, WebSocketTransport};
use mcp_rs::transport::Transport;
use std::path::PathBuf;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // TLS設定 (クライアント)
    let tls_config = TlsConfig {
        cert_path: None,  // クライアントは証明書不要
        key_path: None,   // クライアントは秘密鍵不要
        ca_cert_path: Some(PathBuf::from("/etc/ssl/certs/ca-bundle.crt")),
        verify_server: true,  // サーバー証明書を検証
        accept_invalid_certs: false,
    };

    // WebSocket設定
    let ws_config = WebSocketConfig {
        url: "wss://secure.example.com:8443".to_string(),
        server_mode: false,  // クライアントモード
        use_tls: true,
        tls_config: Some(tls_config),
        ..Default::default()
    };

    let mut transport = WebSocketTransport::new(ws_config)?;
    transport.start().await?;

    println!("Connected to TLS WebSocket server");

    Ok(())
}
```

### 例4: テスト用クライアント (自己署名証明書許可)

```rust
use mcp_rs::transport::websocket::{WebSocketConfig, TlsConfig, WebSocketTransport};
use mcp_rs::transport::Transport;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // TLS設定 (テスト用クライアント)
    let tls_config = TlsConfig {
        cert_path: None,
        key_path: None,
        ca_cert_path: None,
        verify_server: false,  // テスト環境では検証を無効化
        accept_invalid_certs: true,  // 自己署名証明書を許可
    };

    // WebSocket設定
    let ws_config = WebSocketConfig {
        url: "wss://localhost:8443".to_string(),
        server_mode: false,
        use_tls: true,
        tls_config: Some(tls_config),
        timeout_seconds: Some(10),
        ..Default::default()
    };

    let mut transport = WebSocketTransport::new(ws_config)?;
    transport.start().await?;

    println!("Connected to test TLS WebSocket server");
    println!("WARNING: Certificate verification disabled - for testing only!");

    Ok(())
}
```

## 証明書の生成

### 開発用自己署名証明書の作成

OpenSSLを使用して開発用の自己署名証明書を生成できます。

#### ステップ1: 秘密鍵の生成

```bash
openssl genrsa -out localhost.key 2048
```

#### ステップ2: 証明書署名要求(CSR)の作成

```bash
openssl req -new -key localhost.key -out localhost.csr
```

対話形式で以下の情報を入力します:
- Country Name: JP
- State: Tokyo
- Locality: Tokyo
- Organization Name: Development
- Common Name: localhost (重要!)
- Email Address: (空白でOK)

#### ステップ3: 自己署名証明書の生成

```bash
openssl x509 -req -days 365 -in localhost.csr -signkey localhost.key -out localhost.crt
```

#### ステップ4: ファイルの確認

生成されたファイル:
- `localhost.key`: 秘密鍵 (厳重に管理)
- `localhost.crt`: 証明書
- `localhost.csr`: CSR (削除可能)

### 本番環境用証明書

本番環境では、信頼されたCA (Certificate Authority) から証明書を取得してください:

- **Let's Encrypt**: 無料の自動化CA
- **DigiCert**: 商用CA
- **GlobalSign**: グローバルCA
- その他の認証局

## セキュリティのベストプラクティス

### 1. 証明書の管理

✅ **推奨**:
- 秘密鍵のパーミッションを600に設定
- 証明書を定期的に更新 (最低でも年1回)
- 証明書の有効期限を監視

❌ **避けるべき**:
- 秘密鍵をバージョン管理にコミット
- 同じ証明書を複数の環境で使用
- 期限切れの証明書を使用

### 2. 本番環境の設定

✅ **必須**:
```rust
verify_server: true,         // 常にtrue
accept_invalid_certs: false, // 常にfalse
```

### 3. テスト環境の設定

⚠️ **テスト専用**:
```rust
verify_server: false,        // テストのみ許可
accept_invalid_certs: true,  // テストのみ許可
```

**重要**: テスト用設定を本番環境で使用しないでください。

### 4. ファイルパーミッション

秘密鍵ファイルは厳格なパーミッションで保護:

```bash
# 秘密鍵: オーナーのみ読み取り可能
chmod 600 /etc/ssl/private/server.key

# 証明書: 読み取り可能
chmod 644 /etc/ssl/certs/server.crt
```

## トラブルシューティング

### エラー: "Failed to read certificate"

**原因**: 証明書ファイルが見つからない、または読み取り権限がない

**解決策**:
1. ファイルパスが正しいか確認
2. ファイルのパーミッションを確認
3. ファイルが存在するか確認

```rust
let cert_path = PathBuf::from("/etc/ssl/certs/server.crt");
assert!(cert_path.exists(), "Certificate file not found");
```

### エラー: "Failed to parse certificate"

**原因**: 証明書がPEM形式でない、または破損している

**解決策**:
1. 証明書がPEM形式であることを確認
2. ファイルが `-----BEGIN CERTIFICATE-----` で始まることを確認
3. 証明書を再生成

### エラー: "TLS handshake failed"

**原因**: 証明書と秘密鍵が一致しない、またはクライアントが証明書を信頼しない

**解決策**:
1. 証明書と秘密鍵のペアを確認
2. クライアント側のCA証明書を確認
3. テスト環境では `accept_invalid_certs: true` を試す

### エラー: "Connection timeout"

**原因**: TLSハンドシェイクに時間がかかりすぎている

**解決策**:
```rust
timeout_seconds: Some(60),  // タイムアウトを延長
```

## パフォーマンスの考慮事項

### TLSオーバーヘッド

TLS接続は暗号化により若干のオーバーヘッドがあります:

- **初回接続**: ハンドシェイクで+50-100ms
- **データ転送**: 暗号化/復号化で+5-10%
- **CPU使用率**: +10-20%

### 最適化のヒント

1. **接続の再利用**: 頻繁な接続/切断を避ける
2. **適切なハートビート間隔**: 不要な再接続を防ぐ
3. **バッファサイズ**: 大きなメッセージには適切なバッファを設定

```rust
let ws_config = WebSocketConfig {
    heartbeat_interval: 30,  // 30秒
    max_message_size: 16 * 1024 * 1024,  // 16MB
    ..Default::default()
};
```

## 関連ドキュメント

- [WebSocket API リファレンス](../api/transport-websocket.md)
- [セキュリティガイド](../security/tls-best-practices.md)
- [デプロイメントガイド](../deployment/production-setup.md)

## サポート

問題が発生した場合:

1. [GitHub Issues](https://github.com/n-takatsu/mcp-rs/issues)
2. ドキュメント: `docs/`
3. サンプルコード: `examples/`
