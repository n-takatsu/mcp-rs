# PostgreSQL Docker Compose ガイド

MCP-RS Phase 2 PostgreSQL テスト環境を Docker Compose で構築・管理するためのガイドです。

> **使うファイル**: このガイドは、テスト・開発用の `configs/docker/docker-compose.postgres.yml` を使います。リポジトリの直下の `docker-compose.postgres.yml` は別物で、`docker-compose.yml`（HTTPモードのmcp-rs）と一緒に使うPostgreSQLです。そちらは [DOCKER_GUIDE.md](DOCKER_GUIDE.md#docker-compose) を見てください。
>
> **パスワード**: テスト環境は、`POSTGRES_PASSWORD` が無いか空なら、`postgres` を使います（`.env.example` は空です）。pgAdminの既定のパスワードも `admin` です。どれも誰でも知っているパスワードなので、ポート（5432・5050）は、このPCの中（`127.0.0.1`）だけに公開します（v0.24.0から。以前は、同じネットワークの誰からも接続できました）。ほかのPCから使うときは、ポートの指定を変え、パスワードも必ず自分のものに変えてください。

## 概要

このセットアップは以下を提供します：

- **PostgreSQL Primary (15-Alpine)** - メインデータベースサーバー
- **pgAdmin 4** - Web ベースのデータベース管理ツール

## システム要件

- Docker & Docker Compose
- 最低 2GB RAM
- ポート開放: 5432, 5050

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

> **セカンダリ（`postgres-secondary`）は、v0.30.0で外しました**（#732）。レプリケーションの設定が PostgreSQL 15 に合っておらず起動しなかったうえ、使うテストもありませんでした。以前の手順でセカンダリを起動していたら、そのコンテナとボリュームが残っています。コンテナは`--remove-orphans`で消せます。ボリュームは、名前を確かめてから消してください:
>
> ```bash
> docker compose -f configs/docker/docker-compose.postgres.yml up -d --remove-orphans
> docker volume ls --filter name=postgres_secondary_data
> docker volume rm <確かめた名前>
> ```

```bash
# Primary・pgAdminをバックグラウンドで起動
docker compose -f configs/docker/docker-compose.postgres.yml up -d

# Primaryだけ（テストにはこれで足りる）
docker compose -f configs/docker/docker-compose.postgres.yml up -d postgres-primary

# ログを表示しながら起動
docker compose -f configs/docker/docker-compose.postgres.yml up
```

> **初期化のSQL（`scripts/postgres/init.sql`・`test-schema.sql`）は、ボリュームが空のときの最初の起動でだけ実行されます。** 以前の版で初期化が途中で止まったボリュームが残っていると、スキーマが足りないままになります。その場合は、Primaryのコンテナとボリュームだけを消してから起動し直してください（Primaryのデータも消えます）。`down -v`は、pgAdminのボリュームも消します:
>
> ```bash
> docker compose -f configs/docker/docker-compose.postgres.yml rm -sf postgres-primary
> # ボリュームの名前を確かめる（Composeが前にプロジェクトの名前を付ける。例: docker_postgres_primary_data）
> docker volume ls --filter name=postgres_primary_data
> docker volume rm <確かめた名前>
> docker compose -f configs/docker/docker-compose.postgres.yml up -d postgres-primary
> ```

### 3. 接続確認

```bash
# Primary への接続テスト
psql -h localhost -p 5432 -U postgres -d testdb -c "SELECT version();"

# pgAdmin へアクセス（pgAdminも起動したとき）
# http://localhost:5050
# Email: admin@example.com
# Password: admin
```

## コンテナ管理

### ステータス確認

```bash
# 全コンテナのステータス表示
docker compose -f configs/docker/docker-compose.postgres.yml ps

# ログ表示
docker compose -f configs/docker/docker-compose.postgres.yml logs -f

# 特定コンテナのログ
docker compose -f configs/docker/docker-compose.postgres.yml logs -f postgres-primary
```

### コンテナ停止・再起動

```bash
# 全コンテナ停止
docker compose -f configs/docker/docker-compose.postgres.yml stop

# 全コンテナ再起動
docker compose -f configs/docker/docker-compose.postgres.yml restart

# Primaryだけ再起動
docker compose -f configs/docker/docker-compose.postgres.yml restart postgres-primary

# コンテナ削除（データ保持）
docker compose -f configs/docker/docker-compose.postgres.yml down

# コンテナとボリューム削除（データ削除）
docker compose -f configs/docker/docker-compose.postgres.yml down -v
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
docker compose -f configs/docker/docker-compose.postgres.yml down -v

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

コンテナ内では `postgres-primary` のホスト名で接続可能：

```bash
postgresql://postgres:postgres@postgres-primary:5432/testdb
```

## トラブルシューティング

### PostgreSQL 起動エラー

```bash
# ログ確認
docker compose -f configs/docker/docker-compose.postgres.yml logs postgres-primary

# ボリュームクリア（すべてのデータを削除。pgAdminのボリュームも消える。
# Primaryだけを消すときは「2. コンテナ起動」の手順）
docker compose -f configs/docker/docker-compose.postgres.yml down -v

# 起動し直す
docker compose -f configs/docker/docker-compose.postgres.yml up -d
```

### ポート競合

```bash
# ポート確認
netstat -tulpn | grep 5432

# 別ポート指定
POSTGRES_PORT=5432 docker compose -f configs/docker/docker-compose.postgres.yml up -d postgres-primary
```

### 接続タイムアウト

```bash
# ヘルスチェック確認
docker compose -f configs/docker/docker-compose.postgres.yml ps

# コンテナのヘルスチェックログ確認
docker inspect mcp-rs-postgres-primary | grep -A 10 "Health"
```

## パフォーマンス最適化

### Primary 構成パラメータ

`configs/docker/docker-compose.postgres.yml` の `POSTGRES_INITDB_ARGS` で調整：

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

`configs/docker/docker-compose.postgres.yml` で設定：

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
docker compose -f configs/docker/docker-compose.postgres.yml logs -f

# 最新 100 行
docker compose -f configs/docker/docker-compose.postgres.yml logs --tail=100
```

## 環境変数リファレンス

| 変数名 | デフォルト | 説明 |
|--------|----------|------|
| `POSTGRES_USER` | postgres | PostgreSQL ユーザー名 |
| `POSTGRES_PASSWORD` | postgres | PostgreSQL パスワード |
| `POSTGRES_DB` | testdb | デフォルトデータベース |
| `POSTGRES_PORT` | 5432 | Primary ポート |
| `PGADMIN_EMAIL` | admin@example.com | pgAdmin ログインメール |
| `PGADMIN_PASSWORD` | admin | pgAdmin ログインパスワード |
| `PGADMIN_PORT` | 5050 | pgAdmin Web ポート |

## 詳細情報

- [PostgreSQL 15 ドキュメント](https://www.postgresql.org/docs/15/)
- [Docker Compose ドキュメント](https://docs.docker.com/compose/)
- [pgAdmin ドキュメント](https://www.pgadmin.org/docs/)
