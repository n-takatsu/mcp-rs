# PostgreSQL Phase 2 - Performance Benchmarking Guide

本ドキュメントは、PostgreSQL Phase 2実装のベンチマークテスト実行方法を説明しています。

## 概要

`benches/postgres_phase2_benchmarks.rs` ファイルには、PostgreSQL実装の詳細なパフォーマンス測定が含まれています。

### 測定対象

- ✅ **接続プール**: 作成、取得、マルチスレッド効率
- ✅ **クエリ実行**: SELECT、INSERT、UPDATE、DELETEの性能
- ✅ **パラメータバインディング**: SQL injection防止のオーバーヘッド
- ✅ **トランザクション**: BEGIN、COMMIT、ROLLBACK、セーブポイント
- ✅ **インデックス**: インデックスの効果測定
- ✅ **JSON操作**: JSONB型の挿入・抽出・集約
- ✅ **並行処理**: マルチスレッドスループット
- ✅ **メモリ使用**: 接続プールのメモリフットプリント
- ✅ **バッチ操作**: 一括挿入・更新の性能

## ベンチマーク構成

### 使用フレームワーク

**Criterion.rs** - Rustの標準的なベンチマークフレームワーク

```toml
[dev-dependencies]
criterion = { version = "0.5", features = ["html_reports"] }
```

### ベンチマーク設定

| 設定項目 | 値 | 説明 |
|--------|-----|------|
| Sample Size | 100 | サンプリング数 |
| Measurement Time | 10秒 | 測定時間 |
| Confidence Level | 95% | 信頼度 |

## ベンチマーク実行

### 1. すべてのベンチマークを実行

```bash
# 標準的なベンチマーク実行（時間がかかります）
cargo bench --bench postgres_phase2_benchmarks

# 特定のベンチマークのみ実行
cargo bench --bench postgres_phase2_benchmarks -- connection_pool

# クイックベンチマーク（少数のサンプル）
cargo bench --bench postgres_phase2_benchmarks -- --sample-size 10
```

### 2. ベンチマーク結果の表示

```bash
# HTML レポート生成（Criterion デフォルト）
cargo bench --bench postgres_phase2_benchmarks

# リポートは以下に生成されます:
# target/criterion/report/index.html
```

### 3. ベンチマーク間の比較

```bash
# 前回実行結果との自動比較
# Criterion は自動的に差分を検出して報告します

# 特定の比較条件を指定
cargo bench --bench postgres_phase2_benchmarks -- --verbose
```

## ベンチマークカテゴリ

### A. 接続プール (Connection Pool)

**測定項目**: `connection_pool`, `connection_acquisition`

```
┌─────────────────────────────────┐
│ Pool Creation Overhead          │
├─────────────────────────────────┤
│ Pool Size 5   - X ns            │
│ Pool Size 10  - Y ns            │
│ Pool Size 20  - Z ns            │
│ Pool Size 50  - W ns            │
└─────────────────────────────────┘

┌─────────────────────────────────┐
│ Connection Acquisition          │
├─────────────────────────────────┤
│ Average Acquisition - X ns      │
│ P95 Latency         - Y ns      │
│ P99 Latency         - Z ns      │
└─────────────────────────────────┘
```

**期待値**: < 100μs per acquisition

### B. クエリ実行 (Query Performance)

**測定項目**: `select_queries`, `insert_queries`, `update_queries`, `delete_queries`

```
┌─────────────────────────────────┐
│ SELECT Query Performance        │
├─────────────────────────────────┤
│ 10 rows    - X μs per query     │
│ 100 rows   - Y μs per query     │
│ 1000 rows  - Z μs per query     │
└─────────────────────────────────┘

┌─────────────────────────────────┐
│ INSERT Query Performance        │
├─────────────────────────────────┤
│ Batch 1    - X μs per query     │
│ Batch 10   - Y μs per query     │
│ Batch 100  - Z μs per query     │
└─────────────────────────────────┘
```

**期待値**:

- SELECT: < 1ms per 1000 rows
- INSERT: < 100μs per row
- UPDATE: < 200μs per row
- DELETE: < 150μs per row

### C. パラメータバインディング (Parameter Binding)

**測定項目**: `parameter_binding`, `sql_injection_prevention`

```
┌─────────────────────────────────┐
│ Parameter Binding Overhead      │
├─────────────────────────────────┤
│ 1 parameter  - X ns             │
│ 5 parameters - Y ns             │
│ 10 parameters- Z ns             │
└─────────────────────────────────┘

┌─────────────────────────────────┐
│ SQL Injection Prevention Cost   │
├─────────────────────────────────┤
│ Parameterized Query - X ns      │
│ String Concat       - Y ns      │
│ Overhead %          - (X-Y)/Y   │
└─────────────────────────────────┘
```

