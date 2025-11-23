# PostgreSQL Docker Compose ガイド

MCP-RS Phase 2 PostgreSQL テスト環境を Docker Compose で構築・管理するためのガイドです。

## 概要

このセットアップは以下を提供します：

- **PostgreSQL Primary (15-Alpine)** - メインデータベースサーバー
- **PostgreSQL Secondary (15-Alpine)** - レプリケーション/フェイルオーバーテスト用
- **pgAdmin 4** - Web ベースのデータベース管理ツール

## システム要件

- Docker & Docker Compose
- 最低 2GB RAM
- ポート開放: 5432, 5433, 5050

## クイックスタート

### 1. 環境設定

```bash
# .env ファイルを作成（.env.example からコピー）
cp .env.example .env

# 必要に応じて値を編集
# POSTGRES_PASSWORD=your_secure_password
# PGADMIN_PASSWORD=your_pgadmin_password
```

### 2. コンテナ起動

```bash
# プロダクション環境
docker-compose -f docker-compose.postgres.yml up -d

# 開発環境（ログ表示）
docker-compose -f docker-compose.postgres.yml up

# バックグラウンド起動
docker-compose -f docker-compose.postgres.yml up -d
```

### 3. 接続確認

```bash
# Primary への接続テスト
psql -h localhost -p 5432 -U postgres -d testdb -c "SELECT version();"

# Secondary への接続テスト
psql -h localhost -p 5433 -U postgres -d testdb -c "SELECT version();"

# pgAdmin へアクセス
# http://localhost:5050
# Email: admin@example.com
# Password: admin
```

## コンテナ管理

### ステータス確認

```bash
# 全コンテナのステータス表示
docker-compose -f docker-compose.postgres.yml ps

# ログ表示
docker-compose -f docker-compose.postgres.yml logs -f

# 特定コンテナのログ
docker-compose -f docker-compose.postgres.yml logs -f postgres-primary
```

### コンテナ停止・再起動

```bash
# 全コンテナ停止
docker-compose -f docker-compose.postgres.yml stop

# 全コンテナ再起動
docker-compose -f docker-compose.postgres.yml restart

# 特定コンテナ再起動
docker-compose -f docker-compose.postgres.yml restart postgres-primary

# コンテナ削除（データ保持）
docker-compose -f docker-compose.postgres.yml down

# コンテナとボリューム削除（データ削除）
docker-compose -f docker-compose.postgres.yml down -v
```

## テスト実行

### Rust テストスイート実行

```bash
# 環境変数設定（.env から読み込み）
export $(cat .env | xargs)

# 基本テスト実行
cargo test --test postgres_phase2_basic_tests

# 統合テスト実行
cargo test --test postgres_phase2_integration_tests

# 互換性テスト実行
cargo test --test mysql_postgres_compatibility_tests

# すべてのテスト実行
cargo test --test postgres_phase2_basic_tests \
           --test postgres_phase2_integration_tests \
           --test mysql_postgres_compatibility_tests
```

### 手動テスト

```bash
# Primary への接続
psql postgresql://postgres:postgres@localhost:5432/testdb

# テーブル一覧表示
\dt test_schema.*

# スキーマ確認
\dn

# ユーザー確認
\du

# 接続切断
\q
```

## データベース構造

### 作成されるスキーマ

#### `test_schema` - 基本テスト用
- `users` - ユーザーマスタテーブル
- `posts` - 投稿テーブル (JSON メタデータ対応)
- `comments` - コメントテーブル
- `uuid_entities` - UUID テスト用テーブル

#### `transaction_test` - トランザクション/JSON テスト用
- `isolation_test` - 分離レベルテスト
- `savepoint_test` - セーブポイントテスト
- `constraint_test` - 制約テスト
- `json_operations` - JSON 操作テスト
- `concurrent_test` - 並行処理テスト
- `parameter_test` - パラメータバインディングテスト

#### `performance_test` - パフォーマンステスト用
- `large_dataset` - 大規模データセット
- `complex_joins` - 複雑な結合テスト

#### `security_test` - セキュリティテスト用
- `sensitive_data` - 機密データテスト

## 初期化スクリプト

### `scripts/postgres/init.sql`

起動時に自動実行される初期化スクリプト：

- テストユーザー (`testuser`) 作成
- 基本スキーマとテーブル作成
- インデックス作成
- UUID 拡張機能有効化
- トリガーと関数作成

### `scripts/postgres/test-schema.sql`

初期化後に実行される追加スキーマ：

- トランザクション用スキーマ作成
- パフォーマンステスト用テーブル作成
- JSON テスト用テーブル作成

## ボリューム管理

コンテナ内データは以下のボリュームに永続化：

- `postgres_primary_data` - Primary データベースファイル
- `postgres_secondary_data` - Secondary データベースファイル
- `pgadmin_data` - pgAdmin 設定

### ボリューム確認

```bash
docker volume ls | grep mcp-rs

# ボリューム詳細情報
docker volume inspect postgres_primary_data
```

