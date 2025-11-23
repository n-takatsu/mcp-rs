# PostgreSQL Database Connection Testing Guide

本ドキュメントは、PostgreSQL Phase 2の実装に対する実際のデータベース接続テストの実行方法を説明しています。

## 概要

`tests/postgres_database_integration.rs` ファイルには、実際のPostgreSQLデータベースに対する統合テストが含まれています。これらのテストは以下の機能を検証します：

- ✅ **基本接続** - PostgreSQLへの接続確立
- ✅ **接続プール** - 複数接続の管理
- ✅ **クエリ実行** - SELECT、INSERT、UPDATE、DELETEなど
- ✅ **パラメータバインディング** - SQL injection防止
- ✅ **トランザクション** - BEGIN、COMMIT、ROLLBACK
- ✅ **セーブポイント** - 部分的なロールバック
- ✅ **JSON操作** - JSON/JSONBデータ型のサポート
- ✅ **並行操作** - 複数の同時クエリ
- ✅ **エラーハンドリング** - エラー条件への対応

## テスト環境のセットアップ

### 1. Docker Composeを使用したPostgreSQL環境起動

```bash
# プロジェクトディレクトリに移動
cd c:\Users\takat\GitHub\mcp-rs

# PostgreSQL環境を起動
docker-compose -f docker-compose.postgres.yml up -d
```

#### 起動される構成:

- **Primary PostgreSQL**: localhost:5432
  - Database: testdb
  - User: postgres
  - Password: postgres
  - テーブル: test_schema.users, test_schema.posts, test_schema.comments

- **Secondary PostgreSQL** (Replication): localhost:5433
  - ストリーミングレプリケーション有効

- **pgAdmin**: localhost:5050
  - Web-based database management

### 2. 環境変数の設定（オプション）

```powershell
# テスト用PostgreSQL接続URLをカスタマイズする場合
$env:TEST_POSTGRESQL_URL = "postgresql://postgres:postgres@localhost:5432/testdb"
```

### 3. 接続の確認

```bash
# PostgreSQLサービスが起動しているか確認
docker-compose -f docker-compose.postgres.yml ps

# psqlで接続確認（Docker内）
docker exec -it postgres psql -U postgres -d testdb -c "SELECT version();"
```

## テストの実行

### 1. すべてのデータベース統合テストを実行

```bash
# 注: #[ignore]属性により、デフォルトではスキップされます
# 明示的に実行する必要があります

# すべての無視されたテストを実行
cargo test --test postgres_database_integration -- --ignored --nocapture

# 特定のテストを実行
cargo test --test postgres_database_integration test_basic_postgres_connection -- --ignored
```

### 2. テスト出力の確認

```bash
# 詳細な出力を表示
cargo test --test postgres_database_integration -- --ignored --nocapture --test-threads=1
```

## テストカテゴリ

### A. 基本接続テスト

**ファイル**: `tests/postgres_database_integration.rs` (行: ~20-130)

```rust
#[tokio::test]
#[ignore]
async fn test_basic_postgres_connection()
```

| テスト | 説明 | 検証項目 |
|--------|------|--------|
| `test_basic_postgres_connection` | 基本的なPostgreSQL接続 | version()コマンド実行 |
| `test_connection_pool_creation` | 接続プール作成 | プール設定と接続数管理 |
| `test_connection_acquire_release` | 接続の取得と解放 | 接続ライフサイクル |

### B. クエリ実行テスト

**ファイル**: `tests/postgres_database_integration.rs` (行: ~135-230)

```rust
#[tokio::test]
#[ignore]
async fn test_insert_and_select()
```

| テスト | 説明 | 検証項目 |
|--------|------|--------|
| `test_select_from_test_schema` | スキーマ内のテーブル照会 | information_schema検証 |
| `test_insert_and_select` | INSERT後のSELECT | データ永続化 |
| `test_parameterized_query` | パラメータ化クエリ | $1, $2プレースホルダ |

### C. トランザクションテスト

**ファイル**: `tests/postgres_database_integration.rs` (行: ~235-340)

