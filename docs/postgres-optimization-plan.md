# PostgreSQL最適化 段階的実装計画

## 📅 Issue #214: Database Integration Phase 2 - PostgreSQL最適化

**作成日**: 2026年2月1日  
**優先度**: P1 (High)  
**期限**: 2026年1月31日 (延期検討中)  
**総工数**: 3週間

---

## 🎯 実装の段階的アプローチ

### フェーズ構成

```
Phase 1: 基盤構築 (Week 1)          ━━━━━━━━━━ 40%
Phase 2: 高度な機能 (Week 2)        ━━━━━━━━━━ 35%
Phase 3: 最適化・テスト (Week 3)    ━━━━━━━━━━ 25%
```

---

## 📋 Phase 1: PostgreSQL Backend基盤 (Week 1 - 5日間)

### Day 1: プロジェクト構造とコア実装

#### タスク 1.1: ディレクトリ構造作成
```bash
# 実行コマンド
mkdir -p src/handlers/database/engines/postgres
touch src/handlers/database/engines/postgres/{mod.rs,config.rs,pool.rs,error.rs}
```

**作成ファイル**:
- `src/handlers/database/engines/postgres/mod.rs`
- `src/handlers/database/engines/postgres/config.rs`
- `src/handlers/database/engines/postgres/pool.rs`
- `src/handlers/database/engines/postgres/error.rs`

**実装内容**:
```rust
// mod.rs
pub mod config;
pub mod pool;
pub mod error;

pub use config::PostgresConfig;
pub use pool::{create_optimized_pool, OptimizedPoolConfig};
pub use error::PostgresError;

pub struct PostgresEngine {
    pool: sqlx::PgPool,
    config: PostgresConfig,
}
```

#### タスク 1.2: 依存関係追加
```toml
# Cargo.toml
[dependencies]
sqlx = { version = "0.7", features = [
    "runtime-tokio",
    "postgres", 
    "json",
    "uuid",
    "chrono",
    "migrate"
] }
tokio-postgres = "0.7"
```

**成果物**: ✅ 基本プロジェクト構造完成

---

### Day 2: 接続プール最適化

#### タスク 2.1: OptimizedPoolConfig実装
**ファイル**: `src/handlers/database/engines/postgres/pool.rs`

**実装コンポーネント**:
- [ ] `OptimizedPoolConfig` 構造体
- [ ] `create_optimized_pool()` 関数
- [ ] ヘルスチェック機能
- [ ] 接続プールメトリクス

**検証方法**:
```bash
cargo test postgres_pool_creation
cargo test postgres_health_check
```

#### タスク 2.2: 設定ファイル作成
**ファイル**: `configs/database/postgres.toml`

```toml
[connection_pool]
min_connections = 5
max_connections = 100
connection_timeout_secs = 30
idle_timeout_secs = 600
max_lifetime_secs = 1800
```

**成果物**: ✅ 最適化された接続プール

---

### Day 3: トランザクション管理

#### タスク 3.1: PostgreSQL拡張トランザクション
**ファイル**: `src/handlers/database/engines/postgres/transaction.rs`

**実装機能**:
- [ ] Savepoint サポート
- [ ] 4つのIsolation Level
- [ ] ネストトランザクション
- [ ] デッドロック検出

**コード例**:
```rust
impl PostgresEngine {
    pub async fn begin_transaction(&self) -> Result<PostgresTransaction> {
        let mut tx = self.pool.begin().await?;
        Ok(PostgresTransaction { tx, savepoints: vec![] })
    }
}
```

**テスト**:
```bash
cargo test postgres_transaction_savepoint
cargo test postgres_transaction_isolation
```

**成果物**: ✅ トランザクション管理完成

---

### Day 4-5: DatabaseEngineトレイト実装