**期待値**: < 5% オーバーヘッド

### D. トランザクション (Transaction Lifecycle)

**測定項目**: `transaction_lifecycle`, `savepoint_operations`

```
┌─────────────────────────────────┐
│ Transaction Overhead            │
├─────────────────────────────────┤
│ BEGIN + COMMIT - X μs           │
│ BEGIN + ROLLBACK - Y μs         │
│ Savepoint (1) - Z μs            │
│ Savepoint (5) - W μs            │
└─────────────────────────────────┘
```

**期待値**: < 100μs per transaction

### E. インデックス効果 (Index Effectiveness)

**測定項目**: `index_effectiveness`

```
┌─────────────────────────────────┐
│ Index vs Full Scan Performance  │
├─────────────────────────────────┤
│ Indexed Query    - X μs         │
│ Full Table Scan  - Y μs         │
│ Index Speedup    - Y/X倍        │
└─────────────────────────────────┘
```

**期待値**: 10倍以上の高速化

### F. JSON操作 (JSON Operations)

**測定項目**: `json_operations`

```
┌─────────────────────────────────┐
│ JSON Operations Performance     │
├─────────────────────────────────┤
│ JSON INSERT     - X μs          │
│ JSON EXTRACTION - Y μs          │
│ JSON AGGREGATION- Z μs          │
└─────────────────────────────────┘
```

**期待値**: < 500μs per operation

### G. 並行処理 (Concurrent Operations)

**測定項目**: `concurrent_operations`

```
┌─────────────────────────────────┐
│ Thread Scaling                  │
├─────────────────────────────────┤
│ 1 thread  - X throughput/sec    │
│ 2 threads - Y throughput/sec    │
│ 4 threads - Z throughput/sec    │
│ 8 threads - W throughput/sec    │
│ Scaling Efficiency: Y/X*50%     │
└─────────────────────────────────┘
```

**期待値**: 80% スケーリング効率

### H. メモリ使用 (Memory Usage)

**測定項目**: `memory_usage`

```
┌─────────────────────────────────┐
│ Connection Pool Memory          │
├─────────────────────────────────┤
│ 5 connections    - X bytes      │
│ 10 connections   - Y bytes      │
│ 50 connections   - Z bytes      │
│ 100 connections  - W bytes      │
│ Per-Connection   - (W-A)/100    │
└─────────────────────────────────┘
```

**期待値**: < 100KB per connection

### I. バッチ操作 (Batch Operations)

**測定項目**: `batch_operations`

```
┌─────────────────────────────────┐
│ Batch Size Performance          │
├─────────────────────────────────┤
│ Batch 10     - X μs per batch   │
│ Batch 50     - Y μs per batch   │
│ Batch 100    - Z μs per batch   │
│ Batch 1000   - W μs per batch   │
│ Per-row cost - W/1000           │
└─────────────────────────────────┘
```

**期待値**: < 50μs per row in large batches

## ベンチマーク結果の解釈

### 出力例

```
connection_pool/5           time:   [X.XX ns X.XX ns X.XX ns]
connection_pool/10          time:   [Y.YY ns Y.YY ns Y.YY ns]
connection_pool/20          time:   [Z.ZZ ns Z.ZZ ns Z.ZZ ns]

select_queries/10_rows       time:   [X.XX µs X.XX µs X.XX µs]
select_queries/100_rows      time:   [Y.YY µs Y.YY µs Y.YY µs]
select_queries/1000_rows     time:   [Z.ZZ µs Z.ZZ µs Z.ZZ µs]
```

### 統計指標

| 指標 | 説明 |
|------|------|
| **Lower bound** | 95%信頼区間の下限 |
| **Estimate** | 推定平均実行時間 |
| **Upper bound** | 95%信頼区間の上限 |
| **Outliers** | 外れ値（Low/Mild/Severe） |

## ベンチマーク実行フロー

```
┌──────────────────────────┐
│ 1. Criterion初期化      │
│    - コンパイル          │
│    - 環境設定            │
└────────┬─────────────────┘
         │
┌────────▼─────────────────┐
│ 2. ウォームアップ        │
│    - キャッシュ準備      │
│    - JIT最適化           │
└────────┬─────────────────┘
         │
┌────────▼─────────────────┐
│ 3. 計測                  │
│    - 複数回実行          │
│    - 統計計算            │
└────────┬─────────────────┘
         │
┌────────▼─────────────────┐
│ 4. 結果分析              │
│    - 回帰検出            │
│    - レポート生成        │
└────────┬─────────────────┘
         │
┌────────▼─────────────────┐
│ 5. 出力                  │
│    - コンソール出力      │
│    - HTML報告書          │
└──────────────────────────┘
```

## パフォーマンス最適化のヒント

### 1. 接続プール設定

