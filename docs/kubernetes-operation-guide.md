# Kubernetes運用ガイド

## 概要

このガイドでは、MCP-RSプラグインシステムをKubernetes環境で運用するための手順と
ベストプラクティスを説明します。

## 目次

1. [前提条件](#前提条件)
2. [クラスタのセットアップ](#クラスタのセットアップ)
3. [CRDのインストール](#crdのインストール)
4. [Operatorのデプロイ](#operatorのデプロイ)
5. [プラグインのデプロイ](#プラグインのデプロイ)
6. [セキュリティポリシーの適用](#セキュリティポリシーの適用)
7. [監視とアラート](#監視とアラート)
8. [トラブルシューティング](#トラブルシューティング)

---

## 前提条件

### 必要なツール

- **Kubernetes**: v1.24以上
- **kubectl**: クラスタバージョンと互換性のあるバージョン
- **Helm**: v3.0以上
- **Docker**: 20.10以上

### リソース要件

- **最小**: 
  - CPU: 4 cores
  - メモリ: 8 GB
  - ディスク: 50 GB

- **推奨**:
  - CPU: 8+ cores
  - メモリ: 16+ GB
  - ディスク: 100+ GB (SSD推奨)

---

## クラスタのセットアップ

### 1. Kubernetes クラスタの作成

#### ローカル開発環境 (minikube)

```bash
# minikubeのインストール
curl -LO https://storage.googleapis.com/minikube/releases/latest/minikube-linux-amd64
sudo install minikube-linux-amd64 /usr/local/bin/minikube

# クラスタの起動
minikube start --cpus=4 --memory=8192 --disk-size=50g
```

#### 本番環境 (AWS EKS例)

```bash
# eksctlのインストール
curl --silent --location "https://github.com/weaveworks/eksctl/releases/latest/download/eksctl_$(uname -s)_amd64.tar.gz" | tar xz -C /tmp
sudo mv /tmp/eksctl /usr/local/bin

# EKSクラスタの作成
eksctl create cluster \
  --name mcp-rs-cluster \
  --version 1.28 \
  --region us-west-2 \
  --nodegroup-name standard-workers \
  --node-type t3.large \
  --nodes 3 \
  --nodes-min 2 \
  --nodes-max 5
```

### 2. namespaceの作成

```bash
kubectl create namespace mcp-system
kubectl create namespace mcp-plugins
```

---

## CRDのインストール

### Custom Resource Definitionsのデプロイ

```bash
# CRDの適用
kubectl apply -f k8s/crds/plugin-deployment.yaml
kubectl apply -f k8s/crds/plugin-policy.yaml

# CRDの確認
kubectl get crd | grep mcp-rs.io
```

期待される出力:
```
plugindeployments.mcp-rs.io   2024-01-31T...
pluginpolicies.mcp-rs.io      2024-01-31T...
```

---

## Operatorのデプロイ

### Helm Chartを使用したインストール

```bash
# Helmリポジトリの追加
helm repo add mcp-rs https://n-takatsu.github.io/mcp-rs-charts
helm repo update

# Operatorのインストール
helm install mcp-operator mcp-rs/mcp-operator \
  --namespace mcp-system \
  --create-namespace \
  --set image.tag=v0.16.0

# デプロイの確認
kubectl get pods -n mcp-system
```

### マニフェストから直接デプロイ

```bash
# Operatorのデプロイ
kubectl apply -f charts/mcp-operator/manifests/

# ステータスの確認
kubectl rollout status deployment/mcp-operator -n mcp-system
```

---

## プラグインのデプロイ

### 1. PluginDeploymentの作成

#### Example: WordPressプラグイン

```yaml
# wordpress-plugin.yaml
apiVersion: mcp-rs.io/v1
kind: PluginDeployment
metadata:
  name: wordpress-plugin
  namespace: mcp-plugins
spec:
  pluginId: "wordpress-v1"
  image: "ghcr.io/n-takatsu/mcp-rs/wordpress-plugin:latest"
  replicas: 3
  resourceLimits:
    maxCpuPercent: 70.0
    maxMemoryMb: 512
    maxDiskIops: 5000
  env:
    LOG_LEVEL: "info"
  autoScaling:
    minReplicas: 2
    maxReplicas: 10
    targetCpuUtilizationPercentage: 75
  healthCheck:
    path: "/health"
    port: 8080
    initialDelaySeconds: 30
    periodSeconds: 10
```

```bash
# プラグインのデプロイ
kubectl apply -f wordpress-plugin.yaml

# ステータスの確認
kubectl get plugindeployments -n mcp-plugins
kubectl describe plugindeployment wordpress-plugin -n mcp-plugins
```

### 2. スケーリング

```bash
# 手動スケーリング
kubectl scale plugindeployment wordpress-plugin --replicas=5 -n mcp-plugins

# HPA (Horizontal Pod Autoscaler) の確認
kubectl get hpa -n mcp-plugins
```

---

## セキュリティポリシーの適用

### 1. PluginPolicyの作成

```yaml
# wordpress-security-policy.yaml
apiVersion: mcp-rs.io/v1
kind: PluginPolicy
metadata:
  name: wordpress-security-policy
  namespace: mcp-plugins
spec:
  pluginSelector:
    pluginId: "wordpress-v1"
  networkPolicy:
    allowedEgress:
      - "api.wordpress.org"
      - "*.gravatar.com"
    blockedDomains:
      - "malicious.com"
    allowAllEgress: false
  securityContext:
    runAsNonRoot: true
    readOnlyRootFilesystem: true
    dropAllCapabilities: true
    seccompProfile:
      type: "RuntimeDefault"
  rateLimit:
    requestsPerSecond: 100
    burst: 200
```

```bash
# ポリシーの適用
kubectl apply -f wordpress-security-policy.yaml

# ポリシーの確認
kubectl get pluginpolicies -n mcp-plugins
```

### 2. Falcoのセットアップ

```bash
# Falco Helmチャートのインストール
helm repo add falcosecurity https://falcosecurity.github.io/charts
helm repo update

helm install falco falcosecurity/falco \
  --namespace falco-system \
  --create-namespace \
  --set falco.rulesFile={/etc/falco/falco_rules.yaml,/etc/falco/custom-rules.yaml} \
  --set-file customRules.custom-rules.yaml=configs/security/falco-rules.yaml
```

---

## 監視とアラート

### 1. Prometheusのデプロイ

```bash
# Prometheus Operatorのインストール
helm repo add prometheus-community https://prometheus-community.github.io/helm-charts
helm repo update

helm install prometheus prometheus-community/kube-prometheus-stack \
  --namespace monitoring \
  --create-namespace \
  --values configs/monitoring/prometheus-values.yaml
```

### 2. Grafanaダッシュボードのインポート

```bash
# Grafanaにアクセス
kubectl port-forward -n monitoring svc/prometheus-grafana 3000:80

# ブラウザで http://localhost:3000 を開く
# デフォルトログイン: admin / prom-operator

# ダッシュボードのインポート
# + → Import → Upload JSON file
# configs/monitoring/grafana-dashboard.json を選択
```

### 3. アラートルールの確認

```bash
# アラートの確認
kubectl get prometheusrules -n monitoring

# Alertmanager の確認
kubectl port-forward -n monitoring svc/prometheus-kube-prometheus-alertmanager 9093:9093
```

---

## トラブルシューティング

### プラグインが起動しない

```bash
# Podのステータス確認
kubectl get pods -n mcp-plugins -l app=mcp-plugin

# ログの確認
kubectl logs -n mcp-plugins <pod-name>

# イベントの確認
kubectl get events -n mcp-plugins --sort-by='.lastTimestamp'

# リソース制限の確認
kubectl describe pod -n mcp-plugins <pod-name>
```

### ネットワーク接続の問題

```bash
# ネットワークポリシーの確認
kubectl get networkpolicies -n mcp-plugins

# DNS解決のテスト
kubectl run -it --rm debug --image=nicolaka/netshoot --restart=Never -- nslookup api.wordpress.org

# 接続テスト
kubectl run -it --rm debug --image=curlimages/curl --restart=Never -- curl -v https://api.wordpress.org
```

### リソース不足

```bash
# ノードのリソース使用状況
kubectl top nodes

# Podのリソース使用状況
kubectl top pods -n mcp-plugins

# リソースクォータの確認
kubectl get resourcequotas -n mcp-plugins

# LimitRangeの確認
kubectl get limitranges -n mcp-plugins
```

### Operatorのログ確認

```bash
# Operatorのログ
kubectl logs -n mcp-system -l app=mcp-operator --tail=100 -f

# Operatorの再起動
kubectl rollout restart deployment/mcp-operator -n mcp-system
```

---

## ベストプラクティス

### 1. リソース管理

- **リソースリクエストとリミットを適切に設定**
  ```yaml
  resources:
    requests:
      cpu: "500m"
      memory: "512Mi"
    limits:
      cpu: "1000m"
      memory: "1Gi"
  ```

- **HPAを使用した自動スケーリング**

- **PodDisruptionBudgetの設定**
  ```yaml
  apiVersion: policy/v1
  kind: PodDisruptionBudget
  metadata:
    name: wordpress-plugin-pdb
  spec:
    minAvailable: 2
    selector:
      matchLabels:
        plugin: wordpress-v1
  ```

### 2. セキュリティ

- **最小権限の原則を適用**
- **NetworkPolicyで通信を制限**
- **SeccompとAppArmorプロファイルの使用**
- **定期的なイメージスキャン**

### 3. 監視

- **メトリクス収集の設定**
- **アラートしきい値の適切な設定**
- **ログ集約システムの導入 (ELK, Loki等)**

### 4. 高可用性

- **複数のレプリカを展開**
- **複数のavailability zoneに分散**
- **定期的なバックアップ**

---

## 参考資料

- [Kubernetes Documentation](https://kubernetes.io/docs/)
- [Helm Documentation](https://helm.sh/docs/)
- [Prometheus Operator](https://github.com/prometheus-operator/prometheus-operator)
- [Falco Documentation](https://falco.org/docs/)
- [MCP-RS GitHub Repository](https://github.com/n-takatsu/mcp-rs)

---

## サポート

問題が発生した場合は、以下をご確認ください:

- [GitHubイシュー](https://github.com/n-takatsu/mcp-rs/issues)
- [ディスカッション](https://github.com/n-takatsu/mcp-rs/discussions)
- [プロジェクト管理ガイド](./project-management-guide.md)