#### タスク 4.1: execute_query実装
```rust
impl DatabaseEngine for PostgresEngine {
    async fn execute_query(&self, query: &str, params: &[Value]) -> Result<QueryResult> {
        let mut query_builder = sqlx::query(query);
        for param in params {
            query_builder = bind_value(query_builder, param)?;
        }
        let result = query_builder.fetch_all(&self.pool).await?;
        Ok(QueryResult::from_rows(result))
    }
}
```

#### タスク 4.2: Prepared Statement キャッシュ
- [ ] ステートメントキャッシュ実装
- [ ] キャッシュヒット率測定
- [ ] LRU eviction 戦略

**テスト**:
```bash
cargo test postgres_prepared_statement_cache
cargo test postgres_query_execution
```

**成果物**: ✅ 基本クエリ実行機能完成

---

## 📋 Phase 2: PostgreSQL固有機能 (Week 2 - 5日間)

### Day 6-7: JSONB操作

#### タスク 5.1: JsonbHandler実装
**ファイル**: `src/handlers/database/engines/postgres/json.rs`

**実装メソッド**:
- [ ] `insert_jsonb()` - JSONB挿入
- [ ] `query_jsonb_path()` - JSONパス式クエリ
- [ ] `update_jsonb_field()` - フィールド更新
- [ ] `aggregate_jsonb()` - JSONB集約

**サンプルクエリ**:
```sql
-- JSONパス式
SELECT data->'user'->>'name' FROM users WHERE data @> '{"age": 25}';

-- JSONB集約
SELECT jsonb_agg(data) FROM logs WHERE created_at > NOW() - INTERVAL '1 day';
```

**テスト**:
```bash
cargo test postgres_jsonb_insert
cargo test postgres_jsonb_query
cargo test postgres_jsonb_update
```

**成果物**: ✅ JSONB操作完全サポート

---

### Day 8: LISTEN/NOTIFY

#### タスク 6.1: PostgresNotifier実装
**ファイル**: `src/handlers/database/engines/postgres/notify.rs`

**実装機能**:
- [ ] `listen()` - チャンネルリスニング
- [ ] `notify()` - 通知送信
- [ ] 非同期ストリーム対応

**実装例**:
```rust
pub async fn listen(&self, channel: &str) -> Result<impl Stream<Item = Notification>> {
    let (client, connection) = tokio_postgres::connect(&url, NoTls).await?;
    client.execute(&format!("LISTEN {}", channel), &[]).await?;
    // Stream実装
}
```

**デモプログラム**: `examples/postgres_notify_demo.rs`

**成果物**: ✅ リアルタイム通知機能

---

### Day 9: CTEs & Window Functions

#### タスク 7.1: 高度なクエリサポート
**ファイル**: `src/handlers/database/engines/postgres/cte.rs`

**サポート機能**:
- [ ] WITH句 (CTEs)
- [ ] RECURSIVE CTEs
- [ ] Window Functions (ROW_NUMBER, RANK, etc.)
- [ ] LAG/LEAD

**クエリ例**:
```sql
-- Recursive CTE
WITH RECURSIVE subordinates AS (
    SELECT id, name, manager_id FROM employees WHERE manager_id IS NULL
    UNION ALL
    SELECT e.id, e.name, e.manager_id
    FROM employees e
    INNER JOIN subordinates s ON e.manager_id = s.id
)
SELECT * FROM subordinates;
```

**成果物**: ✅ 高度なクエリ機能

---

### Day 10: Full-Text Search

#### タスク 8.1: 全文検索実装
**ファイル**: `src/handlers/database/engines/postgres/fulltext.rs`

**実装機能**:
- [ ] `ts_vector` インデックス作成
- [ ] `ts_query` 検索
- [ ] ランキング関数
- [ ] 日本語対応 (pg_bigm 拡張)

**クエリ例**:
```sql
SELECT *, ts_rank(search_vector, query) AS rank
FROM documents, to_tsquery('english', 'search & term') query
WHERE search_vector @@ query
ORDER BY rank DESC;
```

**成果物**: ✅ 全文検索機能

---

## 📋 Phase 3: パフォーマンス最適化 (Week 3 - 5日間)

