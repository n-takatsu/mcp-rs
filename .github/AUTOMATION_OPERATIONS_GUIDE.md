# 🤖 Issue・ROADMAP 自動化の運用ガイド

> **対象**: mcp-rs プロジェクト開発チーム
> **最終更新**: 2026年11月13日（#519）

Issueのラベルと週次レポートを自動化しているワークフローと、その運用をまとめる。ラベルの基準と作業の流れは、[docs/project-management-guide.md](../docs/project-management-guide.md)を参照。

## 🔄 自動化ワークフロー

| ワークフロー | いつ | すること |
|---|---|---|
| `issue-labeler.yml` | Issueを立てたとき | タイトルからコンポーネントのラベル（`security`・`database`等）を付ける（#517） |
| `issue-labels.yml` | Issueを立てたとき | 優先度が無ければ`priority: medium`を付ける（#523） |
| `issue-labels.yml` | 毎週月曜 10:00 JST・手動 | 開いているIssueのラベルが決まり（種類・優先度が1つずつ、決まりにないラベルが無い）に沿っているかを`scripts/issue_labels.py`で確かめ、沿っていなければ失敗にする |
| `roadmap-health.yml` | 毎週月曜 09:00 JST・手動 | 開いているマイルストーンの進捗と、30日を超えて開いているIssueを報告するIssueを作り、前の週のレポートを閉じる（ROADMAP.mdは書き換えない） |
| `scheduled-security-audit.yml` | 毎日 12:00 JST | `cargo audit`で脆弱性が見つかったら、`cargo-audit-scheduled`・`security`の付いたIssueを作る |

自動化していないもの:

- 種類のラベル（`bug`・`enhancement`等）と、既定と違う優先度は、人が付ける
- GitHub Projectsへの追加、担当者の割り当て、マイルストーンの設定は、手で行う
- ROADMAP.mdは手で更新する（PR経由）

以前の`issue-automation.yml`（Issueのタイトルを`run:`に直接展開していた）は#517で、`roadmap-sync.yml`（ROADMAP.mdを書き換えて`develop`に直接コミットしていた）はそれ以前に削除した。

---

## 📅 運用

### Issueを立てたとき

- テンプレートが種類のラベルを付ける。違っていれば付け替える
- 優先度を判断できれば`priority: high`・`priority: low`に付け替える（付けなければ`medium`のまま）

### 週次

- `issue-labels.yml`が失敗していたら、ジョブのまとめに出ているIssueのラベルを直す
- 週次のROADMAPレポート（`roadmap-health`ラベルのIssue）を確認する。前の週のレポートは、新しいレポートを作るときに自動で閉じる

### リリース前

```bash
# マイルストーンのIssueの一覧
gh issue list --milestone "v1.0.0" --state all --json number,title,state
```

---

## ⚠️ トラブルシューティング

### 週次のラベルの確かめが失敗した

ジョブのまとめ（Actionsの実行の画面）に、決まりに沿っていないIssueと理由が出る。手元からも確かめられる:

```bash
python scripts/issue_labels.py
```

### 週次レポートが作られない

- ワークフローの実行履歴を確認する
- 手動で実行する: `gh workflow run roadmap-health.yml`

---

## 🔗 関連

- [docs/project-management-guide.md](../docs/project-management-guide.md) - ラベルの基準・作業の流れ
- [ROADMAP.md](../ROADMAP.md) - プロジェクト戦略
- [.github/workflows/](workflows/) - ワークフロー
