# セキュアプラグインアーキテクチャ設計提案

## 🚨 現在の設計の脆弱性分析

### 1. セキュリティ境界の問題
```
┌─────────────────────────────────────┐
│ 現在の設計（危険）                    │
├─────────────────────────────────────┤
│ ┌─ MCP-RS Core ─────────────────┐   │
│ │ ┌─ Plugin A ─┐ ┌─ Plugin B ─┐ │   │
│ │ │ 悪意コード  │ │ 正常コード  │ │   │
│ │ │ ↓直接実行  │ │           │ │   │
│ │ └───────────┘ └───────────┘ │   │
│ │ ← セキュリティ境界なし →      │   │
│ └─────────────────────────────────┘   │
└─────────────────────────────────────┘
```

### 2. 具体的なリスク
- **メモリ汚染**: プラグインが本体のメモリ空間にアクセス可能
- **権限昇格**: プラグインが本体と同等の権限で実行
- **データ漏洩**: 他のプラグインや本体の機密情報にアクセス可能
- **システム破壊**: プラグインがシステム全体を停止可能

## 🛡️ 提案：物理分離型セキュアアーキテクチャ

### 1. マルチサーバー境界分離設計
```
┌─────────────────────────────────────────────────────────┐
│ 物理的セキュリティ境界                                     │
├─────────────────────────────────────────────────────────┤
│                                                         │
│ ┌─ MCP Core Server (Protected) ─┐   Network Boundary   │
│ │ - 認証・認可                   │        │              │
│ │ - セキュリティ検証             │        │              │
│ │ - レート制限                   │        │              │
│ │ - 監査ログ                     │        ▼              │
│ │ - リクエスト配信               │ ┌─ Plugin Servers ─┐  │
│ └─────────────────────────────┘ │ │ ┌─ Plugin A ─┐  │  │
│           ▲                       │ │ │ 隔離実行   │  │  │
│           │ gRPC/HTTP API         │ │ └───────────┘  │  │
│           │                       │ │ ┌─ Plugin B ─┐  │  │
│ ┌─ API Gateway ─────────────────┐ │ │ │ 隔離実行   │  │  │
│ │ - TLS終端                     │ │ │ └───────────┘  │  │
│ │ - 認証トークン検証             │ │ └─────────────────┘  │
│ │ - リクエストルーティング       │ └─────────────────────┘  │
│ └─────────────────────────────┘                          │
│           ▲                                              │
│           │ HTTPS                                        │
│           │                                              │
│ ┌─ Client Applications ─────────┐                        │
│ │ - Claude, ChatGPT等          │                        │
│ │ - カスタムクライアント         │                        │
│ └─────────────────────────────┘                        │
└─────────────────────────────────────────────────────────┘
```

### 2. セキュリティレイヤー設計

#### Layer 1: Core Server（完全保護）
```rust
// src/core/secure_server.rs
use std::collections::HashMap;
use tokio::sync::RwLock;

/// 完全に保護されたコアサーバー
pub struct SecureCoreServer {
    /// プラグインサーバーの接続情報（読み取り専用）
    plugin_endpoints: RwLock<HashMap<String, PluginEndpoint>>,
    /// セキュリティポリシーエンジン
    security_engine: SecurityPolicyEngine,
    /// 監査ログ（改ざん防止）
    audit_logger: TamperProofAuditLogger,
}

#[derive(Clone)]
pub struct PluginEndpoint {
    /// プラグインサーバーのURL
    pub url: String,
    /// 認証トークン（暗号化）
    pub auth_token: EncryptedToken,
    /// 許可された操作リスト
    pub allowed_operations: Vec<String>,
    /// レート制限設定
    pub rate_limits: RateLimitConfig,
    /// ヘルスチェック状態
    pub health_status: HealthStatus,
}

impl SecureCoreServer {
    /// プラグインへの安全なリクエスト転送
    pub async fn forward_request_to_plugin(
        &self,
        plugin_name: &str,
        request: SecureRequest,
    ) -> Result<SecureResponse, SecurityError> {
        // 1. セキュリティ検証
        self.security_engine.validate_request(&request).await?;
        
        // 2. プラグインエンドポイント取得
        let endpoint = self.get_plugin_endpoint(plugin_name).await?;
        
        // 3. レート制限チェック
        self.security_engine.check_rate_limit(&endpoint, &request).await?;
        
        // 4. リクエスト暗号化
        let encrypted_request = self.encrypt_request(request, &endpoint).await?;
        
        // 5. プラグインサーバーへHTTPS転送
        let response = self.send_to_plugin(&endpoint, encrypted_request).await?;
        
        // 6. レスポンス検証・復号化
        let validated_response = self.validate_and_decrypt_response(response).await?;
        
        // 7. 監査ログ記録
        self.audit_logger.log_plugin_interaction(plugin_name, &validated_response).await?;
        
        Ok(validated_response)
    }
}
```