### Day 11-12: クエリ分析・最適化

#### タスク 9.1: QueryAnalyzer実装
**ファイル**: `src/handlers/database/engines/postgres/performance.rs`

**実装機能**:
- [ ] `analyze_query()` - EXPLAIN ANALYZE
- [ ] `suggest_indexes()` - インデックス推奨
- [ ] `get_slow_queries()` - スロークエリ検出
- [ ] クエリプラン可視化

**pg_stat_statements有効化**:
```sql
CREATE EXTENSION IF NOT EXISTS pg_stat_statements;
ALTER SYSTEM SET shared_preload_libraries = 'pg_stat_statements';
```

**成果物**: ✅ クエリ分析ツール

---

### Day 13: バッチ処理最適化

#### タスク 10.1: BatchProcessor実装
**ファイル**: `src/handlers/database/engines/postgres/batch.rs`

**実装機能**:
- [ ] `bulk_insert()` - バルクINSERT
- [ ] `copy_from_csv()` - COPY コマンド
- [ ] パイプライン処理

**パフォーマンス目標**:
- 10,000行/秒以上の挿入速度
- メモリ使用量: 100MB以下

**成果物**: ✅ 高速バッチ処理

---

### Day 14: 統合テスト

#### タスク 11.1: テストスイート作成
**テストファイル**:
- `tests/postgres_integration_test.rs`
- `tests/postgres_jsonb_test.rs`
- `tests/postgres_notify_test.rs`
- `tests/postgres_performance_test.rs`
- `tests/postgres_batch_test.rs`

**テストカバレッジ目標**: 80%以上

**実行コマンド**:
```bash
cargo test --test postgres_integration_test
cargo test --test postgres_jsonb_test
cargo test --test postgres_notify_test
```

**成果物**: ✅ 50+テスト (100% passing)

---

### Day 15: ドキュメント・ベンチマーク

#### タスク 12.1: ドキュメント作成
**ドキュメントファイル**:
- `docs/postgres-optimization-guide.md`
- `docs/postgres-json-guide.md`
- `docs/postgres-performance-tuning.md`
- `docs/postgres-best-practices.md`

#### タスク 12.2: ベンチマーク実施
**ベンチマークファイル**:
- `benches/postgres_query_benchmark.rs`
- `benches/postgres_jsonb_benchmark.rs`
- `benches/postgres_batch_benchmark.rs`

**実行コマンド**:
```bash
cargo bench --bench postgres_query_benchmark
cargo bench --bench postgres_jsonb_benchmark
```

**成果物**: ✅ 完全なドキュメント＆ベンチマーク

---

## 📊 成功指標と検証方法

### パフォーマンスKPI

| 指標 | 目標 | 測定方法 |
|------|------|----------|
| 接続プール効率 | 90%以上 | `pool.num_idle() / pool.size()` |
| JSONB操作速度 | MySQL比2倍以上 | ベンチマーク比較 |
| スロークエリ削減 | 80%削減 | `pg_stat_statements`分析 |
| バッチ挿入速度 | 10,000行/秒 | `benches/postgres_batch_benchmark.rs` |
| クエリレスポンス | <100ms (95パーセンタイル) | Prometheusメトリクス |

### 検証手順

```bash
# 1. ユニットテスト
cargo test --lib

# 2. 統合テスト
cargo test --test postgres_integration_test
cargo test --test postgres_jsonb_test
cargo test --test postgres_notify_test

# 3. ベンチマーク
cargo bench --bench postgres_query_benchmark
cargo bench --bench postgres_jsonb_benchmark
cargo bench --bench postgres_batch_benchmark

# 4. フォーマット・Lint
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings

# 5. パフォーマンステスト
cargo run --example postgres_performance_demo
```

---

## 🔧 開発環境セットアップ

### PostgreSQL環境構築

