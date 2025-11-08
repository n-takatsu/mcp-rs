# 📋 ファイル管理ガイドライン

## 🎯 目的
このドキュメントは、mcp-rsプロジェクトにおけるIssue Template、Roadmap、ドキュメントファイルの適切な管理方法を定義します。

## 📂 ファイル配置ルール

### 🗂️ 配置場所の決定基準

#### 📋 レポート・分析系ファイル
```
reports/
├── TEST_REPORT.md                    # テスト実行結果レポート
├── security-audit-report.md          # セキュリティ監査レポート
├── performance-test-results.md       # パフォーマンステスト結果
├── database-*-report.md              # データベース関連レポート
└── README.md                        # レポートディレクトリの説明
```

#### 🏗️ プロジェクト管理系ファイル
```
.github/
├── ISSUE_TEMPLATE/                  # Issue テンプレート
├── PR_MANAGEMENT.md                 # PR管理ルール
├── PULL_REQUEST_TEMPLATE.md         # PRテンプレート
├── FILE_MANAGEMENT.md              # このファイル
└── workflows/                       # GitHub Actions
```

#### 📖 ドキュメント系ファイル
```
project-docs/     # プロジェクト関連ドキュメント
docs/            # 技術仕様・設計書
website/         # GitHub Pages用サイト
examples/        # コードサンプル・デモ
```

#### 🧪 テスト系ファイル
```
tests/
├── unit/                           # ユニットテスト
├── integration/                    # 統合テスト
├── database/                       # データベーステスト
├── wordpress/                      # WordPress関連テスト
├── canary/                        # カナリーテスト
├── performance/                   # パフォーマンステスト
└── fixtures/                      # テストデータ・モック
```

### 📁 ファイル種別判定ガイド

#### 📊 レポート系 → `reports/`
- **条件**: 実行結果、分析結果、監査結果を含む
- **例**: TEST_REPORT.md, security-audit-report.md
- **特徴**: 日付・バージョン・結果データを含む

#### 🔧 管理系 → `.github/`
- **条件**: GitHub機能、PR/Issue管理に関連
- **例**: テンプレート、ワークフロー、管理ルール
- **特徴**: GitHub標準の機能と連携

#### 📚 ドキュメント系 → `project-docs/`, `docs/`, `website/`
- **条件**: 継続的に更新される説明・仕様書
- **分類基準**:
  - `project-docs/`: プロジェクト概要・ガイド
  - `docs/`: 技術仕様・API ドキュメント  
  - `website/`: 外部公開用サイト

#### 🧪 テスト系 → `tests/`
- **条件**: テストコード、テストデータ、モック
- **分類**: 機能別・種類別の階層構造
- **特徴**: 実行可能なテストファイル

## 📂 ファイルライフサイクル

### 🔄 Issue Templates (.github/ISSUE_TEMPLATE/)
**状態**: 永続保持
```
作成 → 使用 → 改善 → [継続保持]
```

**管理方針**:
- ❌ **削除しない**: 将来の類似Issue作成に再利用
- ✅ **改善する**: 使用状況に基づいて内容を改善
- ✅ **統合する**: 類似テンプレートは統合を検討

### 🗺️ Roadmap (ROADMAP.md)
**状態**: 履歴保持型更新
```
作成 → 進捗更新 → 完了マーク → [履歴として保持]
```

**管理方針**:
- ❌ **削除しない**: プロジェクト成長の記録として価値
- ✅ **履歴化**: 完了項目は「完了済み」セクションに移動
- ✅ **定期更新**: 月次での進捗反映

### 📚 ドキュメント (project-docs/, website/)
**状態**: 継続保守
```
作成 → 使用 → 更新 → [バージョン管理]
```

**管理方針**:
- ❌ **削除しない**: 情報の継続性を保つ
- ✅ **バージョン管理**: 機能追加に応じて更新
- ✅ **品質維持**: 定期的な内容見直し

## � ファイル移動・整理の実践ルール

### 📋 整理作業で確立されたベストプラクティス

#### ✅ 成功パターン
1. **段階的整理**: 一度にすべて移動せず、カテゴリ別に段階実行
   - Phase 1: ディレクトリ構造作成
   - Phase 2: ファイル移動と重複削除
   - Phase 3: 参照・リンク修正

2. **モジュール参照の更新**: テストファイル移動時は必須
   - `mod.rs`からの参照をコメント化
   - 古いテストファイルの削除
   - `cargo fmt`で検証

3. **Jekyll サイト修正**: ウェブサイトファイル変更時
   - `_config.yml`の設定確認
   - パーマリンクの修正
   - ナビゲーションリンクの更新

#### ⚠️ 注意すべきポイント
- **cargo fmt エラー**: 移動後は必ず`mod.rs`の参照を確認
- **リンク切れ**: Jekyll サイトのリンクを忘れずに更新
- **テスト実行**: 移動後は`cargo test`で動作確認

### 📁 ファイル種別毎の移動手順

#### 📊 レポートファイル移動
```bash
# 例: TEST_REPORT.md → reports/
Move-Item TEST_REPORT.md reports/TEST_REPORT.md
```