#### Layer 2: Plugin Server（隔離実行）
```rust
// src/plugins/isolated_server.rs

/// 隔離されたプラグインサーバー
pub struct IsolatedPluginServer {
    /// プラグイン実装
    plugin: Box<dyn IsolatedPlugin>,
    /// セキュリティサンドボックス
    sandbox: SecuritySandbox,
    /// リソース制限
    resource_limits: ResourceLimits,
}

pub trait IsolatedPlugin: Send + Sync {
    /// 隔離環境での初期化
    async fn initialize_isolated(&self, config: SandboxConfig) -> Result<(), PluginError>;
    
    /// サンドボックス内でのツール実行
    async fn execute_tool_sandboxed(
        &self,
        tool_name: &str,
        parameters: SanitizedParameters,
    ) -> Result<SanitizedResponse, PluginError>;
}

#[derive(Debug)]
pub struct SecuritySandbox {
    /// メモリ制限（MB）
    pub max_memory_mb: u64,
    /// CPU制限（%）
    pub max_cpu_percent: u8,
    /// ネットワークアクセス制限
    pub network_restrictions: NetworkPolicy,
    /// ファイルシステムアクセス制限
    pub filesystem_restrictions: FilesystemPolicy,
    /// 実行時間制限（秒）
    pub execution_timeout_seconds: u64,
}

impl IsolatedPluginServer {
    /// サンドボックス内でプラグインを実行
    pub async fn execute_in_sandbox(
        &self,
        request: SanitizedRequest,
    ) -> Result<SanitizedResponse, PluginError> {
        // 1. リソース制限設定
        self.sandbox.apply_resource_limits().await?;
        
        // 2. ネットワーク制限適用
        self.sandbox.restrict_network_access().await?;
        
        // 3. ファイルシステム制限適用
        self.sandbox.restrict_filesystem_access().await?;
        
        // 4. タイムアウト設定
        let execution_future = self.plugin.execute_tool_sandboxed(
            &request.tool_name,
            request.parameters,
        );
        
        // 5. タイムアウト付き実行
        match tokio::time::timeout(
            Duration::from_secs(self.resource_limits.execution_timeout_seconds),
            execution_future,
        ).await {
            Ok(result) => result,
            Err(_) => Err(PluginError::ExecutionTimeout),
        }
    }
}
```

### 3. セキュリティ通信プロトコル

#### mTLS + JWT認証
```rust
// src/security/plugin_auth.rs

/// プラグイン間通信の認証システム
pub struct PluginAuthSystem {
    /// コアサーバーの証明書（CA）
    core_ca_cert: X509Certificate,
    /// プラグイン証明書の検証
    plugin_cert_validator: CertificateValidator,
    /// JWT トークン管理
    jwt_manager: JwtTokenManager,
}

impl PluginAuthSystem {
    /// プラグインサーバーとの安全な接続確立
    pub async fn establish_secure_connection(
        &self,
        plugin_endpoint: &PluginEndpoint,
    ) -> Result<SecureConnection, AuthError> {
        // 1. mTLS接続確立
        let tls_connection = self.create_mtls_connection(plugin_endpoint).await?;
        
        // 2. プラグイン証明書検証
        self.plugin_cert_validator.verify_plugin_certificate(
            &tls_connection.peer_certificate()
        ).await?;
        
        // 3. JWT トークン交換
        let jwt_token = self.jwt_manager.create_plugin_token(
            &plugin_endpoint.plugin_id,
            &plugin_endpoint.allowed_operations,
        ).await?;
        
        // 4. セキュアチャネル確立
        Ok(SecureConnection {
            tls_connection,
            jwt_token,
            plugin_id: plugin_endpoint.plugin_id.clone(),
        })
    }
}
```

