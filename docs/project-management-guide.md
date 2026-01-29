# mcp-rs プロジェクト管理ガイド

## 📋 目次

1. [概要](#概要)
2. [GitHub Issues, Projects, Milestonesの使い分け](#github-issues-projects-milestonesの使い分け)
3. [連携フロー手順](#連携フロー手順)
4. [Issueテンプレートの使い方](#issueテンプレートの使い方)
5. [ラベリングシステム](#ラベリングシステム)
6. [ワークフローとオートメーション](#ワークフローとオートメーション)
7. [ベストプラクティス](#ベストプラクティス)

---

## 概要

mcp-rsプロジェクトでは、効率的な開発とトラッキングのために、GitHub Issues、Projects、Milestonesを統合的に活用します。このガイドでは、それぞれのツールの目的と連携方法を説明します。

---

## GitHub Issues, Projects, Milestonesの使い分け

### 📝 GitHub Issues - 個別タスクの詳細管理

**目的**: 個々の機能開発、バグ修正、タスクの詳細な情報を管理

**使用例**:
- 具体的な機能実装 (#211: 動的ポリシー更新システム)
- バグレポート (#XXX: PostgreSQL接続エラー)
- 個別タスク (#XXX: APIドキュメント更新)

**含まれる情報**:
- 詳細な実装仕様
- コード例・技術仕様
- チェックリスト
- 成功指標・完了条件
- 技術的な議論・コメント

### 🎯 GitHub Projects - エピックレベルの管理

**目的**: 複数のIssueをグループ化し、大規模な機能開発の進捗を可視化

**使用例**:
- "v0.2.0-beta リリース準備"プロジェクト
- "セキュリティ強化"プロジェクト
- "パフォーマンス最適化"プロジェクト

**管理する情報**:
- エピックレベルの進捗（例: 動的ポリシー更新システム全体）
- 関連Issueの集約
- カンバンボード（Backlog, In Progress, Review, Done）
- 優先順位の可視化

### 🏁 GitHub Milestones - バージョン/リリース管理

**目的**: リリースバージョンごとにIssueを整理し、リリース進捗を追跡

**使用例**:
- v0.2.0-beta (2026-01-31)
- v0.3.0 (2026-04-30)
- v1.0.0 (2026-08-31)

**管理する情報**:
- リリース予定日
- バージョンに含まれるIssue一覧
- 完了率（8/15 issues completed）

---

## 連携フロー手順

### ステップ1: ROADMAPから優先度を特定

```bash
# ROADMAPを確認
cat ROADMAP.md

# 今週の優先度を特定
# - P0 (Critical): 動的ポリシー更新、Docker Runtime統合
# - P1 (High): WebSocket Transport、PostgreSQL最適化
```

### ステップ2: Epic Issueの作成

**大規模機能の場合、まずEpic Issueを作成**

```markdown
Title: [EPIC] 動的ポリシー更新システム
Labels: epic, priority-p0-critical, enhancement, security
Milestone: v0.2.0-beta
```

**Epic Issueには以下を含める**:
- 概要と目標
- 全体的な実装計画（Phase 1-3など）
- 成功指標
- 関連Sub-Issueへのリンク

### ステップ3: Sub-Issueの作成

**Epicを分割して、具体的なタスクを作成**

```markdown
Title: [TASK] AbuseIPDB API統合
Labels: task, priority-p0-critical, security
Milestone: v0.2.0-beta
Parent: #211
```

**テンプレートを使用**:
1. `.github/ISSUE_TEMPLATE/task.yml` を使用
2. 親Epicを参照
3. 推定工数を記載
4. チェックリストで進捗管理

### ステップ4: MilestoneとLabelsの設定

**Milestoneの割り当て**:
```bash
# Issue作成時にMilestoneを選択
Milestone: v0.2.0-beta (Due: 2026-01-31)
```

**Labelsの適用**:
```bash
# 優先度
priority-p0-critical, priority: high, priority: medium, priority: low

# 種類
enhancement, bug, task, documentation

# コンポーネント
security, database, websocket, infrastructure

# 状態
in-progress, blocked, ready-for-review
```

### ステップ5: GitHub Projectへの追加

**手順**:
1. GitHub Projects タブを開く
2. 該当するProjectを選択（例: "v0.2.0-beta Release"）
3. "Add item" でIssueを追加
4. カラムを設定（Backlog → In Progress → Review → Done）

**プロジェクトボード構成例**:
```
┌─────────────┬────────────────┬─────────────┬──────────┐
│  Backlog    │  In Progress   │   Review    │   Done   │
├─────────────┼────────────────┼─────────────┼──────────┤
│ Issue #211  │  Issue #212    │ Issue #99   │ Issue #87│
│ Issue #213  │  Task #215     │             │ Issue #86│
│ Task #216   │                │             │          │
└─────────────┴────────────────┴─────────────┴──────────┘
```

### ステップ6: Assigneesとdue dateの設定

**Assigneeの割り当て**:
```bash
# Issue詳細ページで "Assignees" セクションから選択
Assignee: @n-takatsu
```

**Due dateの設定**:
```bash
# Project Board上でDue dateを設定
Due date: 2026-01-29 (v0.2.0-beta deadline前)
```

### ステップ7: 進捗更新とクローズ

**進捗更新**:
```markdown
# Issue内でコメント
✅ Phase 1完了: AbuseIPDB API統合完了
🔄 Phase 2進行中: CVE Database統合中

進捗: 60% (3/5タスク完了)
```

**Issue完了時**:
1. チェックリストをすべて完了
2. 関連PRをマージ
3. Issueをクローズ（自動的にDoneカラムに移動）
4. 親Epic Issueの進捗を更新

---

## Issueテンプレートの使い方

### 1. Feature Request（機能追加）

**使用タイミング**: 新機能のアイデアを提案

```markdown
# テンプレート: .github/ISSUE_TEMPLATE/feature_request.yml

Title: [Feature] PostgreSQL JSONB型サポート
Labels: enhancement, database
Milestone: v0.3.0
```

### 2. Bug Report（バグ報告）

**使用タイミング**: バグを発見した時

```markdown
# テンプレート: .github/ISSUE_TEMPLATE/bug_report.yml

Title: [Bug] WebSocket接続時にパニック発生
Labels: bug, websocket
Milestone: v0.2.0-beta
```

### 3. Task（タスク）

**使用タイミング**: Epic配下の具体的なタスク作成

```markdown
# テンプレート: .github/ISSUE_TEMPLATE/task.yml

Title: [TASK] Redis接続プール実装
Labels: task, infrastructure
Parent: #212
Milestone: v0.2.0-beta
```

### 4. Dynamic Policy Update（動的ポリシー更新専用）

**使用タイミング**: セキュリティポリシー関連の変更

```markdown
# テンプレート: .github/ISSUE_TEMPLATE/dynamic_policy_update.yml

Title: [Policy] MITRE ATT&CK統合
Labels: security, policy
Milestone: v0.2.0-beta
```

---

## ラベリングシステム

### 優先度ラベル

| ラベル | 説明 | 使用例 |
|--------|------|--------|
| `priority-p0-critical` | 最優先・緊急対応 | セキュリティ脆弱性、システム障害 |
| `priority: high` | 高優先度 | 主要機能、リリースブロッカー |
| `priority: medium` | 中優先度 | 改善・最適化 |
| `priority: low` | 低優先度 | 将来的な機能、Nice-to-have |

### 種類ラベル

| ラベル | 説明 |
|--------|------|
| `enhancement` | 新機能追加 |
| `bug` | バグ修正 |
| `task` | タスク・サブタスク |
| `documentation` | ドキュメント更新 |
| `epic` | Epic Issue（複数Sub-Issue含む） |

### コンポーネントラベル

| ラベル | 説明 |
|--------|------|
| `security` | セキュリティ関連 |
| `database` | データベース機能 |
| `websocket` | WebSocket通信 |
| `infrastructure` | インフラ・Docker・Kubernetes |
| `component: security` | セキュリティコンポーネント |

### 状態ラベル

| ラベル | 説明 |
|--------|------|
| `in-progress` | 作業中 |
| `blocked` | ブロック中 |
| `ready-for-review` | レビュー待ち |
| `automated` | 自動生成Issue |
| `roadmap-health` | ROADMAP健全性チェック |

---

## ワークフローとオートメーション

### 1. ROADMAP Health Check（週次自動実行）

**ワークフロー**: `.github/workflows/roadmap-health.yml`

**実行内容**:
- 毎週日曜00:00 JSTに自動実行
- Milestone進捗分析
- 長期未解決Issueの検出
- 自動レポート生成（Issue #210など）

**生成されるIssue**:
```markdown
Title: 📊 Weekly ROADMAP Health Check - 2026-01-27
Labels: roadmap-health, automated, weekly-report
Assignee: @n-takatsu
```

### 2. Issue Labeler（自動ラベル付け）

**ワークフロー**: `.github/workflows/issue-labeler.yml`（今後実装予定）

**実行内容**:
- Issue作成時にタイトル/本文からラベルを自動付与
- 例: タイトルに "Security" → `security` ラベル追加

### 3. Project Board Automation

**GitHub Projects設定**:
- Issue作成時 → Backlogカラムに自動追加
- Assignee割り当て時 → In Progressに自動移動
- Issueクローズ時 → Doneに自動移動

---

## ベストプラクティス

### 1. Issue作成時

✅ **推奨**:
- テンプレートを使用する
- 適切なラベルを付与する
- Milestoneを設定する（リリース予定がある場合）
- 関連Issue（親Epic、依存Issue）をリンクする

❌ **避けるべき**:
- ラベルなしでIssueを作成
- 曖昧なタイトル（"バグ修正"だけなど）
- 実装詳細のない薄い内容

### 2. Epic管理

✅ **推奨**:
- Epicには全体像と Phase分けを記載
- Sub-Issueへのリンクを維持
- 進捗率を定期的に更新

```markdown
## 進捗状況
- Phase 1: ✅ 完了 (3/3タスク)
- Phase 2: 🔄 進行中 (2/5タスク)
- Phase 3: ⏳ 未開始 (0/4タスク)

**全体進捗**: 40% (5/12タスク完了)
```

### 3. Milestone管理

✅ **推奨**:
- リリース予定日の1週間前にスコープを確定
- 未完了Issueは次のMilestoneに移動
- Milestone完了時に振り返りIssueを作成

### 4. Project Board活用

✅ **推奨**:
- 毎日ボードを確認し、カラムを更新
- In Progressは3-5個まで（集中作業のため）
- Blockedカラムには理由をコメント

### 5. コミュニケーション

✅ **推奨**:
- 進捗更新は週1回以上
- 質問・議論はIssueコメントで公開
- 重要な決定事項はIssue説明文を更新

---

## 実践例: Issue #211の管理フロー

### 1. Epic Issue作成

```markdown
Title: 🚨 [P0][Critical] 動的ポリシー更新システムの実装
Labels: priority-p0-critical, enhancement, security, component: security
Milestone: v0.2.0-beta
Assignee: @n-takatsu
Project: v0.2.0-beta Release
```

### 2. Sub-Issueに分割

```markdown
# Sub-Issue 1
Title: [TASK] AbuseIPDB API統合
Parent: #211
Estimated: 2日

# Sub-Issue 2
Title: [TASK] CVE Database統合
Parent: #211
Estimated: 2日

# Sub-Issue 3
Title: [TASK] MITRE ATT&CK Framework統合
Parent: #211
Estimated: 3日
```

### 3. Project Boardで管理

```
Backlog: Sub-Issue #215 (AbuseIPDB)
In Progress: Sub-Issue #216 (CVE Database)
Review: -
Done: -
```

### 4. 進捗更新

```markdown
# Epic Issue #211にコメント
## Week 1 進捗報告 (2026-01-27)

✅ 完了:
- [x] AbuseIPDB API統合 (#215)
- [x] 設定ファイル実装

🔄 進行中:
- [ ] CVE Database統合 (#216) - 80%完了

⏳ 未開始:
- [ ] MITRE ATT&CK統合 (#217)

**進捗**: 40% (Week 2で完了予定)
```

### 5. 完了とクローズ

```markdown
# 全Sub-Issue完了後

Epic Issue #211を更新:
- すべてのチェックリスト完了
- テスト100%パス
- ドキュメント更新完了

→ Issue #211をクローズ
→ Project BoardでDoneカラムに移動
→ v0.2.0-betaのMilestone進捗が更新
```

---

## まとめ

### 連携フローの全体像

```
ROADMAP.md
    ↓
Epic Issue (#211, #212, etc.)
    ↓
Sub-Issues / Tasks (#215, #216, etc.)
    ↓
GitHub Project Board (Backlog → In Progress → Review → Done)
    ↓
Milestone進捗 (v0.2.0-beta: 8/15完了)
    ↓
Weekly Health Check (#210)
```

### 定期的なタスク

| 頻度 | タスク |
|------|--------|
| 毎日 | Project Boardの更新、In Progress Issueの作業 |
| 週次 | ROADMAP Health Check確認、進捗報告 |
| リリース前 | Milestoneスコープ確定、未完了Issue整理 |
| 月次 | ラベル・プロジェクト構造の見直し |

---

**最終更新**: 2026年1月27日  
**バージョン**: 1.0  
**メンテナ**: @n-takatsu
