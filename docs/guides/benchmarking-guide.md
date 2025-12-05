# mcp-rs ベンチマークガイド

データベースエンジンとコアコンポーネントの包括的なパフォーマンス測定ガイド

## 📋 目次

- [概要](#概要)
- [ベンチマークファイル一覧](#ベンチマークファイル一覧)
- [実行方法](#実行方法)
- [データベース別ベンチマーク](#データベース別ベンチマーク)
- [結果の解釈](#結果の解釈)
- [CI/CD統合](#cicd統合)

## 概要

mcp-rsプロジェクトには、データベースエンジン（PostgreSQL、MySQL、SQLite、Redis、MongoDB）とコアコンポーネント（MCP Protocol、Policy Hot Reload）の詳細なパフォーマンステストスイートが用意されています。

### 使用フレームワーク

**Criterion.rs** - Rustの標準的なベンチマークフレームワーク

```toml
[dev-dependencies]
criterion = { version = "0.5", features = ["html_reports"] }
```

## ベンチマークファイル一覧

### データベースエンジン

| ファイル | データベース | 測定内容 |
|---------|------------|---------|
| `postgres_phase2_benchmarks.rs` | PostgreSQL | 包括的なPhase 2機能測定 |
| `mysql_performance_benchmark.rs` | MySQL | 基本的なクエリ性能 |
| `mysql_concurrent_connection_performance.rs` | MySQL | 並行接続性能 |
| `mysql_parameterized_query_performance.rs` | MySQL | パラメータ化クエリ |
| `mysql_security_overhead.rs` | MySQL | セキュリティ機能オーバーヘッド |
| `mysql_resource_usage_analysis.rs` | MySQL | リソース使用量分析 |
| `database_engine_performance_comparison.rs` | 全DB | データベース間比較 |

### コアコンポーネント

| ファイル | コンポーネント | 測定内容 |
|---------|--------------|---------|
| `mcp_protocol.rs` | MCP Protocol | プロトコル処理性能 |
| `policy_hot_reload_bench.rs` | Policy | ポリシーホットリロード |
| `performance_test_execution_analysis.rs` | テスト基盤 | テスト実行分析 |

## 実行方法

### 基本実行

```bash
# すべてのベンチマークを実行
cargo bench

# 特定のベンチマークファイルのみ実行
cargo bench --bench postgres_phase2_benchmarks
cargo bench --bench mysql_performance_benchmark

# 特定のベンチマーク項目のみ実行
cargo bench --bench postgres_phase2_benchmarks -- connection_pool
```

### カスタマイズ実行

```bash
# サンプルサイズを変更（高速実行）
cargo bench --bench postgres_phase2_benchmarks -- --sample-size 10

# 測定時間を変更
cargo bench --bench postgres_phase2_benchmarks -- --measurement-time 5

# 詳細出力
cargo bench --bench postgres_phase2_benchmarks -- --verbose

# ベースラインを保存
cargo bench --bench postgres_phase2_benchmarks -- --save-baseline v1.0

# ベースラインと比較
cargo bench --bench postgres_phase2_benchmarks -- --baseline v1.0
```

### HTMLレポート確認

```bash
# ベンチマーク実行後、以下のパスでHTMLレポートが生成されます
# target/criterion/report/index.html

# ブラウザで開く（Windows）
start target/criterion/report/index.html

# ブラウザで開く（Linux/Mac）
open target/criterion/report/index.html
```

## データベース別ベンチマーク

### PostgreSQL

**ファイル**: `benches/postgres_phase2_benchmarks.rs`

#### 測定項目（15カテゴリ）

1. **Connection Pool** - 接続プール管理
2. **Select Queries** - SELECT性能（10/100/1000行）
3. **Insert Queries** - INSERT性能（バッチ1/10/100）
4. **Update Queries** - UPDATE性能
5. **Delete Queries** - DELETE性能
6. **Parameter Binding** - パラメータバインディング
7. **Transactions** - トランザクション処理
8. **Index Effectiveness** - インデックス効果
9. **JSON Operations** - JSON操作
10. **Concurrent Operations** - 並行処理（1-8スレッド）
11. **Memory Usage** - メモリ使用量
12. **Batch Operations** - バッチ処理

#### 実行例

```bash
# PostgreSQL Phase 2 完全テスト
cargo bench --bench postgres_phase2_benchmarks

# 特定カテゴリのみ
cargo bench --bench postgres_phase2_benchmarks -- select_queries
cargo bench --bench postgres_phase2_benchmarks -- transactions
cargo bench --bench postgres_phase2_benchmarks -- concurrent_operations
```

#### パフォーマンス目標

| 項目 | 目標 | 単位 |
|------|------|------|
| 接続取得 | < 100 | µs |
| SELECT (1K行) | < 10 | ms |
| INSERT | < 100 | µs/行 |
| UPDATE | < 200 | µs/行 |
| DELETE | < 150 | µs/行 |
| Transaction | < 100 | µs |
| JSON Operation | < 500 | µs |
| Index Speedup | > 10 | 倍 |

### MySQL

**ファイル群**: `mysql_*.rs` (5ファイル)

#### 1. 基本パフォーマンス (`mysql_performance_benchmark.rs`)

```bash
cargo bench --bench mysql_performance_benchmark
```

測定内容：
- 基本的なCRUD操作
- クエリ実行時間
- トランザクション性能

#### 2. 並行接続性能 (`mysql_concurrent_connection_performance.rs`)

```bash
cargo bench --bench mysql_concurrent_connection_performance
```

測定内容：
- 同時接続数スケーリング
- マルチスレッド効率
- 接続プール最適化

#### 3. パラメータ化クエリ (`mysql_parameterized_query_performance.rs`)

```bash
cargo bench --bench mysql_parameterized_query_performance
```

測定内容：
- プリペアドステートメント性能
- パラメータバインディングコスト
- SQL injection防止オーバーヘッド

#### 4. セキュリティオーバーヘッド (`mysql_security_overhead.rs`)

```bash
cargo bench --bench mysql_security_overhead
```

測定内容：
- セキュリティ機能の性能影響
- 暗号化コスト
- 認証オーバーヘッド

#### 5. リソース使用量分析 (`mysql_resource_usage_analysis.rs`)

```bash
cargo bench --bench mysql_resource_usage_analysis
```

測定内容：
- メモリフットプリント
- CPU使用率
- ディスクI/O

### データベース比較

**ファイル**: `database_engine_performance_comparison.rs`

```bash
cargo bench --bench database_engine_performance_comparison
```

すべてのデータベースエンジン（PostgreSQL、MySQL、SQLite、Redis、MongoDB）の性能を横並びで比較します。

測定内容：
- 基本CRUD操作の速度比較
- 接続プール効率
- トランザクション性能
- メモリ使用量

## 結果の解釈

### Criterion出力形式

```
benchmark_name             time:   [X.XX ms X.XX ms X.XX ms]
                           change: [-5.00% +1.00% +7.00%] (within noise)
                           thrpt:  [X.XX Melem/s X.XX Melem/s X.XX Melem/s]
```

### 統計情報

- **time**: 実行時間の推定値と95%信頼区間
  - `[下限値 中央値 上限値]`
- **change**: 前回実行との比較
  - 負値: 性能改善 ✅
  - 正値: 性能低下 ⚠️
- **thrpt**: スループット（要素数/秒）

### パフォーマンス判定基準

| 変化率 | 判定 | 対応 |
|--------|------|------|
| < -10% | 🎉 大幅改善 | 変更を記録 |
| -10% ~ -5% | ✅ 改善 | そのまま継続 |
| -5% ~ +5% | ⚪ 安定 | 問題なし |
| +5% ~ +10% | ⚠️ 注意 | 原因調査 |
| > +10% | ❌ 低下 | 即座に対応 |

## ベストプラクティス

### 1. 定期的な実行

```bash
# 開発中は定期的に実行
cargo bench

# プルリクエスト前に必ず実行
cargo bench --bench postgres_phase2_benchmarks
cargo bench --bench mysql_performance_benchmark

# ベースラインとの比較
cargo bench -- --baseline main
```

### 2. 結果の記録

```bash
# タイムスタンプ付きログ保存
cargo bench 2>&1 | tee "benchmark_results/bench_$(date +%Y%m%d_%H%M%S).log"

# JSON形式で保存
cargo bench -- --save-baseline "release-v1.0"
```

### 3. リグレッション検出

```bash
# 前回との比較
git checkout main
cargo bench -- --save-baseline main

git checkout feature-branch
cargo bench -- --baseline main
```

## CI/CD統合

### GitHub Actions

```yaml
name: Performance Benchmarks
on: [push, pull_request]

jobs:
  benchmark:
    runs-on: ubuntu-latest
    services:
      postgres:
        image: postgres:15
        env:
          POSTGRES_PASSWORD: postgres
        options: >-
          --health-cmd pg_isready
          --health-interval 10s
          --health-timeout 5s
          --health-retries 5
      mysql:
        image: mysql:8.0
        env:
          MYSQL_ROOT_PASSWORD: root
        options: >-
          --health-cmd "mysqladmin ping"
          --health-interval 10s
          --health-timeout 5s
          --health-retries 5

    steps:
      - uses: actions/checkout@v3
      - uses: dtolnay/rust-toolchain@stable
      
      - name: Run PostgreSQL Benchmarks
        run: cargo bench --bench postgres_phase2_benchmarks
        env:
          POSTGRES_HOST: localhost
          POSTGRES_PORT: 5432
          POSTGRES_USER: postgres
          POSTGRES_PASSWORD: postgres
          
      - name: Run MySQL Benchmarks
        run: |
          cargo bench --bench mysql_performance_benchmark
          cargo bench --bench mysql_concurrent_connection_performance
        env:
          MYSQL_HOST: localhost
          MYSQL_PORT: 3306
          MYSQL_USER: root
          MYSQL_PASSWORD: root
          
      - name: Store benchmark result
        uses: benchmark-action/github-action-benchmark@v1
        with:
          tool: 'cargo'
          output-file-path: target/criterion/output.txt
          github-token: ${{ secrets.GITHUB_TOKEN }}
          auto-push: true
```

## トラブルシューティング

### Q. ベンチマークが遅い

```
A. 以下を試してください:
   1. --sample-size 10 で高速化
   2. --measurement-time 5 で時間短縮
   3. バックグラウンドプロセスを終了
   4. 特定のベンチマークのみ実行
```

### Q. メモリ不足エラー

```
A. メモリ削減方法:
   1. concurrent_operations の並行数を減らす
   2. batch_operations のバッチサイズを減らす
   3. --sample-size を小さくする
   4. 大規模テストを個別に実行
```

### Q. 結果が不安定

```
A. 安定性向上:
   1. --sample-size 200 で増加
   2. --measurement-time 30 で延長
   3. 複数回実行して平均を取る
   4. システム負荷を減らす
```

### Q. データベース接続エラー

```
A. 接続確認:
   1. データベースサービスが起動しているか確認
   2. 環境変数が正しく設定されているか確認
   3. ポートが開いているか確認
   4. 認証情報が正しいか確認
```

## パフォーマンスプロファイリング

### Flamegraph生成

```bash
# Linux/Mac
cargo install flamegraph
cargo flamegraph --bench postgres_phase2_benchmarks

# Windows (WSL推奨)
wsl cargo flamegraph --bench postgres_phase2_benchmarks
```

### Valgrind でメモリ分析

```bash
# メモリリーク検出
valgrind --leak-check=full \
  ./target/release/deps/postgres_phase2_benchmarks-*

# メモリプロファイリング
valgrind --tool=massif \
  ./target/release/deps/postgres_phase2_benchmarks-*
```

### Perf で詳細分析

```bash
# Linux
perf record cargo bench --bench postgres_phase2_benchmarks
perf report
```

## 参考資料

### ドキュメント

- [PostgreSQL ベンチマークガイド](../POSTGRES_BENCHMARKING_GUIDE.md)
- [MySQL Phase 1 ガイド](../mysql-phase1-guide.md)
- [データベース設計](../design/database-handler.md)

### 外部リンク

- [Criterion.rs Documentation](https://docs.rs/criterion/)
- [PostgreSQL Performance Tuning](https://www.postgresql.org/docs/current/performance-tips.html)
- [MySQL Performance Best Practices](https://dev.mysql.com/doc/refman/8.0/en/optimization.html)
- [Rust Performance Book](https://nnethercote.github.io/perf-book/)

## 測定時間の目安

| 実行スタイル | 推定時間 | 用途 |
|-----------|---------|------|
| クイック実行<br>`--sample-size 10` | 1-2分 | 開発中の迅速確認 |
| 標準実行<br>デフォルト | 5-10分 | 日常的なベンチマーク |
| 詳細実行<br>`--sample-size 200` | 15-30分 | リリース前の詳細測定 |
| 全ベンチマーク<br>`cargo bench` | 30-60分 | CI/CD、包括的測定 |

---

**最終更新**: 2025年12月5日
**メンテナ**: mcp-rs開発チーム
