# MCP-RS Production Deployment

本番環境デプロイメント用の設定ファイルとスクリプト集

## 📁 構成

```
├── Dockerfile                    # 本番環境用Dockerイメージ
├── docker-compose.yml            # Docker Compose設定
├── nginx.conf                    # Nginxリバースプロキシ設定
├── k8s/                          # Kubernetes マニフェスト
│   ├── namespace.yaml           # Namespace定義
│   ├── configmap.yaml           # ConfigMapとSecret
│   ├── deployment.yaml          # Deployment、Service、ServiceAccount
│   ├── ingress.yaml             # Ingress設定
│   └── hpa.yaml                 # Horizontal Pod Autoscaler
├── scripts/deploy/              # デプロイメントスクリプト
│   ├── docker-deploy.sh         # Docker Composeデプロイ
│   └── k8s-deploy.sh            # Kubernetesデプロイ
└── docs/                        # ドキュメント
    └── deployment-guide.md      # 詳細なデプロイメントガイド
```

## 🚀 クイックスタート

### Docker Compose

```bash
# 基本デプロイメント
./scripts/deploy/docker-deploy.sh deploy

# Nginxを含む完全構成
./scripts/deploy/docker-deploy.sh deploy --with-nginx

# ステータス確認
./scripts/deploy/docker-deploy.sh status

# ログ確認
./scripts/deploy/docker-deploy.sh logs
```

### Kubernetes

```bash
# フルデプロイメント
./scripts/deploy/k8s-deploy.sh deploy

# アップデート
IMAGE_TAG=v1.0.0 ./scripts/deploy/k8s-deploy.sh update

# スケーリング
./scripts/deploy/k8s-deploy.sh scale 5

# ステータス確認
./scripts/deploy/k8s-deploy.sh status
```

## 🔒 セキュリティ設定

### TLS証明書の準備

```bash
# 開発環境（自己署名証明書）
mkdir -p certs
openssl req -x509 -newkey rsa:4096 -nodes \
  -keyout certs/server.key \
  -out certs/server.crt \
  -days 365 \
  -subj "/CN=localhost"

# 本番環境（Let's Encrypt推奨）
# cert-managerやcertbotを使用
```

### 環境変数

`.env`ファイルを作成（Docker Compose使用時）:

```env
# Database
POSTGRES_PASSWORD=your_secure_password
MYSQL_ROOT_PASSWORD=your_secure_password

# Application
RUST_LOG=info
MCP_SERVER_PORT=3000
```

## 📊 監視とメトリクス

### ヘルスチェック

```bash
# Docker
curl http://localhost:3000/health

# Kubernetes
kubectl exec -it deployment/mcp-rs -n mcp-rs -- \
  curl http://localhost:3000/health
```

### メトリクス

```bash
# Prometheusメトリクス
curl http://localhost:3000/metrics
```

### ログ

```bash
# Docker
docker-compose logs -f mcp-rs

# Kubernetes
kubectl logs -f deployment/mcp-rs -n mcp-rs
```

## 🔄 CI/CD パイプライン

### GitHub Actions ワークフロー

`.github/workflows/deploy.yml`には以下のステージが含まれます:

1. **Pre-deployment checks**
   - コードフォーマット確認
   - Clippy linting
   - ユニットテスト
   - セキュリティ監査

2. **Build Docker image**
   - マルチプラットフォームビルド（amd64/arm64）
   - GitHub Container Registryへプッシュ
   - SBOM生成

3. **Security scan**
   - Trivyによる脆弱性スキャン
   - GitHub Securityへ結果アップロード

4. **Deploy to staging**
   - Staging環境への自動デプロイ
   - ヘルスチェック

5. **Deploy to production**
   - 手動承認後にProduction環境へデプロイ
   - バックアップ、ヘルスチェック、検証

### トリガー

- **自動**: `main`ブランチへのpush → Staging
- **自動**: `v*.*.*`タグ → Production
- **手動**: GitHub Actions UIから実行可能

## 📈 スケーリング

### 垂直スケーリング（リソース増強）

```yaml
# k8s/deployment.yaml
resources:
  requests:
    memory: "512Mi"
    cpu: "500m"
  limits:
    memory: "1Gi"
    cpu: "1000m"
```

### 水平スケーリング（レプリカ増加）

```bash
# 手動スケーリング
kubectl scale deployment/mcp-rs --replicas=5 -n mcp-rs

# HPAによる自動スケーリング設定
# k8s/hpa.yaml を参照
```

## 🔄 ロールバック

### Docker Composeでのロールバック

```bash
# 前のイメージに戻す
docker-compose down
docker pull ghcr.io/n-takatsu/mcp-rs:previous-tag
docker-compose up -d
```

### Kubernetesでのロールバック

```bash
# 自動ロールバック
./scripts/deploy/k8s-deploy.sh rollback

# 特定バージョンへロールバック
kubectl rollout undo deployment/mcp-rs \
  --to-revision=2 -n mcp-rs
```

## 🛠️ トラブルシューティング

### よくある問題

#### コンテナ起動エラー

```bash
# ログ確認
docker-compose logs mcp-rs
kubectl logs -l app=mcp-rs -n mcp-rs

# 詳細情報
kubectl describe pod <pod-name> -n mcp-rs
```

#### データベース接続エラー

```bash
# 接続テスト
docker-compose exec postgres psql -U postgres -d mcp_rs
kubectl exec -it <db-pod> -n mcp-rs -- psql -U postgres
```

#### TLS証明書エラー

```bash
# 証明書確認
openssl x509 -in certs/server.crt -text -noout

# Kubernetes Secret確認
kubectl get secret mcp-rs-tls-certs -n mcp-rs -o yaml
```

## 📚 詳細ドキュメント

完全なデプロイメントガイドは[deployment-guide.md](../docs/deployment-guide.md)を参照してください。

## 🔗 関連リンク

- [WebSocket Transport Guide](docs/websocket-transport.md)
- [Security Documentation](../docs/security/)
- [API Documentation](../docs/api/)
- [GitHub Repository](https://github.com/n-takatsu/mcp-rs)

## 📞 サポート

問題が発生した場合:

- [GitHub Issues](https://github.com/n-takatsu/mcp-rs/issues)
- [Discussions](https://github.com/n-takatsu/mcp-rs/discussions)
- [Documentation](https://github.com/n-takatsu/mcp-rs/tree/main/docs)