#### 🧪 テストファイル移動
```bash
# 1. 新しいテスト構造作成
mkdir tests/unit tests/integration tests/database

# 2. ファイル移動
Move-Item src/handlers/database/basic_tests.rs tests/database/engine_tests.rs

# 3. mod.rs の参照削除・コメント化
# 4. cargo fmt で検証
```

#### 🌐 ウェブサイトリンク修正
```yaml
# _config.yml の更新
header_pages:
  - api-reference.md
  - user-guide.md
  
# パーマリンク修正
permalink: /api-reference/
```

## �🔄 具体的な運用フロー

### Phase 1: 機能開発開始
1. **Issue作成**: テンプレートを使用してEpic Issue作成
2. **Roadmap更新**: 新機能を「🚧 開発中機能」に追加
3. **進捗追跡**: GitHub Projectでの管理開始

### Phase 2: 開発進行
1. **進捗更新**: Issue内のチェックリスト更新
2. **ブロッカー管理**: 依存関係の明確化
3. **週次レビュー**: Roadmapの進捗率更新

### Phase 3: 機能完了
1. **Issue完了**: すべてのチェックリスト完了
2. **Issue クローズ**: 受け入れ基準達成確認後
3. **Roadmap履歴化**: 完了項目を履歴セクションに移動

### Phase 4: 長期保守
1. **テンプレート改善**: 使用経験に基づく改善
2. **ドキュメント更新**: 新機能の反映
3. **年次レビュー**: 全体的な整理と改善

## 📊 品質管理

### ✅ 推奨事項
```markdown
✅ Issue Templates: 構造化された継続利用
✅ Roadmap: 履歴価値を保持する進捗管理
✅ Documentation: バージョン対応の継続更新
✅ GitHub Projects: 自動化による効率的管理
✅ 段階的整理: カテゴリ別の段階的ファイル移動
✅ 参照更新: mod.rs・リンクの確実な修正
✅ 動作確認: cargo fmt・cargo test での検証
```

### ❌ 避けるべき事項
```markdown
❌ 完了機能のファイル削除
❌ 履歴情報の破棄  
❌ テンプレートの頻繁な削除・再作成
❌ ドキュメントの非継続的更新
❌ 一括移動: 段階的整理を行わない大規模変更
❌ 参照未更新: mod.rs やリンクの修正漏れ
❌ 検証不足: 移動後の動作確認を怠る
```

## 🎯 整理完了状況（2025年11月8日現在）

### ✅ 完了済み整理作業
1. **PR管理システム**: `.github/PR_MANAGEMENT.md`, `.github/PULL_REQUEST_TEMPLATE.md`
2. **テストスイート再構築**: `tests/` ディレクトリ構造化（Phase 1-2完了）
   - ユニット・統合・データベース・WordPress・カナリー・パフォーマンステスト分離
   - 重複テスト削除と統合
   - 古いテストファイル削除とmod.rs参照修正
3. **ウェブサイト修正**: Jekyll設定とナビゲーションリンク修正
4. **レポート整理**: `TEST_REPORT.md` → `reports/TEST_REPORT.md` 移動
5. **ビルドエラー修正**: cargo fmt エラー解決

### 📊 整理効果
- **テスト実行**: 121テスト → 120テスト合格（1テスト無視）
- **コード品質**: Clippy警告 0件
- **ビルド**: cargo fmt エラー解決
- **構造化**: 機能別ディレクトリ分離完了

## 🎯 期待される効果

### 📈 短期効果
- 開発効率50%向上
- Issue管理の標準化
- 進捗透明性の確保

### 📊 長期効果
- プロジェクト成熟度の証明
- コントリビューター参加促進
- 企業採用時の信頼性向上

## 🔗 関連リソース

- [GitHub Project Setup](.github/PROJECT_SETUP.md)
- [Contributing Guidelines](CONTRIBUTING.md)
- [Project Roadmap](ROADMAP.md)

## 📚 参考: 実際の整理作業履歴

### 🔄 テストスイート再構築（2025年11月8日）
```bash
# Phase 1: ディレクトリ構造作成
mkdir tests/unit tests/integration tests/database tests/wordpress tests/canary tests/performance tests/fixtures

# Phase 2: ファイル移動例
Move-Item src/handlers/database/basic_tests.rs tests/database/engine_tests.rs
Move-Item src/handlers/database/simple_test.rs tests/database/ # → 統合後削除
Move-Item src/session/session_tests.rs tests/integration/session_management.rs

# Phase 3: 参照修正
# src/handlers/database/mod.rs の test モジュール参照をコメント化
# cargo fmt エラー修正確認
```

### 📋 レポートファイル移動（2025年11月8日）
```bash
# 適切なディレクトリへの配置
Move-Item TEST_REPORT.md reports/TEST_REPORT.md
```

### 🌐 Jekyll サイト修正例
```yaml
# website/_config.yml 更新
baseurl: "/mcp-rs"
header_pages:
  - api-reference.md
  - user-guide.md

# パーマリンク修正
---
permalink: /api-reference/
---
```

---

**最終更新**: 2025年11月8日  
**整理作業実績**: PR管理・テスト再構築・サイト修正・レポート整理完了  
**次回レビュー予定**: 2025年12月8日