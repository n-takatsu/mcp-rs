# .github フォルダ構成ガイド

このフォルダには、GitHubリポジトリの管理・運用に関するファイルが格納されています。

## 📁 フォルダ構成

```text
.github/
├── GITHUB_FOLDER_GUIDE.md          # このファイル
├── AUTOMATION_OPERATIONS_GUIDE.md  # Issue・ROADMAPの自動化の運用
├── EPIC_ISSUE_TEMPLATE.md          # Epic Issueの本文のひな形
├── ROADMAP_UPDATE_TEMPLATE.md      # ROADMAP.md更新のチェックリスト
├── FILE_MANAGEMENT.md              # ファイル配置のガイドライン
├── PR_MANAGEMENT.md                # PRの運用ルール
├── PULL_REQUEST_TEMPLATE.md        # PR作成時のテンプレート
├── README.ja.md                    # 日本語キーワード・SEO用
├── topics.txt                      # リポジトリのトピック
├── ISSUE_TEMPLATE/
│   ├── bug_report.yml              # バグ報告（bug）
│   ├── feature_request.yml         # 機能の提案（enhancement）
│   └── task.yml                    # 個別の作業（enhancement）
└── workflows/
    ├── ci.yml                          # フォーマット・clippy・テスト・MSRV・cargo audit
    ├── rust.yml                        # Rustのビルド・テスト
    ├── docs.yml                        # ドキュメントの生成・GitHub Pages（main）
    ├── container-integration-tests.yml # コンテナでの結合テスト
    ├── docker-security.yml             # Dockerイメージのビルド・スキャン・SBOM
    ├── docker-security-scan.yml        # Dockerイメージのスキャン（Trivy、毎日）
    ├── deploy.yml                      # リリースのDockerイメージの公開
    ├── scheduled-security-audit.yml    # 毎日のcargo audit
    ├── issue-labeler.yml               # Issueのコンポーネントのラベル
    ├── issue-labels.yml                # Issueの優先度の既定と、週次のラベルの確かめ
    └── roadmap-health.yml              # 週次のROADMAPレポート
```

## 📋 利用ガイド

### 新しいIssueを作成する場合

`ISSUE_TEMPLATE/`から選ぶ。ラベルの基準と作業の流れは[docs/project-management-guide.md](../docs/project-management-guide.md)を参照。

### 新しいPRを作成する場合

`develop`へ出す。`PULL_REQUEST_TEMPLATE.md`が自動で使われる。ルールは`PR_MANAGEMENT.md`を参照。

### 自動化を確認する場合

`AUTOMATION_OPERATIONS_GUIDE.md`を参照。

## 🔄 更新履歴

- **2026-11-13**: GitHub Projects・マイルストーンの初期設定の手順書と、重複していたIssueテンプレート（`.md`）を削除（#519）
- **2025-01-08**: PR管理システム追加（`PR_MANAGEMENT.md`・`PULL_REQUEST_TEMPLATE.md`）
