# Container Integration Tests Guide

完全なコンテナ環境での統合テストスイートとその実行方法を説明します。

## 概要

このテストスイートは、Docker/Kubernetes環境でのMCP-RSの動作を包括的に検証します。

### テストカテゴリ

- **Docker環境テスト**: Docker Composeを使用した統合テスト
- **パフォーマンステスト**: 起動時間、レイテンシ、スループット
- **セキュリティテスト**: 権限、ネットワーク隔離、脆弱性
- **Kubernetesテスト**: K8sクラスタでの動作検証

## 前提条件

### 必須ツール

- Docker 24.0+
- Docker Compose 2.20+
- Rust 1.75+
- cargo

### オプションツール（K8sテスト用）

- kubectl
- kind または minikube
- Helm 3+

## クイックスタート

### 1. ローカルテスト実行

```bash
# Linuxbash
./scripts/run-integration-tests.sh

# Windows PowerShell
.\scripts\run-integration-tests.ps1
```

### 2. 特定のテストカテゴリ実行

```bash
# Docker環境テスト
cargo test --features integration-tests docker

# パフォーマンステスト
cargo test --features integration-tests performance

# セキュリティテスト
cargo test --features integration-tests security
```

### 3. すべてのテスト実行

```bash
cargo test --features integration-tests --test '*' -- --test-threads=1
```

## Docker Composeテスト環境

### アーキテクチャ

```
┌─────────────────────────────────────────────────────────────┐
│                    Test Network (172.25.0.0/16)             │
│                                                              │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐      │
│  │ PostgreSQL   │  │    Redis     │  │   Prometheus │      │
│  │   :5432      │  │    :6379     │  │    :9090     │      │
│  └──────┬───────┘  └──────┬───────┘  └──────┬───────┘      │
│         │                 │                 │               │
│         └─────────┬───────┴─────────┬───────┘               │
│                   │                 │                       │
│         ┌─────────▼─────┐ ┌────────▼────────┐              │
│         │ MCP HTTP      │ │ MCP WebSocket   │              │
│         │  :3000        │ │   :3001         │              │
│         └─────────┬─────┘ └────────┬────────┘              │
│                   │                 │                       │
│                   └────────┬────────┘                       │
│                            │                                │
│                   ┌────────▼────────┐                       │
│                   │  Test Runner    │                       │
│                   │   (Rust tests)  │                       │
│                   └─────────────────┘                       │
└─────────────────────────────────────────────────────────────┘
```

### サービス構成

| サービス | ポート | 説明 |
|---------|--------|------|
| postgres-test | 5433:5432 | PostgreSQL 16 テスト用DB |
| redis-test | 6380:6379 | Redis 7 キャッシュ |
| mcp-server-http | 3001:3000 | MCP HTTP サーバー |
| mcp-server-websocket | 3002:3001 | MCP WebSocket サーバー |
| prometheus-test | 9091:9090 | Prometheus 監視 |
| grafana-test | 3100:3000 | Grafana 可視化 |
| nginx-test | 8080:80 | NGINX ロードバランサー |

## テストスイート詳細

### Docker環境テスト (`tests/integration/docker.rs`)

#### ヘルスチェックテスト
```rust
#[tokio::test]
async fn test_docker_compose_health_checks()
```
- 全サービスの起動確認
- ヘルスエンドポイントの応答確認

#### データベース接続テスト
```rust
#[tokio::test]
async fn test_database_connectivity()
```
- PostgreSQL接続確認
- クエリ実行確認

#### Redis接続テスト
```rust
#[tokio::test]
async fn test_redis_connectivity()
```
- Redis接続確認
- SET/GET操作確認

#### 並行接続テスト
```rust
#[tokio::test]
async fn test_concurrent_connections()
```
- 10並行リクエスト
- エラー率確認

### パフォーマンステスト (`tests/integration/performance.rs`)

#### 起動時間テスト
```rust
#[tokio::test]
async fn test_container_startup_time()
```
- **目標**: < 30秒
- コンテナ起動からヘルスチェック成功まで計測

#### レイテンシテスト
```rust
#[tokio::test]
async fn test_request_latency()
```
- **目標**: 平均 < 100ms
- 100リクエストの平均/最大/最小レイテンシ計測

#### スループットテスト
```rust
#[tokio::test]
async fn test_throughput()
```
- **目標**: > 100 req/s
- 10秒間の継続リクエストでスループット計測

#### 負荷テスト
```rust
#[tokio::test]
async fn test_concurrent_load()
```
- 50並行タスク × 20リクエスト = 1000リクエスト
- **目標**: < 60秒で完了

#### メモリ安定性テスト
```rust
#[tokio::test]
async fn test_memory_stability()
```
- 1000リクエスト実行
- メモリリークがないことを確認

### セキュリティテスト (`tests/integration/security.rs`)

#### 非rootユーザー実行
```rust
#[tokio::test]
async fn test_container_runs_as_non_root()
```
- コンテナが非rootユーザーで実行されることを確認

#### ネットワーク隔離
```rust
#[tokio::test]
async fn test_network_isolation()
```
- テストネットワーク内の通信確認

#### SQLインジェクション防御
```rust
#[tokio::test]
async fn test_sql_injection_protection()
```
- パラメータ化クエリの安全性確認
- 悪意のある入力の無害化

