# サーバーが要る結合テスト

PostgreSQL・Redisのサーバーが要るテストは`#[ignore]`にしてあり、`cargo test`では実行されない。CIでは`container-integration-tests.yml`の「Run database integration tests」のステップが、サービスのコンテナに対して`--ignored`で実行する（#513）。

## 接続先

接続先は環境変数で渡す（`tests/common/mod.rs`）。`--ignored`で実行したのに変数が無いときや、つながらないときは、テストを失敗にする。

| 変数 | CIの値 |
|---|---|
| `TEST_DATABASE_URL` | `postgres://testuser:testpass@localhost:5433/mcptest` |
| `TEST_REDIS_URL` | `redis://localhost:6380/0` |

## 対象

| データベース | テスト（`--test`） |
|---|---|
| PostgreSQL（`engines::postgresql`、本体のハンドラーが使う） | `mod`の`database::postgresql_integration_tests`・`database::postgres_database_integration_tests`、`database_handler_column_encryption_test`・`database_handler_micro_segmentation_test`・`column_encryption_rbac_test` |
| PostgreSQL（`engines::postgres`） | `postgres_engine_test`・`postgres_jsonb_migration_test`・`postgres_transaction_test`・`postgres_integration_test` |
| Redis | `mod`の`database::redis_integration_test`・`database::redis_integration_tests` |

`database_postgres_tools_test`（データベースのツール、#453）は`#[ignore]`ではなく、`TEST_DATABASE_URL`があれば実行し、無ければ飛ばす。

MySQL・SQLite・MongoDBのテストは、本体から使えるようにするIssue（#527・#528・#530）で足す。

## ローカルで実行する

CIと同じ設定のコンテナを起動する:

```bash
docker run -d --name mcp-rs-test-postgres -p 5433:5432 \
  -e POSTGRES_DB=mcptest -e POSTGRES_USER=testuser -e POSTGRES_PASSWORD=testpass \
  postgres:16-alpine
docker run -d --name mcp-rs-test-redis -p 6380:6379 redis:7-alpine
```

テストを実行する。同じデータベース・キーを使うテストがあるので、`--test-threads=1`で1つずつ実行する:

```bash
export TEST_DATABASE_URL=postgres://testuser:testpass@localhost:5433/mcptest
export TEST_REDIS_URL=redis://localhost:6380/0

cargo test --features redis-backend --no-fail-fast \
  --test mod \
  --test postgres_engine_test --test postgres_jsonb_migration_test \
  --test postgres_transaction_test --test postgres_integration_test \
  --test database_handler_column_encryption_test \
  --test database_handler_micro_segmentation_test \
  --test column_encryption_rbac_test \
  -- --ignored --test-threads=1
```

PowerShellでは、変数を`$env:TEST_DATABASE_URL = "postgres://..."`のように設定する。

終わったらコンテナを消す:

```bash
docker rm -f mcp-rs-test-postgres mcp-rs-test-redis
```