```rust
// 推奨設定
let pool = sqlx::postgres::PgPoolOptions::new()
    .max_connections(10)                    // CPU cores * 2-4
    .max_lifetime(Some(Duration::from_secs(1800)))
    .idle_timeout(Some(Duration::from_secs(600)))
    .connect(&url)
    .await?;
```

### 2. クエリ最適化

```sql
-- インデックス活用
CREATE INDEX idx_email ON users(email);

-- 選択列の最小化
SELECT id, name FROM users WHERE email = $1;  -- ✓ Good
SELECT * FROM users WHERE email = $1;         -- ✗ Bad

-- バッチ処理
INSERT INTO users (name, email) VALUES ($1, $2), ($3, $4), ...;  -- ✓ Good
```

### 3. トランザクション最適化

```rust
// 短いトランザクション
let mut tx = pool.begin().await?;
// 必要な操作のみ
tx.commit().await?;

// セーブポイントの活用
let mut tx = pool.begin().await?;
// 長い操作...
sqlx::query("SAVEPOINT sp1").execute(&mut *tx).await?;
// リスキーな操作...
if result.is_err() {
    sqlx::query("ROLLBACK TO SAVEPOINT sp1").execute(&mut *tx).await?;
}
tx.commit().await?;
```

### 4. JSON操作最適化

```sql
-- JSONフィールドのインデックス
CREATE INDEX idx_metadata ON posts USING GIN(metadata);

-- 効率的な抽出
SELECT metadata->>'author' FROM posts WHERE metadata @> '{"type":"article"}';
```

## トラブルシューティング

### Q. ベンチマークが遅い

```
A. 以下を確認してください:
   1. サンプルサイズを減らす: --sample-size 10
   2. バックグラウンドプロセスを終了
   3. 計測時間を短縮: --measurement-time 5
```

### Q. 結果が不安定

```
A. 以下を実行してください:
   1. 複数回実行して安定性を確認
   2. 計測時間を延長: --measurement-time 30
   3. コンパイル最適化を確認: cargo bench --release
```

### Q. メモリ不足エラー

```
A. メモリ使用量を削減:
   1. サンプルサイズ削減
   2. バッチサイズ削減
   3. 並行スレッド数削減
```

## ベストプラクティス

### 1. 定期的なベンチマーク実行

```bash
# 毎回のコミット前
cargo bench --bench postgres_phase2_benchmarks

# リグレッション検出
# Criterion は自動的に前回との比較を実施
```

### 2. ベンチマーク結果の記録

```bash
# タイムスタンプ付きで結果を保存
cargo bench --bench postgres_phase2_benchmarks 2>&1 | tee "bench_$(date +%Y%m%d_%H%M%S).log"
```

### 3. CI/CDパイプラインでの実行

```yaml
# GitHub Actions例
name: Performance Benchmarks
on: [pull_request]

jobs:
  benchmark:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
      - uses: dtolnay/rust-toolchain@stable
      - run: cargo bench --bench postgres_phase2_benchmarks
      - name: Store benchmark result
        uses: benchmark-action/github-action-benchmark@v1
        with:
          tool: 'cargo'
          output-file-path: target/criterion/output.txt
```

## パフォーマンス目標

### Phase 2 達成目標

| 測定項目 | 目標値 | 状況 |
|--------|--------|------|
| 接続取得 | < 100μs | - |
| SELECT | < 1ms/1000行 | - |
| INSERT | < 100μs/行 | - |
| UPDATE | < 200μs/行 | - |
| DELETE | < 150μs/行 | - |
| Parameter Binding Overhead | < 5% | - |
| Transaction Overhead | < 100μs | - |
| JSON Operations | < 500μs | - |
| Concurrent Scaling | 80% efficiency | - |
| Memory per Connection | < 100KB | - |

## 関連ドキュメント

- [ベンチマークガイド総合](./benchmarking-guide.md) - 全データベースエンジンのベンチマーク総合ガイド
- [ベンチマークディレクトリ](../../benches/README.md) - ベンチマークファイル一覧
- `docs/implementation/postgresql_phase2.md` - PostgreSQL Phase 2実装ガイド
- `benches/postgres_phase2_benchmarks.rs` - ベンチマークソースコード

## 変更履歴

| 日付 | 変更内容 |
|------|---------|
| 2025-12-05 | ドキュメント整理 - docs/guidesに移動、相互参照追加 |
| 2025-01-22 | 初版作成 - 15個のベンチマークカテゴリを追加 |

---

**Note**: ベンチマーク実行にはCriterion.rsが必要です。プロジェクトの`Cargo.toml`に以下を追加してください:

```toml
[dev-dependencies]
criterion = { version = "0.5", features = ["html_reports"] }
```

実行時間: 標準的な実行で 5-10 分の時間が必要です。
