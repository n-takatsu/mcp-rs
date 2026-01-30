# GitHub Project Board 設定ガイド

## 📋 概要

mcp-rsプロジェクトのGitHub Project Boardを設定し、Issue #211-214を追加する手順書です。

---

## 🎯 Project Board作成手順

### ステップ1: 新規Projectの作成

1. **GitHubリポジトリのProjectsタブを開く**
   ```
   https://github.com/n-takatsu/mcp-rs/projects
   ```

2. **"New project"ボタンをクリック**

3. **テンプレート選択**
   - "Board" テンプレートを選択
   - または "Table" テンプレート（詳細管理向け）

4. **Project名を入力**
   ```
   Project名: v0.2.0-beta Release
   説明: v0.2.0-beta リリースに向けた機能開発とタスク管理
   ```

5. **"Create project"をクリック**

---

## 📊 Project Board構造設定

### ステップ2: カラム（列）の設定

デフォルトのカラムを以下のように調整：

| カラム名 | 説明 | 自動化設定 |
|----------|------|------------|
| **📋 Backlog** | 未着手のタスク | Issue作成時に自動追加 |
| **🔄 In Progress** | 作業中のタスク | Assignee割り当て時に移動 |
| **👀 Review** | レビュー待ち | PR作成時に自動追加 |
| **✅ Done** | 完了したタスク | Issueクローズ時に移動 |

**カラム追加手順：**
1. Project Board右上の "..."メニューをクリック
2. "Settings"を選択
3. "Fields"セクションで"Status"フィールドを編集
4. 上記4つのステータスを設定

### ステップ3: カスタムフィールドの追加

**優先度フィールド**
1. "Add field" → "Single select"
2. フィールド名: `Priority`
3. オプション:
   - 🔴 P0 (Critical)
   - 🟠 P1 (High)
   - 🟡 P2 (Medium)
   - 🟢 P3 (Low)

**工数見積もりフィールド**
1. "Add field" → "Text"
2. フィールド名: `Estimated Effort`
3. 例: "3日", "1週間"

**担当者フィールド**
- デフォルトの"Assignees"を使用

---

## 📝 Issueの追加手順

### ステップ4: 既存IssueをProjectに追加

**方法1: Project Board画面から追加**
1. Project Board下部の "+ Add item"をクリック
2. Issue番号で検索:
   - `#211` - 動的ポリシー更新システム
   - `#212` - Docker ランタイム統合
   - `#213` - WebSocket Transport基盤
   - `#214` - PostgreSQL最適化
3. Issueを選択してEnter

**方法2: Issue画面から追加**
1. Issue詳細ページ（例: #211）を開く
2. 右サイドバーの"Projects"セクションをクリック
3. "v0.2.0-beta Release" Projectを選択

### ステップ5: Issueの初期配置

各Issueを適切なカラムに配置：

| Issue | カラム | Priority | Assignee | Estimated Effort |
|-------|--------|----------|----------|------------------|
| #211 | Backlog | P0 (Critical) | @n-takatsu | 3-4週間 |
| #212 | Backlog | P0 (Critical) | @n-takatsu | 2-3週間 |
| #213 | Backlog | P1 (High) | @n-takatsu | 2週間 |
| #214 | Backlog | P1 (High) | @n-takatsu | 3週間 |

**配置手順：**
1. Issueカードをドラッグ＆ドロップで移動
2. または、カードをクリック→"Status"を変更

---

## 🤖 自動化設定

### ステップ6: Workflow Automation

**設定手順：**
1. Project Settings → "Workflows"
2. 以下のワークフローを有効化：

**Auto-add to project**
```yaml
When: Issues are created
Action: Add to project (Status: Backlog)
Filter: Repository: n-takatsu/mcp-rs
```

**Auto-move to In Progress**
```yaml
When: Issue is assigned
Action: Set Status to "In Progress"
```

**Auto-move to Done**
```yaml
When: Issue is closed
Action: Set Status to "Done"
```