```bash
# Docker Composeでセットアップ
docker-compose -f docker-compose.postgres.yml up -d

# 拡張機能インストール
docker exec -it mcp-postgres psql -U postgres -c "CREATE EXTENSION IF NOT EXISTS pg_stat_statements;"
docker exec -it mcp-postgres psql -U postgres -c "CREATE EXTENSION IF NOT EXISTS pg_trgm;"
docker exec -it mcp-postgres psql -U postgres -c "CREATE EXTENSION IF NOT EXISTS btree_gin;"
```

**docker-compose.postgres.yml**:
```yaml
version: '3.8'

services:
  postgres:
    image: postgres:16-alpine
    container_name: mcp-postgres
    environment:
      POSTGRES_PASSWORD: postgres
      POSTGRES_DB: mcp_rs_test
    ports:
      - "5432:5432"
    volumes:
      - postgres_data:/var/lib/postgresql/data
      - ./configs/database/init.sql:/docker-entrypoint-initdb.d/init.sql
    command:
      - "postgres"
      - "-c"
      - "shared_preload_libraries=pg_stat_statements"
      - "-c"
      - "pg_stat_statements.track=all"

volumes:
  postgres_data:
```

---

## 📝 実装チェックリスト

### Phase 1: 基盤構築 ✅

- [ ] ディレクトリ構造作成
- [ ] `PostgresEngine` 実装
- [ ] `OptimizedPoolConfig` 実装
- [ ] 接続プール最適化
- [ ] トランザクション管理
- [ ] `DatabaseEngine` トレイト実装
- [ ] Prepared Statement キャッシュ

### Phase 2: 高度な機能 ✅

- [ ] `JsonbHandler` 実装
- [ ] JSONB操作 (挿入・クエリ・更新・集約)
- [ ] `PostgresNotifier` 実装
- [ ] LISTEN/NOTIFY機能
- [ ] CTEs & Window Functions
- [ ] Full-Text Search

### Phase 3: 最適化・テスト ✅

- [ ] `QueryAnalyzer` 実装
- [ ] インデックス推奨機能
- [ ] スロークエリ検出
- [ ] `BatchProcessor` 実装
- [ ] COPY コマンド実装
- [ ] 統合テスト (50+テスト)
- [ ] ベンチマーク実装
- [ ] ドキュメント作成

---

## 🚨 リスク管理

### 潜在的リスク

| リスク | 影響度 | 対策 |
|--------|--------|------|
| PostgreSQLバージョン互換性 | 中 | 最小バージョン12以上を明記 |
| JSONB性能劣化 | 高 | GINインデックス必須化 |
| 接続プール枯渇 | 高 | 監視アラート設定 |
| スロークエリ増加 | 中 | pg_stat_statements常時監視 |
| メモリリーク | 中 | Valgrind/ASanによる検証 |

### 緊急対応プラン

1. **接続プール枯渇時**:
   - 自動スケールアップ (max_connections増加)
   - 古い接続の強制切断

2. **性能劣化時**:
   - クエリプラン再分析
   - VACUUM ANALYZE実行
   - インデックス再構築

---

## 📅 マイルストーン

```
Week 1 (Day 1-5)   ━━━━━━━━━━━━━━━━━━━━ 基盤構築完了
Week 2 (Day 6-10)  ━━━━━━━━━━━━━━━━━━━━ 高度な機能完了
Week 3 (Day 11-15) ━━━━━━━━━━━━━━━━━━━━ 最適化・テスト完了

Final Deliverables:
✅ PostgreSQL Engine実装完了
✅ 50+テスト (100% passing)
✅ 包括的ドキュメント
✅ ベンチマーク結果
```

---

## 🎯 次のステップ

1. **プロジェクト承認**: この実装計画をレビュー
2. **環境準備**: PostgreSQL Docker環境構築
3. **Phase 1開始**: ディレクトリ構造作成からスタート
4. **週次レビュー**: 各フェーズ終了時に進捗確認

**開始予定日**: 実装計画承認後即座  
**完了予定日**: 3週間後