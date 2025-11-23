# PostgreSQL Phase 2 Benchmarking Suite

PostgreSQL Phase 2実装の詳細なパフォーマンス測定スイート

## ベンチマークファイル一覧

### PostgreSQL Phase 2 ベンチマーク

**ファイル**: `benches/postgres_phase2_benchmarks.rs`

Criterion.rsを使用した包括的なパフォーマンス測定

#### 測定項目 (15カテゴリ)

1. **Connection Pool** (接続プール)
   - プール作成オーバーヘッド
   - 接続取得パフォーマンス

2. **Select Queries** (SELECT クエリ)
   - 10行: 〜100µs
   - 100行: 〜1ms
   - 1000行: 〜10ms

3. **Insert Queries** (INSERT クエリ)
   - バッチサイズ 1, 10, 100

4. **Update Queries** (UPDATE クエリ)
   - シンプルUPDATE
   - JOINを使用したUPDATE

5. **Delete Queries** (DELETE クエリ)
   - IDによるDELETE
   - 条件付きDELETE

6. **Parameter Binding** (パラメータバインディング)
   - 1〜10パラメータ
   - SQL injection防止コスト

7. **Transactions** (トランザクション)
   - BEGIN/COMMIT
   - BEGIN/ROLLBACK
   - ネストされたセーブポイント

8. **Index Effectiveness** (インデックス効果)
   - インデックス付きクエリ
   - フルテーブルスキャン
   - スピードアップ測定

9. **JSON Operations** (JSON操作)
   - JSON挿入
   - フィールド抽出
   - 集約処理

10. **Concurrent Operations** (並行処理)
    - 1〜8スレッド
    - スケーリング効率測定

11. **Memory Usage** (メモリ使用量)
    - 接続プールメモリフットプリント
    - スケーリング分析

12. **Batch Operations** (バッチ処理)
    - 10〜1000行バッチ
    - 行単位コスト

## 実行方法

### 1. クイック実行

```bash
# デフォルト設定で実行
cargo bench --bench postgres_phase2_benchmarks
```

### 2. カスタマイズ実行

```bash
# 特定のベンチマークのみ実行
cargo bench --bench postgres_phase2_benchmarks -- connection_pool

# サンプルサイズ変更（高速実行）
cargo bench --bench postgres_phase2_benchmarks -- --sample-size 10

# 特定のメトリックに絞る
cargo bench --bench postgres_phase2_benchmarks -- select_queries
```

### 3. 詳細実行

```bash
# すべてのベンチマーク詳細出力
cargo bench --bench postgres_phase2_benchmarks -- --verbose

# 拡張レポート生成
cargo bench --bench postgres_phase2_benchmarks -- --verbose --save-baseline main
```

## 結果の解釈

### Criterion出力形式

```
benchmark_name             time:   [X.XX ms X.XX ms X.XX ms]
                           change: [-5.00% +1.00% +7.00%] (within noise)
                           slope  : [X.XX ms X.XX ms X.XX ms]
```

### 統計情報

- **time**: 実行時間の推定値と95%信頼区間
- **change**: 前回実行との比較
- **slope**: 線形回帰係数

### パフォーマンス判定

- ✅ **改善**: change が負値
- ⚠️ **安定**: change が ±5% 以内
- ❌ **低下**: change が +5% 以上

## ベンチマーク結果の保存

### ベースライン保存

```bash
# 基準となる結果を保存
cargo bench --bench postgres_phase2_benchmarks -- --save-baseline v1.0

# 結果は以下に保存されます:
# target/criterion/v1.0/
```

### 結果の比較

```bash
# 前回結果との比較
cargo bench --bench postgres_phase2_benchmarks -- --baseline v1.0

# 複数バージョン比較
cargo bench --bench postgres_phase2_benchmarks -- --verbose
```

## HTMLレポート

### レポート生成

```bash
# デフォルトで HTML レポートが生成されます
cargo bench --bench postgres_phase2_benchmarks

# レポートの場所
# target/criterion/report/index.html
```

### レポート内容

- グラフィカルな性能推移
- 統計分析結果
- 前回実行との比較
- パフォーマンスランキング

## パフォーマンス目標

### PostgreSQL Phase 2 目標値

| 項目 | 目標 | 単位 |
|------|------|------|
| 接続取得 | < 100 | µs |
| SELECT (1K行) | < 10 | ms |
| INSERT | < 100 | µs/行 |
| UPDATE | < 200 | µs/行 |
| DELETE | < 150 | µs/行 |
| Parameter Overhead | < 5 | % |
| Transaction | < 100 | µs |
| JSON Operation | < 500 | µs |
| Index Speedup | > 10 | 倍 |
| Thread Scaling | > 80 | % |

## トラブルシューティング

### Q. ベンチマークが遅い

```
A. 以下をお試しください:
   1. --sample-size 10 で高速化
   2. --measurement-time 5 で時間短縮
   3. バックグラウンドプロセス終了
```

### Q. メモリ不足

```
A. メモリ削減方法:
   1. concurrent_operations の並行数削減
   2. batch_operations のバッチサイズ削減
   3. sample_size 削減
```

### Q. 結果が不安定

```
A. 安定性向上:
   1. --sample-size 200 で増加
   2. --measurement-time 30 で延長
   3. 複数回実行して比較
```

## ベストプラクティス

### 1. 定期的な実行

```bash
# 毎日の開発時
cargo bench --bench postgres_phase2_benchmarks

# 本番前
cargo bench --bench postgres_phase2_benchmarks -- --verbose
```

### 2. 結果の記録

```bash
# タイムスタンプ付きで記録
cargo bench --bench postgres_phase2_benchmarks \
  2>&1 | tee "results/bench_$(date +%Y%m%d_%H%M%S).log"
```

### 3. リグレッション検出

```bash
# ベースラインとの比較
cargo bench --bench postgres_phase2_benchmarks -- --baseline main
```

## CI/CD統合

### GitHub Actions

```yaml
name: Performance Benchmarks
on: [push, pull_request]

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
          github-token: ${{ secrets.GITHUB_TOKEN }}
          auto-push: true
```

## パフォーマンスプロファイリング

### Perf で詳細分析

```bash
# flamegraph を生成（Linux）
cargo flamegraph --bench postgres_phase2_benchmarks
```

### Valgrind でメモリプロファイリング

```bash
# メモリリーク検出
valgrind --leak-check=full ./target/release/deps/postgres_phase2_benchmarks-*
```

## 参考資料

- [Criterion.rs Documentation](https://docs.rs/criterion/)
- [PostgreSQL Performance Tuning](https://www.postgresql.org/docs/current/performance-tips.html)
- [Rust Benchmarking Best Practices](https://nnethercote.github.io/perf-book/)
- `POSTGRES_BENCHMARKING_GUIDE.md` - 詳細なガイド

## 測定時間

| 実行スタイル | 推定時間 |
|-----------|---------|
| 標準実行 | 5-10分 |
| クイック実行 | 1-2分 |
| 詳細実行 | 15-20分 |

---

**最後の実行**: [実行日時は自動更新]
**メンテナ**: PostgreSQL Phase 2チーム
