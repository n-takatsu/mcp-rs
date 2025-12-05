# Benchmarks

mcp-rsプロジェクトのパフォーマンスベンチマークスイート

## 📊 ベンチマークファイル

### データベースエンジン

#### PostgreSQL

- **`postgres_phase2_benchmarks.rs`** - PostgreSQL Phase 2 包括的パフォーマンス測定
  - 接続プール、クエリ実行、トランザクション、JSON操作、並行処理

#### MySQL Engine

- **`mysql_performance_benchmark.rs`** - MySQL基本パフォーマンス測定
- **`mysql_concurrent_connection_performance.rs`** - 並行接続性能測定
- **`mysql_parameterized_query_performance.rs`** - パラメータ化クエリ性能
- **`mysql_security_overhead.rs`** - セキュリティ機能オーバーヘッド測定
- **`mysql_resource_usage_analysis.rs`** - リソース使用量分析

#### データベース比較

- **`database_engine_performance_comparison.rs`** - 全データベースエンジン横並び比較
  - PostgreSQL、MySQL、SQLite、Redis、MongoDB

### コアコンポーネント

- **`mcp_protocol.rs`** - MCP Protocol処理性能測定
- **`policy_hot_reload_bench.rs`** - ポリシーホットリロード性能測定
- **`performance_test_execution_analysis.rs`** - テスト実行分析

## 🚀 クイックスタート

### すべてのベンチマークを実行

```bash
cargo bench
```

### 特定のベンチマークを実行

```bash
# PostgreSQL
cargo bench --bench postgres_phase2_benchmarks

# MySQL
cargo bench --bench mysql_performance_benchmark

# データベース比較
cargo bench --bench database_engine_performance_comparison
```

### 高速実行（開発中）

```bash
cargo bench --bench postgres_phase2_benchmarks -- --sample-size 10
```

## 📖 詳細ドキュメント

包括的なベンチマークガイドは以下を参照してください：

**[docs/guides/benchmarking-guide.md](../docs/guides/benchmarking-guide.md)**

内容：

- 各ベンチマークの詳細説明
- 実行方法とカスタマイズ
- 結果の解釈方法
- パフォーマンス目標値
- CI/CD統合方法
- トラブルシューティング

## 📈 結果の確認

ベンチマーク実行後、HTMLレポートが生成されます：

```bash
# Windows
start target/criterion/report/index.html

# Linux/Mac
open target/criterion/report/index.html
```

## 🎯 パフォーマンス目標

### PostgreSQL Target

| 項目 | 目標 |
|------|------|
| 接続取得 | < 100 µs |
| SELECT (1K行) | < 10 ms |
| INSERT | < 100 µs/行 |
| Transaction | < 100 µs |

### MySQL Target

| 項目 | 目標 |
|------|------|
| 基本クエリ | < 1 ms |
| 並行接続 | 1000+ 同時接続 |
| パラメータ化 | < 5% オーバーヘッド |

詳細な目標値は [benchmarking-guide.md](../docs/guides/benchmarking-guide.md) を参照してください。

## 🔧 環境設定

### MySQL 環境変数

```bash
export MYSQL_HOST=localhost
export MYSQL_PORT=3306
export MYSQL_USER=root
export MYSQL_PASSWORD=root
export MYSQL_DATABASE=test
```

### PostgreSQL 環境変数

```bash
export POSTGRES_HOST=localhost
export POSTGRES_PORT=5432
export POSTGRES_USER=postgres
export POSTGRES_PASSWORD=postgres
export POSTGRES_DATABASE=test
```

## 📝 ベンチマーク追加ガイドライン

新しいベンチマークを追加する場合：

1. Criterion.rsを使用
2. 適切なカテゴリに配置（データベース/コンポーネント）
3. READMEとドキュメントを更新
4. パフォーマンス目標値を定義

---

**詳細**: [docs/guides/benchmarking-guide.md](../docs/guides/benchmarking-guide.md)
