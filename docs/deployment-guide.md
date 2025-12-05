# MCP-RS Deployment Guide

本番環境へのMCP-RSデプロイメントガイド

## 目次

1. [概要](#概要)
2. [前提条件](#前提条件)
3. [Dockerデプロイメント](#dockerデプロイメント)
4. [Kubernetesデプロイメント](#kubernetesデプロイメント)
5. [CI/CDパイプライン](#cicdパイプライン)
6. [監視とログ](#監視とログ)
7. [トラブルシューティング](#トラブルシューティング)

## 概要

MCP-RSは、以下のデプロイメント方法をサポートしています:

- **Docker Compose**: 開発環境・小規模本番環境向け
- **Kubernetes**: 大規模本番環境向け
- **GitHub Actions**: 自動デプロイメントパイプライン

### アーキテクチャ

```
┌─────────────────┐
│  Nginx/Ingress  │ ← TLS終端、ロードバランシング
└────────┬────────┘
         │
    ┌────┴────┐
    │  MCP-RS │ ← アプリケーション (複数レプリカ)
    └────┬────┘
         │
    ┌────┴────┐
    │Database │ ← PostgreSQL/MySQL
    │  Redis  │ ← キャッシュ
    └─────────┘
```

## 前提条件

### 必須要件

- **Docker**: 24.0以上
- **Docker Compose**: 2.20以上
- **Kubernetes**: 1.28以上 (K8sデプロイメントの場合)
- **kubectl**: 1.28以上
- **Git**: 2.40以上

### 推奨要件

- **CPU**: 4コア以上
- **メモリ**: 8GB以上
- **ストレージ**: 50GB以上
- **ネットワーク**: 100Mbps以上

## Dockerデプロイメント

### 1. リポジトリのクローン

```bash
git clone https://github.com/n-takatsu/mcp-rs.git
cd mcp-rs
```

### 2. 環境変数の設定

`.env`ファイルを作成:

```bash
# Database
POSTGRES_PASSWORD=your_secure_password
MYSQL_ROOT_PASSWORD=your_secure_password

# Application
RUST_LOG=info
MCP_SERVER_PORT=3000
```

### 3. TLS証明書の準備

```bash
mkdir -p certs

# 自己署名証明書（開発環境）
openssl req -x509 -newkey rsa:4096 -nodes \
  -keyout certs/server.key \
  -out certs/server.crt \
  -days 365 \
  -subj "/CN=localhost"

# 本番環境ではLet's Encryptなどを使用
```

### 4. Dockerイメージのビルド

```bash
docker build -t mcp-rs:latest .
```

### 5. アプリケーションの起動

```bash
# 基本構成（アプリケーション + データベース）
docker-compose up -d

# Nginxリバースプロキシを含む完全構成
docker-compose --profile with-nginx up -d
```

### 6. 動作確認

```bash
# ヘルスチェック
curl http://localhost:3000/health

# WebSocket接続テスト
wscat -c ws://localhost:8080
```

### 7. ログの確認

```bash
# アプリケーションログ
docker-compose logs -f mcp-rs

# すべてのサービスログ
docker-compose logs -f
```

### 8. 停止と削除

```bash
# 停止
docker-compose down

# データも削除
docker-compose down -v
```

## Kubernetesデプロイメント

### 1. Namespaceの作成

```bash
kubectl apply -f k8s/namespace.yaml
```

### 2. ConfigMapとSecretの作成

**重要**: 本番環境では、Secretの値を適切に変更してください。

```bash
# Secretを編集
kubectl create secret generic mcp-rs-secrets \
  --from-literal=POSTGRES_PASSWORD=your_password \
  --from-literal=MYSQL_ROOT_PASSWORD=your_password \
  --from-literal=DATABASE_URL=postgresql://postgres:password@postgres:5432/mcp_rs \
  --from-literal=MYSQL_URL=mysql://root:password@mysql:3306/mcp_rs \
  --from-literal=REDIS_URL=redis://redis:6379 \
  -n mcp-rs

# ConfigMapを適用
kubectl apply -f k8s/configmap.yaml
```

### 3. TLS証明書の登録

```bash
kubectl create secret tls mcp-rs-tls-certs \
  --cert=certs/server.crt \
  --key=certs/server.key \
  -n mcp-rs
```

### 4. アプリケーションのデプロイ

```bash
# Deployment、Service、ServiceAccountを作成
kubectl apply -f k8s/deployment.yaml

# Ingressを作成
kubectl apply -f k8s/ingress.yaml

# Horizontal Pod Autoscalerを作成
kubectl apply -f k8s/hpa.yaml
```

### 5. デプロイメントの確認

```bash
# Podの状態確認
kubectl get pods -n mcp-rs

# Serviceの確認
kubectl get svc -n mcp-rs

# Ingressの確認
kubectl get ingress -n mcp-rs

# ログの確認
kubectl logs -f deployment/mcp-rs -n mcp-rs
```

### 6. スケーリング

```bash
# 手動スケーリング
kubectl scale deployment mcp-rs --replicas=5 -n mcp-rs

# HPAの状態確認
kubectl get hpa -n mcp-rs
```

### 7. ローリングアップデート

```bash
# イメージの更新
kubectl set image deployment/mcp-rs \
  mcp-rs=ghcr.io/n-takatsu/mcp-rs:v1.0.1 \
  -n mcp-rs

# ロールアウト状態の確認
kubectl rollout status deployment/mcp-rs -n mcp-rs

# ロールアウト履歴
kubectl rollout history deployment/mcp-rs -n mcp-rs
```

### 8. ロールバック

```bash
# 直前のバージョンに戻す
kubectl rollout undo deployment/mcp-rs -n mcp-rs

# 特定のリビジョンに戻す
kubectl rollout undo deployment/mcp-rs --to-revision=2 -n mcp-rs
```

## CI/CDパイプライン

### GitHub Actionsワークフロー

MCP-RSは、GitHub Actionsを使用した自動デプロイメントをサポートしています。

#### デプロイメントフロー

```
┌──────────────┐
│ Code Push   │
└──────┬───────┘
       │
       ▼
┌──────────────┐
│ Pre-checks  │ ← フォーマット、Clippy、テスト
└──────┬───────┘
       │
       ▼
┌──────────────┐
│ Build Image │ ← Dockerイメージビルド
└──────┬───────┘
       │
       ▼
┌──────────────┐
│ Security    │ ← Trivyスキャン
└──────┬───────┘
       │
       ▼
┌──────────────┐
│ Deploy      │ ← Staging → Production
└──────────────┘
```

### トリガー

1. **自動デプロイメント**
   - `main`ブランチへのプッシュ → Staging環境
   - `v*.*.*`タグ → Production環境

2. **手動デプロイメント**
   - GitHub ActionsのUIから実行
   - 環境を選択可能（Staging/Production）

### 環境変数の設定

GitHub リポジトリの設定で、以下のSecretsを設定:

```
REGISTRY_USERNAME=<GitHub username>
REGISTRY_PASSWORD=<GitHub token>
DEPLOY_SSH_KEY=<SSH private key>
PRODUCTION_HOST=<production server>
```

### デプロイメントの実行

#### 自動デプロイメント

```bash
# Staging環境へのデプロイ
git push origin main

# Production環境へのデプロイ
git tag v1.0.0
git push origin v1.0.0
```

#### 手動デプロイメント

1. GitHubリポジトリの「Actions」タブを開く
2. 「Production Deployment」ワークフローを選択
3. 「Run workflow」をクリック
4. 環境を選択して実行

## 監視とログ

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

### ログ収集

#### Docker

```bash
# リアルタイムログ
docker-compose logs -f mcp-rs

# 特定期間のログ
docker-compose logs --since 1h mcp-rs
```

#### Kubernetes

```bash
# リアルタイムログ
kubectl logs -f deployment/mcp-rs -n mcp-rs

# すべてのPodのログ
kubectl logs -l app=mcp-rs -n mcp-rs --all-containers=true

# 前のコンテナのログ（クラッシュ時）
kubectl logs deployment/mcp-rs -n mcp-rs --previous
```

### 監査ログ

監査ログは以下の場所に記録されます:

- Docker: `/var/log/mcp-rs/audit.log`
- Kubernetes: `/var/log/mcp-rs/audit.log` (Pod内)

## トラブルシューティング

### よくある問題

#### 1. コンテナが起動しない

```bash
# ログを確認
docker-compose logs mcp-rs

# コンテナの状態を確認
docker-compose ps

# イメージの再ビルド
docker-compose build --no-cache
```

#### 2. データベース接続エラー

```bash
# データベースの状態を確認
docker-compose ps postgres mysql

# データベース接続をテスト
docker-compose exec postgres psql -U postgres -d mcp_rs
docker-compose exec mysql mysql -u root -p mcp_rs
```

#### 3. TLS/WebSocket接続エラー

```bash
# 証明書の確認
openssl x509 -in certs/server.crt -text -noout

# ポートの確認
netstat -an | grep 8443

# ファイアウォールの確認（Linux）
sudo ufw status
```

#### 4. Kubernetesでのデバッグ

```bash
# Podの詳細情報
kubectl describe pod <pod-name> -n mcp-rs

# Podへのアクセス
kubectl exec -it <pod-name> -n mcp-rs -- /bin/bash

# イベントログ
kubectl get events -n mcp-rs --sort-by='.lastTimestamp'

# リソース使用状況
kubectl top pods -n mcp-rs
```

### パフォーマンス最適化

#### リソース制限の調整

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

#### HPAの調整

```yaml
# k8s/hpa.yaml
spec:
  minReplicas: 3
  maxReplicas: 20
  metrics:
    - type: Resource
      resource:
        name: cpu
        target:
          averageUtilization: 60
```

## セキュリティベストプラクティス

1. **Secretの管理**
   - 環境変数を使用
   - KubernetesのSecretを暗号化
   - 定期的なパスワード更新

2. **ネットワークセキュリティ**
   - TLS/HTTPSの使用
   - Network Policyの設定
   - ファイアウォールの設定

3. **コンテナセキュリティ**
   - 非rootユーザーで実行
   - 読み取り専用ファイルシステム
   - 定期的なイメージ更新

4. **監査とログ**
   - 監査ログの有効化
   - ログの集中管理
   - アラート設定

## サポート

問題が発生した場合:

1. [GitHub Issues](https://github.com/n-takatsu/mcp-rs/issues)
2. [ドキュメント](https://github.com/n-takatsu/mcp-rs/tree/main/docs)
3. [ディスカッション](https://github.com/n-takatsu/mcp-rs/discussions)

## 参考リンク

- [Docker Documentation](https://docs.docker.com/)
- [Kubernetes Documentation](https://kubernetes.io/docs/)
- [GitHub Actions Documentation](https://docs.github.com/en/actions)
- [Nginx Documentation](https://nginx.org/en/docs/)