### ボリューム削除

```bash
# 全ボリューム削除
docker-compose -f docker-compose.postgres.yml down -v

# または個別削除
docker volume rm postgres_primary_data
```

## ネットワーク管理

### ネットワーク情報

```bash
# ネットワーク確認
docker network ls | grep postgres

# 詳細情報
docker network inspect mcp-rs_postgres-network
```

### コンテナ間通信

コンテナ内では `postgres-primary`, `postgres-secondary` のホスト名で接続可能：

```
postgresql://postgres:postgres@postgres-primary:5432/testdb
postgresql://postgres:postgres@postgres-secondary:5432/testdb
```

## トラブルシューティング

### PostgreSQL 起動エラー

```bash
# ログ確認
docker-compose -f docker-compose.postgres.yml logs postgres-primary

# ボリュームクリア（データ削除）
docker-compose -f docker-compose.postgres.yml down -v

# 再起動
docker-compose -f docker-compose.postgres.yml up -d
```

### ポート競合

```bash
# ポート確認
netstat -tulpn | grep 5432

# 別ポート指定
POSTGRES_PORT=5432 docker-compose -f docker-compose.postgres.yml up -d
```

### 接続タイムアウト

```bash
# ヘルスチェック確認
docker-compose -f docker-compose.postgres.yml ps

# コンテナのヘルスチェックログ確認
docker inspect mcp-rs-postgres-primary | grep -A 10 "Health"
```

### Secondary レプリケーション失敗

```bash
# Secondary ログ確認
docker-compose -f docker-compose.postgres.yml logs postgres-secondary

# Primary でレプリケーションスロット確認
psql -h localhost -p 5432 -U postgres -d postgres \
  -c "SELECT * FROM pg_replication_slots;"

# Primary で接続確認
psql -h localhost -p 5432 -U postgres -d postgres \
  -c "SELECT * FROM pg_stat_replication;"
```

## パフォーマンス最適化

### Primary 構成パラメータ

`docker-compose.postgres.yml` の `POSTGRES_INITDB_ARGS` で調整：

```yaml
POSTGRES_INITDB_ARGS: >-
  -c max_connections=200        # 最大接続数
  -c shared_buffers=256MB       # 共有バッファ
  -c effective_cache_size=1GB   # キャッシュサイズ
  -c work_mem=8MB              # 作業メモリ
```

### テスト環境最適化

開発/テスト用に以下を推奨：

```bash
# .env で設定
POSTGRES_MEMORY=512MB
POSTGRES_CPU_SHARES=1024

# docker-compose.yml に追加
mem_limit: 512m
cpus: '1.0'
```

## セキュリティ注意事項

⚠️ **本番環境では使用しないこと**

開発・テスト専用です：

- デフォルトユーザー/パスワード使用
- SSL/TLS 設定なし
- ローカルホストのみ公開

本番環境での設定：

```bash
# .env で強力なパスワード設定
POSTGRES_PASSWORD=very_strong_password_here
PGADMIN_PASSWORD=very_strong_password_here

# SSL/TLS 有効化
# 証明書ファイル配置
# docker-compose.yml で ssl_cert_file 設定
```

## ヘルスチェック

コンテナの自動ヘルスチェック設定：

```yaml
healthcheck:
  test: ["CMD-SHELL", "pg_isready -U postgres -d testdb"]
  interval: 10s
  timeout: 5s
  retries: 5
  start_period: 10s
```

## ログ管理

### ログローテーション

`docker-compose.postgres.yml` で設定：

```yaml
logging:
  driver: "json-file"
  options:
    max-size: "10m"      # 単一ログファイル最大 10MB
    max-file: "3"        # 保持ファイル数
```

### ログ確認

```bash
# リアルタイムログ
docker-compose -f docker-compose.postgres.yml logs -f

# 最新 100 行
docker-compose -f docker-compose.postgres.yml logs --tail=100
```

## 環境変数リファレンス

| 変数名 | デフォルト | 説明 |
|--------|----------|------|
| `POSTGRES_USER` | postgres | PostgreSQL ユーザー名 |
| `POSTGRES_PASSWORD` | postgres | PostgreSQL パスワード |
| `POSTGRES_DB` | testdb | デフォルトデータベース |
| `POSTGRES_PORT` | 5432 | Primary ポート |
| `POSTGRES_SECONDARY_PORT` | 5433 | Secondary ポート |
| `PGADMIN_EMAIL` | admin@example.com | pgAdmin ログインメール |
| `PGADMIN_PASSWORD` | admin | pgAdmin ログインパスワード |
| `PGADMIN_PORT` | 5050 | pgAdmin Web ポート |

## 詳細情報

- [PostgreSQL 15 ドキュメント](https://www.postgresql.org/docs/15/)
- [Docker Compose ドキュメント](https://docs.docker.com/compose/)
- [pgAdmin ドキュメント](https://www.pgadmin.org/docs/)