### 4. 設定ファイル（分離型）

#### Core Server設定
```toml
# mcp-core-config.toml
[core_server]
bind_addr = "127.0.0.1:8080"
tls_cert_path = "/etc/mcp/certs/core-server.crt"
tls_key_path = "/etc/mcp/certs/core-server.key"
ca_cert_path = "/etc/mcp/certs/ca.crt"

[security]
max_request_size_mb = 10
request_timeout_seconds = 30
rate_limit_per_minute = 100

[audit]
log_level = "INFO"
log_file = "/var/log/mcp/core-audit.log"
tamper_protection = true

[plugins]
discovery_timeout_seconds = 10
health_check_interval_seconds = 30

# プラグインサーバーの接続情報
[[plugins.servers]]
name = "wordpress-handler"
url = "https://plugin-wordpress:8443"
cert_path = "/etc/mcp/certs/wordpress-plugin.crt"
allowed_operations = ["list_posts", "create_post", "update_post"]
max_requests_per_minute = 50

[[plugins.servers]]
name = "github-handler"
url = "https://plugin-github:8444"
cert_path = "/etc/mcp/certs/github-plugin.crt"
allowed_operations = ["list_repos", "create_issue"]
max_requests_per_minute = 30
```

#### Plugin Server設定
```toml
# wordpress-plugin-config.toml
[plugin_server]
name = "wordpress-handler"
bind_addr = "0.0.0.0:8443"
tls_cert_path = "/etc/plugin/certs/wordpress.crt"
tls_key_path = "/etc/plugin/certs/wordpress.key"
core_server_url = "https://mcp-core:8080"

[sandbox]
max_memory_mb = 512
max_cpu_percent = 50
execution_timeout_seconds = 10

[network_policy]
allowed_domains = ["wordpress.example.com"]
blocked_domains = ["*"]
allow_localhost = false

[filesystem_policy]
read_only_paths = ["/app/config"]
writable_paths = ["/tmp/plugin-cache"]
blocked_paths = ["/", "/etc", "/var"]

[wordpress]
url = "https://wordpress.example.com"
# 認証情報は環境変数または秘密管理システムから取得
```

### 5. Dockerコンテナ分離

#### Core Server（最小権限）
```dockerfile
# Dockerfile.core
FROM scratch
COPY mcp-core /usr/local/bin/
COPY ca-certificates.crt /etc/ssl/certs/

# 読み取り専用ルートファイルシステム
VOLUME ["/var/log/mcp", "/etc/mcp/certs"]

USER 65534:65534  # nobody user
EXPOSE 8080

ENTRYPOINT ["/usr/local/bin/mcp-core"]
```

#### Plugin Server（サンドボックス）
```dockerfile
# Dockerfile.plugin
FROM alpine:3.18
RUN adduser -D -s /bin/sh plugin

# 最小限のツールのみ
RUN apk add --no-cache ca-certificates

COPY wordpress-plugin /usr/local/bin/
COPY plugin-config.toml /etc/plugin/

# セキュリティ強化
RUN chmod 500 /usr/local/bin/wordpress-plugin
RUN chown plugin:plugin /usr/local/bin/wordpress-plugin

USER plugin:plugin
EXPOSE 8443

# リソース制限
ENTRYPOINT ["/usr/local/bin/wordpress-plugin"]
```

### 6. Kubernetes分離配置