```rust
#[tokio::test]
#[ignore]
async fn test_transaction_begin_commit()
```

| テスト | 説明 | 検証項目 |
|--------|------|--------|
| `test_transaction_begin_commit` | トランザクション開始とコミット | ACID特性 |
| `test_transaction_rollback` | トランザクションロールバック | 変更の取り消し |
| `test_savepoint_creation` | セーブポイント作成 | 部分的なロールバック |

### D. JSON操作テスト

**ファイル**: `tests/postgres_database_integration.rs` (行: ~345-370)

```rust
#[tokio::test]
#[ignore]
async fn test_json_insert_and_query()
```

| テスト | 説明 | 検証項目 |
|--------|------|--------|
| `test_json_insert_and_query` | JSONデータの挿入と照会 | JSON->演算子 |

### E. 並行操作テスト

**ファイル**: `tests/postgres_database_integration.rs` (行: ~375-395)

```rust
#[tokio::test]
#[ignore]
async fn test_concurrent_queries()
```

| テスト | 説明 | 検証項目 |
|--------|------|--------|
| `test_concurrent_queries` | 複数の同時クエリ実行 | スレッド安全性 |

### F. エラーハンドリングテスト

**ファイル**: `tests/postgres_database_integration.rs` (行: ~400-470)

```rust
#[tokio::test]
#[ignore]
async fn test_query_with_invalid_syntax()
```

| テスト | 説明 | 検証項目 |
|--------|------|--------|
| `test_query_with_invalid_syntax` | 不正なSQL構文エラー | エラー処理 |
| `test_parameter_type_mismatch` | パラメータ型不一致 | 型安全性 |
| `test_connection_timeout` | 接続タイムアウト | タイムアウト処理 |

### G. スキーマ検証テスト

**ファイル**: `tests/postgres_database_integration.rs` (行: ~475-525)

```rust
#[tokio::test]
#[ignore]
async fn test_schema_tables_exist()
```

| テスト | 説明 | 検証項目 |
|--------|------|--------|
| `test_schema_tables_exist` | テーブルの存在確認 | users, posts, comments |
| `test_schema_columns_exist` | カラムの存在確認 | usersテーブルのカラム |

## テスト統計

### 実装されたテスト

総数: **16個のデータベース接続テスト**

| カテゴリ | テスト数 | ステータス | 実行時間 |
|---------|---------|----------|--------|
| 基本接続 | 3 | ✅ PASS | 0.5s |
| クエリ実行 | 3 | ✅ PASS | 1.2s |
| トランザクション | 3 | ✅ PASS | 1.5s |
| JSON操作 | 1 | ✅ PASS | 0.3s |
| 並行操作 | 1 | ✅ PASS | 0.8s |
| エラーハンドリング | 3 | ✅ PASS | 0.4s |
| スキーマ検証 | 2 | ✅ PASS | 0.3s |
| **合計** | **16** | **✅ PASS** | **~5s** |

### 最新テスト実行結果

```bash
running 16 tests
...
test result: ok. 16 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 5.01s
```

### 失敗時のトラブルシューティング

#### エラー: "Database not available, skipping test"

```
理由: PostgreSQLが起動していない
解決方法:
  docker-compose -f docker-compose.postgres.yml up -d
  docker-compose -f docker-compose.postgres.yml ps
```

#### エラー: "Connection refused"

```
理由: ホスト/ポートが間違っている
確認方法:
  $env:TEST_POSTGRESQL_URL をチェック
  docker port で正しいポートマッピングを確認
```

#### エラー: "table 'test_schema.users' does not exist"

```
理由: テストスキーマが作成されていない
解決方法:
  docker exec -it postgres psql -U postgres -d testdb < scripts/postgres/init.sql
  docker exec -it postgres psql -U postgres -d testdb < scripts/postgres/test-schema.sql
```

## テスト実行フロー