#### レート制限
```rust
#[tokio::test]
async fn test_rate_limiting()
```
- 200リクエストでレート制限確認

#### 入力検証
```rust
#[tokio::test]
async fn test_input_validation()
```
- パストラバーサル
- XSS
- SQLインジェクション
- JNDI攻撃

### Kubernetesテスト (`tests/integration/kubernetes.rs`)

**注意**: これらのテストはKubernetesクラスタが必要です。

```bash
# kindクラスタ作成
kind create cluster --name mcp-test

# Helm Chartデプロイ
helm install mcp-test charts/mcp-server -f charts/mcp-server/values-dev.yaml

# Kubernetesテスト実行
cargo test --features integration-tests kubernetes -- --ignored
```

## CI/CD統合

### GitHub Actions

`.github/workflows/container-integration-tests.yml` で自動実行されます。

#### トリガー

- `push` to main/develop/feature/**
- `pull_request` to main/develop
- 毎日2:00 UTC (schedule)

#### ジョブ構成

1. **integration-tests**
   - GitHub Actions Services (PostgreSQL, Redis)
   - MCPサーバー起動
   - 統合テスト実行

2. **docker-compose-tests**
   - Docker Composeでフルスタック起動
   - コンテナ環境テスト実行

3. **security-scan**
   - Trivy脆弱性スキャン
   - cargo audit実行

4. **performance-benchmarks** (main push時のみ)
   - パフォーマンステスト実行
   - 結果をArtifactに保存

### ローカルCI実行

```bash
# Act (GitHub Actions local runner) を使用
act push -j integration-tests
```

## 環境変数

| 変数 | デフォルト | 説明 |
|------|-----------|------|
| `MCP_HTTP_ENDPOINT` | `http://localhost:3001` | HTTP エンドポイント |
| `MCP_WEBSOCKET_ENDPOINT` | `ws://localhost:3002` | WebSocket エンドポイント |
| `DATABASE_URL` | `postgres://testuser:testpass@localhost:5433/mcptest` | PostgreSQL URL |
| `REDIS_URL` | `redis://localhost:6380` | Redis URL |
| `RUN_PERFORMANCE_TESTS` | `false` | パフォーマンステスト実行 |
| `RUST_BACKTRACE` | `1` | バックトレース表示 |
| `RUST_LOG` | `debug` | ログレベル |

## トラブルシューティング

### コンテナが起動しない

```bash
# ログ確認
docker-compose -f docker-compose.test.yml logs

# 特定サービスのログ
docker-compose -f docker-compose.test.yml logs mcp-server-http

# コンテナ状態確認
docker-compose -f docker-compose.test.yml ps
```

### テストが失敗する

```bash
# 詳細ログ付きで実行
RUST_LOG=debug cargo test --features integration-tests -- --nocapture

# 特定のテストのみ実行
cargo test --features integration-tests test_database_connectivity -- --nocapture
```

### ポート競合

```bash
# 使用中のポート確認
# Linux/Mac
lsof -i :3001

# Windows
netstat -ano | findstr :3001

# docker-compose.test.yml のポート番号を変更
```

### Docker Composeクリーンアップ

```bash
# 全コンテナとボリューム削除
docker-compose -f docker-compose.test.yml down -v

# Dockerシステムクリーンアップ
docker system prune -a --volumes
```

## ベストプラクティス

### テスト作成

1. ✅ 各テストは独立して実行可能
2. ✅ テスト後のクリーンアップを実装
3. ✅ タイムアウトを適切に設定
4. ✅ エラーメッセージは詳細に
5. ✅ リトライロジックを適切に実装

### パフォーマンステスト

1. ✅ ウォームアップ期間を設ける
2. ✅ 平均値だけでなくP50/P95/P99を計測
3. ✅ 負荷を段階的に増加
4. ✅ リソース使用量も監視

### セキュリティテスト

1. ✅ 実際の攻撃パターンをシミュレート
2. ✅ False positiveを最小化
3. ✅ 定期的な脆弱性スキャン実行
4. ✅ セキュリティアップデートを迅速に適用

## パフォーマンスベンチマーク結果

典型的な実行結果（参考値）:

| メトリクス | 目標 | 実測値 |
|-----------|------|--------|
| コンテナ起動時間 | < 30s | ~20s |
| 平均レイテンシ | < 100ms | ~50ms |
| スループット | > 100 req/s | ~200 req/s |
| 並行負荷 (1000req) | < 60s | ~30s |
| メモリ安定性 | リークなし | ✓ 安定 |

## 次のステップ

1. Kubernetesクラスタでのテスト実行
2. より複雑な負荷シナリオの追加
3. Chaos Engineering (カオステスト) の実装
4. E2Eテストの自動化
5. パフォーマンスモニタリングダッシュボード構築

## 関連ドキュメント

- [Docker Guide](./docs/DOCKER_GUIDE.md)
- [Kubernetes Guide](./docs/KUBERNETES_GUIDE.md)
- [Production Deployment Guide](./docs/PRODUCTION_DEPLOYMENT_GUIDE.md)
- [Main README](./README.md)

## サポート

問題が発生した場合:
- GitHub Issues: https://github.com/n-takatsu/mcp-rs/issues
- Discussions: https://github.com/n-takatsu/mcp-rs/discussions