```yaml
# k8s-secure-deployment.yaml
apiVersion: v1
kind: Namespace
metadata:
  name: mcp-secure
---
# Core Server（保護されたnamespace）
apiVersion: apps/v1
kind: Deployment
metadata:
  name: mcp-core
  namespace: mcp-secure
spec:
  template:
    spec:
      securityContext:
        runAsNonRoot: true
        runAsUser: 65534
        fsGroup: 65534
      containers:
      - name: mcp-core
        image: mcp-core:latest
        securityContext:
          allowPrivilegeEscalation: false
          readOnlyRootFilesystem: true
          capabilities:
            drop: ["ALL"]
        resources:
          requests:
            memory: "256Mi"
            cpu: "250m"
          limits:
            memory: "512Mi"
            cpu: "500m"
---
# WordPress Plugin（隔離されたnamespace）
apiVersion: v1
kind: Namespace
metadata:
  name: mcp-plugins-wordpress
---
apiVersion: apps/v1
kind: Deployment
metadata:
  name: wordpress-plugin
  namespace: mcp-plugins-wordpress
spec:
  template:
    spec:
      securityContext:
        runAsNonRoot: true
        runAsUser: 1000
      containers:
      - name: wordpress-plugin
        image: wordpress-plugin:latest
        securityContext:
          allowPrivilegeEscalation: false
          readOnlyRootFilesystem: true
          capabilities:
            drop: ["ALL"]
        resources:
          requests:
            memory: "128Mi"
            cpu: "100m"
          limits:
            memory: "512Mi"    # サンドボックス制限
            cpu: "200m"
---
# ネットワークポリシー（通信制限）
apiVersion: networking.k8s.io/v1
kind: NetworkPolicy
metadata:
  name: core-server-isolation
  namespace: mcp-secure
spec:
  podSelector:
    matchLabels:
      app: mcp-core
  policyTypes:
  - Ingress
  - Egress
  ingress:
  - from:
    - namespaceSelector:
        matchLabels:
          name: api-gateway
  egress:
  - to:
    - namespaceSelector:
        matchLabels:
          name: mcp-plugins-wordpress
    ports:
    - protocol: TCP
      port: 8443
---
apiVersion: networking.k8s.io/v1
kind: NetworkPolicy
metadata:
  name: plugin-isolation
  namespace: mcp-plugins-wordpress
spec:
  podSelector:
    matchLabels:
      app: wordpress-plugin
  policyTypes:
  - Ingress
  - Egress
  ingress:
  - from:
    - namespaceSelector:
        matchLabels:
          name: mcp-secure
  egress:
  - to: []  # 外部通信は明示的に許可された宛先のみ
    ports:
    - protocol: TCP
      port: 443  # WordPress API用HTTPS
```

## 🔒 セキュリティ効果

### 1. 物理的境界による完全分離
- **プロセス分離**: 各プラグインが独立したプロセス/コンテナで実行
- **メモリ分離**: プラグイン間でメモリ空間を共有しない
- **ネットワーク分離**: 明示的に許可された通信のみ

### 2. 多層防御
- **認証**: mTLS + JWT による強固な認証
- **認可**: 操作レベルでの細かい権限制御
- **監査**: すべての通信を改ざん防止ログに記録
- **サンドボックス**: リソース制限によるDoS攻撃防止

### 3. 障害分離
- **単一障害点の排除**: 1つのプラグインの障害が全体に影響しない
- **段階的復旧**: 障害プラグインのみを切り離して運用継続
- **ヘルスモニタリング**: リアルタイムでプラグインの状態監視

## 📋 実装優先順位

### Phase 1: 基盤設計（2週間）
1. セキュア通信プロトコルの実装
2. プラグインサーバーのベースフレームワーク
3. 基本的なサンドボックス機能

### Phase 2: セキュリティ強化（2週間）
1. mTLS認証システム
2. リソース制限とタイムアウト
3. 監査ログシステム

### Phase 3: 運用機能（1週間）
1. ヘルスチェックとモニタリング
2. 設定管理システム
3. デプロイメント自動化

この設計により、悪意のあるプラグインが本体に与える影響を完全に排除し、企業レベルのセキュリティ要件を満たすことができます。