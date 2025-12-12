# Kubernetes Operator ガイド

## 概要

MCP Kubernetes Operator は、Kubernetes クラスタ上で MCP サーバーを管理するためのオペレーターです。Custom Resource Definitions (CRD) を使用して、MCPServer、Plugin、SecurityPolicy リソースを宣言的に管理します。

## 目次

- [前提条件](#前提条件)
- [インストール](#インストール)
- [CRD リファレンス](#crdリファレンス)
- [使用例](#使用例)
- [運用ガイド](#運用ガイド)
- [トラブルシューティング](#トラブルシューティング)

## 前提条件

- Kubernetes 1.25+
- kubectl コマンドラインツール
- Helm 3.0+ (Helm インストールを使用する場合)

## インストール

### 方法 1: Helm を使用したインストール

```bash
# CRD をインストール
kubectl apply -f k8s/crds/

# Helm チャートをインストール
helm install mcp-operator charts/mcp-operator \
  --namespace mcp-system \
  --create-namespace
```

### 方法 2: kubectl を使用したインストール

```bash
# CRD をインストール
kubectl apply -f k8s/crds/

# Operator をインストール
kubectl apply -f k8s/manifests/
```

### インストールの確認

```bash
# Operator Pod が Running 状態であることを確認
kubectl get pods -n mcp-system

# CRD が登録されていることを確認
kubectl get crd | grep mcp.n-takatsu.dev
```

## CRD リファレンス

### MCPServer

MCPServer リソースは、MCP サーバーのデプロイメントを定義します。

```yaml
apiVersion: mcp.n-takatsu.dev/v1alpha1
kind: MCPServer
metadata:
  name: example-mcpserver
  namespace: default
spec:
  replicas: 2              # レプリカ数 (1-10)
  image: "mcp-rs:latest"   # コンテナイメージ
  transport: http          # トランスポートプロトコル (http/stdio/websocket)
  port: 3000               # サーバーポート
  resources:
    limits:
      cpu: "1000m"
      memory: "512Mi"
    requests:
      cpu: "100m"
      memory: "128Mi"
  securityPolicy: "default-policy"  # SecurityPolicy への参照
  plugins:                 # 有効化するプラグイン
    - wordpress-plugin
    - mysql-plugin
  env:                     # 環境変数
    - name: RUST_LOG
      value: "info"
```

#### Status フィールド

```yaml
status:
  phase: Running           # Pending/Running/Failed/Terminating
  readyReplicas: 2         # 準備完了レプリカ数
  conditions:
    - type: Available
      status: "True"
      lastTransitionTime: "2024-01-01T00:00:00Z"
```

### Plugin

Plugin リソースは、プラグインの設定と隔離レベルを定義します。

```yaml
apiVersion: mcp.n-takatsu.dev/v1alpha1
kind: Plugin
metadata:
  name: wordpress-plugin
  namespace: default
spec:
  name: wordpress
  image: "mcp-rs-wordpress-plugin:latest"
  version: "1.0.0"
  isolation:
    level: Container         # None/Process/Container/VM
    networkIsolation: true
    filesystemIsolation: true
    processIsolation: true
  resources:
    limits:
      cpu: "500m"
      memory: "256Mi"
  config:
    api_endpoint: "https://api.wordpress.org"
    timeout: "30s"
  dependencies:
    - base-plugin
```

### SecurityPolicy

SecurityPolicy リソースは、セキュリティポリシーを定義します。

```yaml
apiVersion: mcp.n-takatsu.dev/v1alpha1
kind: SecurityPolicy
metadata:
  name: default-policy
  namespace: default
spec:
  enabled: true
  authentication:
    required: true
    methods:
      - jwt
      - oauth2
  authorization:
    mode: rbac
    allowedRoles:
      - admin
      - developer
  rateLimiting:
    enabled: true
    requestsPerMinute: 100
    burstSize: 200
  encryption:
    tlsEnabled: true
    tlsVersion: "1.3"
    certSecretName: "tls-cert-secret"
  audit:
    enabled: true
    logLevel: info
    retentionDays: 30
```

## 使用例

### 基本的な MCP サーバーのデプロイ

```bash
# MCPServer リソースを作成
kubectl apply -f - <<EOF
apiVersion: mcp.n-takatsu.dev/v1alpha1
kind: MCPServer
metadata:
  name: my-mcp-server
spec:
  replicas: 2
  image: "mcp-rs:latest"
  transport: http
  port: 3000
EOF

# ステータスを確認
kubectl get mcpserver my-mcp-server
kubectl describe mcpserver my-mcp-server
```

### プラグインの追加

```bash
# Plugin リソースを作成
kubectl apply -f - <<EOF
apiVersion: mcp.n-takatsu.dev/v1alpha1
kind: Plugin
metadata:
  name: wordpress-plugin
spec:
  name: wordpress
  image: "mcp-rs-wordpress-plugin:latest"
  version: "1.0.0"
  isolation:
    level: Container
EOF

# MCPServer にプラグインを追加
kubectl patch mcpserver my-mcp-server --type='json' -p='[
  {"op": "add", "path": "/spec/plugins/-", "value": "wordpress-plugin"}
]'
```

### セキュリティポリシーの適用

```bash
# SecurityPolicy リソースを作成
kubectl apply -f k8s/manifests/examples.yaml

# MCPServer にセキュリティポリシーを関連付け
kubectl patch mcpserver my-mcp-server --type='merge' -p='
spec:
  securityPolicy: default-policy
'
```

### スケーリング

```bash
# レプリカ数を変更
kubectl scale mcpserver my-mcp-server --replicas=5

# または kubectl edit を使用
kubectl edit mcpserver my-mcp-server
```

## 運用ガイド

### ログの確認

```bash
# Operator のログ
kubectl logs -n mcp-system -l app.kubernetes.io/name=mcp-operator

# MCP サーバーのログ
kubectl logs -l app=my-mcp-server
```

### メトリクスの確認

```bash
# Operator のメトリクスエンドポイント
kubectl port-forward -n mcp-system svc/mcp-operator-metrics 8080:8080

# ブラウザで http://localhost:8080/metrics にアクセス
```

### アップグレード

```bash
# Helm を使用している場合
helm upgrade mcp-operator charts/mcp-operator \
  --namespace mcp-system

# kubectl を使用している場合
kubectl apply -f k8s/manifests/
```

### アンインストール

```bash
# Helm を使用している場合
helm uninstall mcp-operator --namespace mcp-system

# kubectl を使用している場合
kubectl delete -f k8s/manifests/

# CRD を削除 (注意: すべてのカスタムリソースも削除されます)
kubectl delete -f k8s/crds/
```

## トラブルシューティング

### Operator が起動しない

```bash
# Operator のログを確認
kubectl logs -n mcp-system -l app.kubernetes.io/name=mcp-operator

# Pod のステータスを確認
kubectl describe pod -n mcp-system -l app.kubernetes.io/name=mcp-operator

# RBAC 権限を確認
kubectl auth can-i list mcpservers --as=system:serviceaccount:mcp-system:mcp-operator
```

### MCPServer が Running にならない

```bash
# MCPServer のステータスを確認
kubectl describe mcpserver <name>

# 関連する Deployment を確認
kubectl get deployment <name>
kubectl describe deployment <name>

# Pod のログを確認
kubectl logs -l app=<name>
```

### CRD の検証エラー

```bash
# CRD の詳細を確認
kubectl explain mcpserver.spec

# リソースを検証
kubectl apply --dry-run=client -f your-resource.yaml
```

### リソースが削除できない

```bash
# Finalizer を確認
kubectl get mcpserver <name> -o yaml | grep finalizers

# Finalizer を削除 (慎重に実行)
kubectl patch mcpserver <name> -p '{"metadata":{"finalizers":[]}}' --type=merge
```

## ベストプラクティス

### 1. リソース制限の設定

すべての MCPServer リソースにリソース制限を設定してください。

```yaml
spec:
  resources:
    limits:
      cpu: "1000m"
      memory: "512Mi"
    requests:
      cpu: "100m"
      memory: "128Mi"
```

### 2. セキュリティポリシーの使用

本番環境では必ず SecurityPolicy を適用してください。

```yaml
spec:
  securityPolicy: "production-policy"
```

### 3. プラグインの隔離

プラグインには適切な隔離レベルを設定してください。

```yaml
spec:
  isolation:
    level: Container  # 推奨
    networkIsolation: true
    filesystemIsolation: true
```

### 4. モニタリング

メトリクスとログを監視してください。

```bash
# Prometheus でメトリクスを収集
kubectl apply -f - <<EOF
apiVersion: v1
kind: ServiceMonitor
metadata:
  name: mcp-operator
spec:
  selector:
    matchLabels:
      app.kubernetes.io/name: mcp-operator
  endpoints:
  - port: metrics
EOF
```

## 高度な使用例

### カナリアデプロイメント

```bash
# ステージング環境
kubectl apply -f - <<EOF
apiVersion: mcp.n-takatsu.dev/v1alpha1
kind: MCPServer
metadata:
  name: mcp-server-canary
spec:
  replicas: 1
  image: "mcp-rs:v0.2.0-beta"
  transport: http
EOF

# 本番環境
kubectl apply -f - <<EOF
apiVersion: mcp.n-takatsu.dev/v1alpha1
kind: MCPServer
metadata:
  name: mcp-server-production
spec:
  replicas: 5
  image: "mcp-rs:v0.1.0"
  transport: http
EOF
```

### マルチテナント環境

```bash
# テナント A 用の namespace
kubectl create namespace tenant-a

# テナント A の MCPServer
kubectl apply -n tenant-a -f - <<EOF
apiVersion: mcp.n-takatsu.dev/v1alpha1
kind: MCPServer
metadata:
  name: mcp-server-a
spec:
  replicas: 2
  securityPolicy: "tenant-a-policy"
EOF
```

## 参考資料

- [Kubernetes Operator Pattern](https://kubernetes.io/docs/concepts/extend-kubernetes/operator/)
- [Custom Resource Definitions](https://kubernetes.io/docs/tasks/extend-kubernetes/custom-resources/custom-resource-definitions/)
- [kube-rs Documentation](https://kube.rs/)
- [MCP-RS GitHub Repository](https://github.com/n-takatsu/mcp-rs)
