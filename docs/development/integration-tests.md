# サーバーが要る結合テスト

PostgreSQL・Redisのサーバーが要るテストは`#[ignore]`にしてあり、`cargo test`では実行されない。CIでは`container-integration-tests.yml`の「Run database integration tests」のステップが、サービスのコンテナに対して`--ignored`で実行する（#513）。

## 接続先

接続先は環境変数で渡す（`tests/common/mod.rs`）。`--ignored`で実行したのに変数が無いときや、つながらないときは、テストを失敗にする。

**使い捨てのデータベースを指すこと。** テストは準備で、`test_schema`のスキーマを`DROP … CASCADE`で消して作り直す。ほかにも`test_`・`microseg_it_`で始まる表を作って消す。Redisは`test:`等で始まるキーを書いて消す。データベースのツールのテスト（下）は、もっと大きく消す: `database_redis_tools_test`は、Redisのデータベースの番号9・10・11を`FLUSHDB`で空にし、`database_mongodb_tools_test`は、決まった名前（`mcp_rs_tools_read`等）のMongoDBのデータベースを消す。共有のサーバーを指さないこと（#810）。

| 変数 | CIの値 |
|---|---|
| `TEST_DATABASE_URL` | `postgres://testuser:testpass@localhost:5433/mcptest` |
| `TEST_REDIS_URL` | `redis://localhost:6380/0` |
| `TEST_MYSQL_URL` | `mysql://root:testpass@127.0.0.1:3307/mcptest` |
| `TEST_MARIADB_URL` | `mysql://root:testpass@127.0.0.1:3308/mcptest` |
| `TEST_MONGODB_URL` | `mongodb://localhost:27018` |

## 対象

| データベース | テスト（`--test`） |
|---|---|
| PostgreSQL（`engines::postgresql`、本体のハンドラーが使う） | `mod`の`database::postgresql_integration_tests`・`database::postgres_database_integration_tests`、`database_handler_micro_segmentation_test` |
| Redis | `mod`の`database::redis_integration_test`・`database::redis_integration_tests` |

古いPostgreSQLの実装（`engines::postgres`）とそのテストは、本体から使われていなかったので消した（#638）。

データベースのツール（`[database]`のハンドラー）のテストは`#[ignore]`ではなく、変数があれば実行し、無ければ飛ばす。CIでは、それぞれのステップ（「Run database tool tests」）が実行する。

| データベース | テスト（`--test`） | 変数 |
|---|---|---|
| PostgreSQL（#453） | `database_postgres_tools_test` | `TEST_DATABASE_URL` |
| MySQL・MariaDB（#527） | `database_mysql_tools_test` | `TEST_MYSQL_URL`・`TEST_MARIADB_URL` |
| Redis（#529） | `database_redis_tools_test` | `TEST_REDIS_URL` |
| MongoDB（#530） | `database_mongodb_tools_test` | `TEST_MONGODB_URL` |

SQLite（#528）は、テストの中でファイルを作るので、サーバーが要らない（`mod`の`database::sqlite_integration_tests`）。`sqlite`のfeatureが要る（既定では入っていない）: `cargo test --features sqlite --test mod database::sqlite_integration_tests`。MySQLのエンジンの結合テスト（`mod`の`database::mysql_integration_tests`）は`#[ignore]`で、CIは`TEST_MYSQL_URL`を渡して`--ignored`で実行する。

## ローカルで実行する

CIと同じ設定のコンテナを起動する。ポートは、ほかのコンピューターから届かないよう`127.0.0.1`にだけ開く（MongoDB・Redisのコンテナは認証が無い）:

```bash
docker run -d --name mcp-rs-test-postgres -p 127.0.0.1:5433:5432 \
  -e POSTGRES_DB=mcptest -e POSTGRES_USER=testuser -e POSTGRES_PASSWORD=testpass \
  postgres:16-alpine
docker run -d --name mcp-rs-test-redis -p 127.0.0.1:6380:6379 redis:7-alpine
# データベースのツールのテストも実行するなら
docker run -d --name mcp-rs-test-mysql -p 127.0.0.1:3307:3306 \
  -e MYSQL_ROOT_PASSWORD=testpass -e MYSQL_DATABASE=mcptest mysql:8.4
docker run -d --name mcp-rs-test-mariadb -p 127.0.0.1:3308:3306 \
  -e MARIADB_ROOT_PASSWORD=testpass -e MARIADB_DATABASE=mcptest mariadb:11.4
docker run -d --name mcp-rs-test-mongodb -p 127.0.0.1:27018:27017 mongo:7
```

起動したばかりのコンテナは、初期化の間はつながらない（MySQL・MariaDBは数十秒かかる）。テストの前に、どれも答えるまで待つ（起動していないコンテナの行は飛ばす）:

```bash
until docker exec mcp-rs-test-postgres pg_isready -U testuser > /dev/null; do sleep 1; done
until docker exec mcp-rs-test-redis redis-cli ping > /dev/null; do sleep 1; done
until docker exec mcp-rs-test-mysql mysqladmin ping -h 127.0.0.1 -ptestpass --silent; do sleep 1; done
until docker exec mcp-rs-test-mariadb mariadb-admin ping -h 127.0.0.1 -ptestpass --silent; do sleep 1; done
until docker exec mcp-rs-test-mongodb mongosh --quiet --eval 'db.runCommand({ ping: 1 })' > /dev/null; do sleep 1; done
```

テストを実行する。同じデータベース・キーを使うテストがあるので、`--test-threads=1`で1つずつ実行する:

```bash
export TEST_DATABASE_URL=postgres://testuser:testpass@localhost:5433/mcptest
export TEST_REDIS_URL=redis://localhost:6380/0

cargo test --features redis-backend --no-fail-fast \
  --test mod \
  --test database_handler_micro_segmentation_test \
  -- --ignored --test-threads=1
```

データベースのツールのテストは、変数を設定して、それぞれを実行する（`--ignored`は要らない）:

```bash
export TEST_MYSQL_URL=mysql://root:testpass@127.0.0.1:3307/mcptest
export TEST_MARIADB_URL=mysql://root:testpass@127.0.0.1:3308/mcptest
export TEST_MONGODB_URL=mongodb://localhost:27018

cargo test --test database_postgres_tools_test --test database_mysql_tools_test
cargo test --features redis-backend --test database_redis_tools_test -- --test-threads=1
cargo test --features mongodb-backend --test database_mongodb_tools_test -- --test-threads=1
# MySQLのエンジンの結合テスト（#[ignore]。CIと同じく、変数を設定した後に）
cargo test --features database,mysql-backend --test mod database::mysql_integration_tests -- --ignored --test-threads=1
```

PowerShellでは、変数を`$env:TEST_DATABASE_URL = "postgres://..."`のように設定する。

終わったらコンテナを消す:

```bash
docker rm -f mcp-rs-test-postgres mcp-rs-test-redis mcp-rs-test-mysql mcp-rs-test-mariadb mcp-rs-test-mongodb
```