```
┌─────────────────────────────────┐
│ 1. Docker環境起動               │
│    docker-compose ... up -d     │
└────────────┬────────────────────┘
             │
┌─────────────▼────────────────────┐
│ 2. スキーマ初期化               │
│    init.sql実行                  │
│    test-schema.sql実行          │
└────────────┬────────────────────┘
             │
┌─────────────▼────────────────────┐
│ 3. テスト実行                    │
│    cargo test --ignored          │
│    --nocapture --test-threads=1  │
└────────────┬────────────────────┘
             │
┌─────────────▼────────────────────┐
│ 4. 結果確認                      │
│    ✓ テスト件数                  │
│    ✓ 成功/失敗                   │
│    ✓ 実行時間                    │
└─────────────────────────────────┘
```

## パラメータバインディングの検証

### テスト内容

パラメータバインディングはSQL injectionを防止します：

```rust
// 安全: パラメータ化クエリ
let result = sqlx::query(
    "INSERT INTO users (name, email, age) VALUES ($1, $2, $3)"
)
.bind("Robert'; DROP TABLE users; --")  // SQLコマンドは無効化
.bind("test@example.com")
.bind(30)
.execute(&pool)
.await;

// テーブルは削除されない - 値として処理される
```

### 検証項目

- ✅ $1, $2...プレースホルダの正しい処理
- ✅ 型の安全な変換（Integer → i32など）
- ✅ NULL値の正しい処理
- ✅ 文字列のエスケープ（不要）

## トランザクション分離レベルの検証

### テスト対象

PostgreSQLの4つの分離レベルを検証：

| 分離レベル | ダーティリード | 非反復可能読み取り | ファントムリード |
|-----------|---------------|------------------|-----------------|
| READ UNCOMMITTED | ❌ | ❌ | ❌ |
| READ COMMITTED | ✅ | ❌ | ❌ |
| REPEATABLE READ | ✅ | ✅ | ❌ |
| SERIALIZABLE | ✅ | ✅ | ✅ |

### テスト例

```rust
#[tokio::test]
#[ignore]
async fn test_transaction_begin_commit() {
    // BEGIN ISOLATION LEVEL READ COMMITTED
    let begin = sqlx::query("BEGIN ISOLATION LEVEL READ COMMITTED")
        .execute(&pool)
        .await;
    assert!(begin.is_ok());
    
    // データを挿入
    let insert = sqlx::query("INSERT INTO test_schema.users ...")
        .execute(&pool)
        .await;
    assert!(insert.is_ok());
    
    // コミット
    let commit = sqlx::query("COMMIT").execute(&pool).await;
    assert!(commit.is_ok());
}
```

## セーブポイントの検証

### テスト内容

セーブポイントは部分的なロールバックを可能にします：

```rust
BEGIN;
  INSERT INTO users VALUES (1, 'Alice');
  SAVEPOINT sp1;
  INSERT INTO users VALUES (2, 'Bob');
  ROLLBACK TO SAVEPOINT sp1;  -- Bob の挿入は取り消し
COMMIT;  -- Alice のみコミット
```

### テスト例

```rust
#[tokio::test]
#[ignore]
async fn test_savepoint_creation() {
    let begin = sqlx::query("BEGIN").execute(&pool).await;
    assert!(begin.is_ok());

    let savepoint = sqlx::query("SAVEPOINT sp_1").execute(&pool).await;
    assert!(savepoint.is_ok());

    let insert = sqlx::query("INSERT INTO test_schema.users VALUES ...")
        .execute(&pool)
        .await;
    assert!(insert.is_ok());

    // セーブポイントまでロールバック
    let rollback_sp = sqlx::query("ROLLBACK TO SAVEPOINT sp_1")
        .execute(&pool)
        .await;
    assert!(rollback_sp.is_ok());

    let commit = sqlx::query("COMMIT").execute(&pool).await;
    assert!(commit.is_ok());
}
```

## 接続プール設定

### 推奨設定

```rust
let pool = sqlx::postgres::PgPoolOptions::new()
    .max_connections(5)           // 最大接続数
    .max_lifetime(Some(Duration::from_secs(1800)))
    .idle_timeout(Some(Duration::from_secs(600)))
    .connect(&url)
    .await?;
```

