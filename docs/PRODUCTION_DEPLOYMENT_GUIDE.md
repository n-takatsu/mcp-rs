# MCP-RS Production Deployment Guide

本ガイドでは、mcp-server Helm Chart を使用した本番環境へのデプロイメント手順を説明します。

## 目次

- [前提条件](#前提条件)
- [環境別デプロイメント](#環境別デプロイメント)
  - [開発環境](#開発環境)
  - [ステージング環境](#ステージング環境)
  - [本番環境](#本番環境)
- [カナリアデプロイメント](#カナリアデプロイメント)
- [アップグレード手順](#アップグレード手順)
- [ロールバック手順](#ロールバック手順)
- [監視とメトリクス](#監視とメトリクス)
- [バックアップと復元](#バックアップと復元)
- [トラブルシューティング](#トラブルシューティング)

---

## 前提条件

### 必須ツール

- Kubernetes クラスター v1.26+
- Helm v3.10+
- kubectl v1.26+
- (オプション) cert-manager for TLS certificates
- (オプション) Prometheus Operator for monitoring

### クラスター要件

- 最小ノード数: 3
- ノードあたり最小リソース:
  - CPU: 4 cores
  - Memory: 8GB
  - Storage: 100GB

### インストール前の準備

```bash
# Namespace 作成
kubectl create namespace mcp-production

# Secrets 作成（本番環境用）
kubectl create secret generic mcp-server-prod-secrets \
  --from-literal=API_KEY=your-api-key \
  --from-literal=DB_PASSWORD=your-db-password \
  -n mcp-production

# ConfigMap 作成（カスタム設定用）
kubectl create configmap mcp-server-prod-config \
  --from-file=security-policy-production.toml \
  -n mcp-production
```

---

## 環境別デプロイメント

### 開発環境

最小リソースでの単一レプリカデプロイメント。

```bash
# Chart の検証
helm lint charts/mcp-server -f charts/mcp-server/values-dev.yaml

# Dry-run で確認
helm install mcp-server-dev charts/mcp-server \
  -f charts/mcp-server/values-dev.yaml \
  -n mcp-dev \
  --create-namespace \
  --dry-run --debug

# デプロイ
helm install mcp-server-dev charts/mcp-server \
  -f charts/mcp-server/values-dev.yaml \
  -n mcp-dev \
  --create-namespace

# 状態確認
kubectl get pods -n mcp-dev
kubectl get svc -n mcp-dev

# ローカルアクセス
kubectl port-forward -n mcp-dev svc/mcp-server-dev 3000:3000
```

**特徴:**
- レプリカ数: 1
- リソース: CPU 100m-500m, Memory 128Mi-512Mi
- オートスケーリング: 無効
- セキュリティポリシー: 緩和
- ログレベル: debug

---

### ステージング環境

本番環境に近い設定でのテスト環境。

```bash
# デプロイ
helm install mcp-server-staging charts/mcp-server \
  -f charts/mcp-server/values-staging.yaml \
  -n mcp-staging \
  --create-namespace \
  --wait --timeout 5m

# 状態確認
kubectl get pods -n mcp-staging
kubectl get hpa -n mcp-staging
kubectl get pdb -n mcp-staging

# Ingress 確認
kubectl get ingress -n mcp-staging
```

**特徴:**
- レプリカ数: 2-5（オートスケーリング）
- リソース: CPU 250m-1000m, Memory 512Mi-2Gi
- TLS 証明書: Let's Encrypt staging
- モニタリング: 有効

---

### 本番環境

High Availability 構成での完全な本番デプロイメント。

```bash
# デプロイ前チェック
helm template mcp-server-prod charts/mcp-server \
  -f charts/mcp-server/values-production.yaml \
  -n mcp-production | kubectl apply --dry-run=client -f -

# デプロイ
helm install mcp-server-prod charts/mcp-server \
  -f charts/mcp-server/values-production.yaml \
  -n mcp-production \
  --wait --timeout 10m

# デプロイ確認
kubectl get all -n mcp-production -l app.kubernetes.io/instance=mcp-server-prod

# HPA 状態確認
kubectl get hpa -n mcp-production

# PDB 確認
kubectl get pdb -n mcp-production

# エンドポイント確認
kubectl get endpoints -n mcp-production
```

**特徴:**
- レプリカ数: 5-20（オートスケーリング）
- リソース: CPU 500m-2000m, Memory 1Gi-4Gi
- Pod Disruption Budget: minAvailable 3
- TLS 証明書: Let's Encrypt production
- バックアップ: 有効（毎日2時）
- 監視: Prometheus + AlertManager

---

## カナリアデプロイメント

新バージョンを段階的にロールアウト。

### ステップ 1: カナリアデプロイ（10% トラフィック）

```bash
# カナリアデプロイ
helm install mcp-server-canary charts/mcp-server \
  -f charts/mcp-server/values-canary.yaml \
  --set image.tag=0.16.0 \
  --set ingress.annotations."nginx\.ingress\.kubernetes\.io/canary-weight"=10 \
  -n mcp-production

# トラフィック確認
kubectl logs -n mcp-production -l deployment-type=canary --tail=100 -f
```

### ステップ 2: トラフィック増加（50%）

```bash
# トラフィックを50%に増加
helm upgrade mcp-server-canary charts/mcp-server \
  -f charts/mcp-server/values-canary.yaml \
  --set image.tag=0.16.0 \
  --set ingress.annotations."nginx\.ingress\.kubernetes\.io/canary-weight"=50 \
  --reuse-values \
  -n mcp-production

# メトリクス確認
kubectl top pods -n mcp-production
```

### ステップ 3: 完全切り替え

```bash
# 安定版を新バージョンにアップグレード
helm upgrade mcp-server-prod charts/mcp-server \
  -f charts/mcp-server/values-production.yaml \
  --set image.tag=0.16.0 \
  -n mcp-production

# カナリアを削除
helm uninstall mcp-server-canary -n mcp-production
```

---

## アップグレード手順

### ゼロダウンタイムアップグレード

```bash
# 現在のバージョン確認
helm list -n mcp-production

# Chart の差分確認
helm diff upgrade mcp-server-prod charts/mcp-server \
  -f charts/mcp-server/values-production.yaml \
  -n mcp-production

# アップグレード実行
helm upgrade mcp-server-prod charts/mcp-server \
  -f charts/mcp-server/values-production.yaml \
  --set image.tag=0.16.0 \
  -n mcp-production \
  --wait --timeout 10m

# ロールアウト状態確認
kubectl rollout status deployment/mcp-server-prod -n mcp-production
```

### 設定変更のみのアップグレード

```bash
# values.yaml を編集後
helm upgrade mcp-server-prod charts/mcp-server \
  -f charts/mcp-server/values-production.yaml \
  --reuse-values \
  -n mcp-production
```

---

## ロールバック手順

### 手動ロールバック

```bash
# リビジョン履歴確認
helm history mcp-server-prod -n mcp-production

# 前バージョンにロールバック
helm rollback mcp-server-prod -n mcp-production

# 特定リビジョンにロールバック
helm rollback mcp-server-prod 5 -n mcp-production
```

### Kubernetes Deployment ロールバック

```bash
# Deployment ロールアウト履歴
kubectl rollout history deployment/mcp-server-prod -n mcp-production

# 前リビジョンにロールバック
kubectl rollout undo deployment/mcp-server-prod -n mcp-production

# 特定リビジョンにロールバック
kubectl rollout undo deployment/mcp-server-prod --to-revision=3 -n mcp-production
```

---

## 監視とメトリクス

### Prometheus メトリクス

```bash
# ServiceMonitor 確認
kubectl get servicemonitor -n mcp-production

# メトリクスエンドポイント確認
kubectl port-forward -n mcp-production svc/mcp-server-prod 9090:9090
# http://localhost:9090/metrics にアクセス
```

### ログ確認

```bash
# 全Podのログ
kubectl logs -n mcp-production -l app=mcp-server --tail=100 -f

# 特定Podのログ
kubectl logs -n mcp-production mcp-server-prod-xxxxx -f

# エラーログのみ
kubectl logs -n mcp-production -l app=mcp-server --tail=100 | grep ERROR
```

### リソース使用状況

```bash
# Pod リソース使用量
kubectl top pods -n mcp-production

# Node リソース使用量
kubectl top nodes

# HPA 状態
kubectl describe hpa mcp-server-prod -n mcp-production
```

---

## バックアップと復元

### バックアップ確認

```bash
# CronJob 確認
kubectl get cronjob -n mcp-production

# 最新のバックアップジョブ
kubectl get jobs -n mcp-production -l app.kubernetes.io/component=backup

# バックアップログ
kubectl logs -n mcp-production -l app.kubernetes.io/component=backup
```

### 手動バックアップ

```bash
# 手動バックアップジョブ作成
kubectl create job --from=cronjob/mcp-server-prod-backup \
  manual-backup-$(date +%Y%m%d%H%M%S) \
  -n mcp-production

# バックアップファイル確認
kubectl exec -n mcp-production -it mcp-server-prod-xxxxx -- ls -lh /backup
```

### 復元手順

```bash
# Pod を一時停止
kubectl scale deployment mcp-server-prod --replicas=0 -n mcp-production

# バックアップから復元
kubectl exec -n mcp-production -it <backup-pod> -- \
  tar xzf /backup/mcp-backup-20250113-020000.tar.gz -C /var/lib/mcp

# Pod を再起動
kubectl scale deployment mcp-server-prod --replicas=5 -n mcp-production
```

---

## トラブルシューティング

### Pod が起動しない

```bash
# Pod 状態確認
kubectl describe pod -n mcp-production <pod-name>

# イベント確認
kubectl get events -n mcp-production --sort-by='.lastTimestamp'

# リソース不足チェック
kubectl top nodes
kubectl describe nodes | grep -A 5 "Allocated resources"
```

### Liveness/Readiness Probe 失敗

```bash
# Probe エンドポイント確認
kubectl exec -n mcp-production -it <pod-name> -- wget -O- http://localhost:3000/health

# Probe 設定確認
kubectl get pod <pod-name> -n mcp-production -o jsonpath='{.spec.containers[0].livenessProbe}'
```

### HPA が動作しない

```bash
# Metrics Server 確認
kubectl get apiservice v1beta1.metrics.k8s.io -o yaml
kubectl top nodes

# HPA 詳細
kubectl describe hpa mcp-server-prod -n mcp-production

# メトリクス確認
kubectl get --raw /apis/metrics.k8s.io/v1beta1/namespaces/mcp-production/pods
```

### Ingress が動作しない

```bash
# Ingress 詳細
kubectl describe ingress mcp-server-prod -n mcp-production

# Ingress Controller ログ
kubectl logs -n ingress-nginx -l app.kubernetes.io/component=controller

# DNS 確認
nslookup mcp-server.production.example.com

# 証明書確認
kubectl get certificate -n mcp-production
kubectl describe certificate mcp-server-production-tls -n mcp-production
```

### パフォーマンス問題

```bash
# CPU/メモリ使用率
kubectl top pods -n mcp-production

# リクエストレイテンシ確認
kubectl exec -n mcp-production -it <pod-name> -- \
  wget -O- http://localhost:9090/metrics | grep http_request_duration

# コネクション数確認
kubectl exec -n mcp-production -it <pod-name> -- \
  netstat -an | grep :3000 | wc -l
```

### ログ分析

```bash
# エラー率確認
kubectl logs -n mcp-production -l app=mcp-server --since=1h | grep ERROR | wc -l

# 特定エラーパターン検索
kubectl logs -n mcp-production -l app=mcp-server --since=1h | grep "database connection"

# JSON ログのパース（jq使用）
kubectl logs -n mcp-production <pod-name> --tail=100 | jq '.level,.message'
```

---

## ベストプラクティス

### デプロイメント前

1. ✅ Staging 環境で十分にテスト
2. ✅ リソース要求・制限を適切に設定
3. ✅ バックアップを取得
4. ✅ ロールバック計画を準備

### デプロイメント中

1. ✅ `--wait` フラグを使用してデプロイ完了を待機
2. ✅ ログとメトリクスを監視
3. ✅ Pod の起動を段階的に確認

### デプロイメント後

1. ✅ ヘルスチェックの成功を確認
2. ✅ エンドツーエンドテストを実行
3. ✅ アラートが正常に動作していることを確認
4. ✅ パフォーマンスメトリクスをベースライン化

---

## 関連ドキュメント

- [Kubernetes Guide](./KUBERNETES_GUIDE.md)
- [Security Policy Guide](../demo-policies/README.md)
- [API Documentation](./docs/api/README.md)

---

## サポート

問題が発生した場合:
1. GitHub Issues: https://github.com/n-takatsu/mcp-rs/issues
2. Discussions: https://github.com/n-takatsu/mcp-rs/discussions
