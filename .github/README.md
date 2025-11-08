# .github フォルダ構成

このフォルダには、GitHubリポジトリの管理・運用に関するファイルが格納されています。

## 📁 フォルダ構成

```
.github/
├── README.md                    # このファイル
├── README.ja.md                 # 日本語キーワード・SEO用
├── FILE_MANAGEMENT.md           # ファイル管理ガイドライン
├── PROJECT_SETUP.md             # プロジェクト設定ガイド
├── PR_MANAGEMENT.md             # PR管理ルール（新規追加）
├── PULL_REQUEST_TEMPLATE.md     # PRテンプレート（新規追加）
├── ISSUE_TEMPLATE/              # Issueテンプレート集
│   ├── bug_report.md
│   ├── bug_report.yml
│   ├── dynamic_policy_update.yml
│   ├── feature_request.md
│   └── feature_request.yml
└── workflows/                   # GitHub Actions ワークフロー
    ├── ci.yml
    ├── docs.yml
    └── rust.yml
```

## 🔧 管理ファイルの役割

### PR・Issue管理
- **`PR_MANAGEMENT.md`**: PRワークフローと品質管理ルール
- **`PULL_REQUEST_TEMPLATE.md`**: GitHub PR作成時の自動テンプレート
- **`ISSUE_TEMPLATE/`**: Issue作成時のテンプレート集

### プロジェクト管理
- **`FILE_MANAGEMENT.md`**: ファイル構成とネーミング規則
- **`PROJECT_SETUP.md`**: 開発環境セットアップガイド

### CI/CD
- **`workflows/`**: GitHub Actions による自動化
  - `ci.yml`: 継続的インテグレーション
  - `docs.yml`: ドキュメント生成・デプロイ
  - `rust.yml`: Rust固有のテスト・ビルド

## 📋 利用ガイド

### 新しいPRを作成する場合
1. `PR_MANAGEMENT.md` のワークフロールールを確認
2. プロジェクトルートの `PR_DESCRIPTION.md` を更新
3. PR作成時に `PULL_REQUEST_TEMPLATE.md` が自動適用される

### 新しいIssueを作成する場合
1. `ISSUE_TEMPLATE/` から適切なテンプレートを選択
2. バグ報告: `bug_report.yml`
3. 機能リクエスト: `feature_request.yml`
4. ポリシー更新: `dynamic_policy_update.yml`

### 開発環境セットアップ
1. `PROJECT_SETUP.md` の手順に従う
2. `FILE_MANAGEMENT.md` でコーディング規則を確認

## 🔄 更新履歴

- **2025-01-08**: PR管理システム追加 (`PR_MANAGEMENT.md`, `PULL_REQUEST_TEMPLATE.md`)
- 既存: Issue テンプレート、GitHub Actions ワークフロー

---

この構成により、GitHubリポジトリの運用がより効率的で一貫性のあるものになります。