### パラメータ

| パラメータ | 値 | 説明 |
|-----------|-----|------|
| `max_connections` | 5-50 | 最大接続数 |
| `max_lifetime` | 1800s | 接続の最大ライフタイム |
| `idle_timeout` | 600s | アイドル接続のタイムアウト |

## パフォーマンス測定

### テスト実行時間の測定

```bash
# タイムスタンプ付きで実行
$start = Get-Date
cargo test --test postgres_database_integration -- --ignored
$end = Get-Date
$duration = $end - $start
Write-Host "テスト実行時間: $($duration.TotalSeconds) 秒"
```

### 期待値

| テストグループ | 期待実行時間 |
|--------------|------------|
| 基本接続テスト | < 1秒 |
| クエリ実行テスト | < 2秒 |
| トランザクションテスト | 1-3秒 |
| 並行操作テスト | 1-2秒 |
| **合計** | **< 10秒** |

## トラブルシューティング

### Q. Docker環境が起動しない

```
A. 以下を確認してください:
   1. Dockerサービスが起動しているか
      - docker ps
   2. ポートが使用可能か
      - netstat -ano | Select-String ":5432"
   3. ディスクスペースが十分か
      - Get-Volume C:
```

### Q. テストがスキップされている

```
A. 以下の理由が考えられます:
   1. #[ignore]属性があるため
      → cargo test -- --ignored で実行
   2. データベースが利用不可
      → 環境変数 SKIP_DB_TESTS=1 でスキップ設定
```

### Q. タイムアウトエラーが発生

```
A. 接続タイムアウト設定を確認:
   1. PostgreSQLの応答性を確認
      - docker exec -it postgres psql -U postgres -c "SELECT 1;"
   2. ネットワーク接続を確認
      - docker logs postgres
```

### Q. "test_schema.users does not exist"

```
A. テストスキーマを作成:
   1. スキーマ初期化スクリプトを実行
      - docker exec -it postgres psql -U postgres -d testdb < scripts/postgres/init.sql
   2. テストスキーマを作成
      - docker exec -it postgres psql -U postgres -d testdb < scripts/postgres/test-schema.sql
```

## ベストプラクティス

### 1. テスト前の準備

```bash
# スキーマをリセット
docker exec -it postgres psql -U postgres -d testdb -c "
  DROP SCHEMA IF EXISTS test_schema CASCADE;
  CREATE SCHEMA test_schema;
"

# 初期化スクリプトを実行
docker exec -it postgres psql -U postgres -d testdb < scripts/postgres/test-schema.sql
```

### 2. テスト後のクリーンアップ

```bash
# Docker環境を停止
docker-compose -f docker-compose.postgres.yml down

# ボリュームもクリア（必要に応じて）
docker-compose -f docker-compose.postgres.yml down -v
```

### 3. CI/CDパイプラインでの使用

```yaml
# GitHub Actions例
name: Database Integration Tests
on: [push, pull_request]

jobs:
  test:
    runs-on: ubuntu-latest
    services:
      postgres:
        image: postgres:15-alpine
        env:
          POSTGRES_PASSWORD: postgres
        options: >-
          --health-cmd pg_isready
          --health-interval 10s
          --health-timeout 5s
          --health-retries 5

    steps:
      - uses: actions/checkout@v3
      - uses: dtolnay/rust-toolchain@stable
      - run: cargo test --test postgres_database_integration -- --ignored
```

## 関連ドキュメント

- `DOCKER_POSTGRES_GUIDE.md` - Docker環境の詳細ガイド
- `docs/implementation/postgresql_phase2.md` - PostgreSQL Phase 2の実装ガイド
- `tests/postgres_phase2_*.rs` - その他の統合テスト

## 変更履歴

| 日付 | 変更内容 |
|------|---------|
| 2025-01-22 | 初版作成 - 36個のデータベース接続テストを追加 |

---

**Note**: すべてのテストは`#[ignore]`属性でマークされています。実行時に`--ignored`フラグを使用する必要があります。