**Auto-move to Review**
```yaml
When: Pull request is opened
Action: Set Status to "Review"
```

---

## 📌 View（ビュー）の設定

### ステップ7: カスタムビューの作成

**1. Priority View（優先度別）**
- Group by: Priority
- Sort: Priority (P0 → P3)
- Filter: Status != Done

**2. Assignee View（担当者別）**
- Group by: Assignees
- Sort: Status
- Filter: Status = In Progress OR Status = Review

**3. Milestone View（マイルストーン別）**
- Group by: Milestone
- Sort: Due date
- Filter: Milestone = v0.2.0-beta

**設定手順：**
1. Project Board右上の "View options"
2. "New view" → "Board"または"Table"
3. 上記設定を適用

---

## 🔗 Issue連携の確認

### ステップ8: 動作確認

**確認項目：**
- [ ] Issue #211-214がProjectに追加されている
- [ ] 各Issueが"Backlog"カラムにある
- [ ] Priority, Assignee, Milestoneが正しく設定されている
- [ ] Issueをクリックして詳細が表示される

**テスト操作：**
1. Issue #211を"In Progress"に移動
2. Issueページでステータスが更新されているか確認
3. "Backlog"に戻す

---

## 📊 Project管理のベストプラクティス

### 日次運用

**毎日のタスク：**
- [ ] Project Boardを開く
- [ ] "In Progress"カラムを確認（3-5個まで）
- [ ] 完了したIssueを"Done"に移動
- [ ] 新規Issueを"Backlog"に追加

### 週次レビュー

**毎週のタスク：**
- [ ] ROADMAP Health Check Issue (#210)を確認
- [ ] Milestoneの進捗率をチェック
- [ ] "Backlog"の優先順位を見直し
- [ ] ブロックされているIssueを特定

### Milestone完了時

**リリース前のタスク：**
- [ ] すべてのIssueが"Done"または次のMilestoneに移動
- [ ] Project Boardのスナップショット作成
- [ ] 振り返りIssueを作成（Retrospective）
- [ ] 次のMilestone用Projectを作成

---

## 🎯 現在のIssue一覧

### v0.2.0-beta Milestone (Due: 2026-01-31)

| Issue | Title | Priority | Status | Effort |
|-------|-------|----------|--------|--------|
| [#211](https://github.com/n-takatsu/mcp-rs/issues/211) | 🚨 動的ポリシー更新システム | P0 | Backlog | 3-4週間 |
| [#212](https://github.com/n-takatsu/mcp-rs/issues/212) | 🐳 Docker ランタイム統合 | P0 | Backlog | 2-3週間 |
| [#214](https://github.com/n-takatsu/mcp-rs/issues/214) | 🗄️ PostgreSQL最適化 | P1 | Backlog | 3週間 |

### v0.3.0 Milestone (Due: 2026-04-30)

| Issue | Title | Priority | Status | Effort |
|-------|-------|----------|--------|--------|
| [#213](https://github.com/n-takatsu/mcp-rs/issues/213) | 🔌 WebSocket Transport基盤 | P1 | Backlog | 2週間 |

---

## 🔧 トラブルシューティング

### よくある問題

**Q: Issueが自動的にProjectに追加されない**
- A: Workflow Automationが有効か確認
- A: リポジトリの権限設定を確認

**Q: カラム移動ができない**
- A: Project Settingsで"Status"フィールドが設定されているか確認
- A: ブラウザのキャッシュをクリア

**Q: カスタムフィールドが表示されない**
- A: View Optionsで該当フィールドを表示設定に追加

---

## 📚 参考リンク

- [GitHub Projects Documentation](https://docs.github.com/en/issues/planning-and-tracking-with-projects)
- [プロジェクト管理ガイド](./project-management-guide.md)
- [ROADMAP.md](../ROADMAP.md)

---

**作成日**: 2026年1月27日  
**最終更新**: 2026年1月27日  
**メンテナ**: @n-takatsu
