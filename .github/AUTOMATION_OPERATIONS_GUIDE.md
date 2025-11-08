# 🤖 ROADMAP 自動化システム運用ガイド

> **対象**: mcp-rs プロジェクト開発チーム  
> **作成日**: 2025年11月9日  
> **システム範囲**: GitHub Projects v2 + GitHub Actions 統合自動化  

## 🎯 システム概要

### 🏗️ アーキテクチャ
```
ROADMAP.md (戦略) ←→ GitHub Milestones (計画) ←→ GitHub Issues (実行) ←→ Projects v2 (可視化)
      ↑                         ↑                        ↑                      ↑
      │                         │                        │                      │
   週次更新              Milestone管理            Issue自動化            カスタムフィールド
  健全性チェック         リリース計画              ラベル・関連付け         進捗ビューア
```

### 🔄 自動化ワークフロー
1. **`roadmap-sync.yml`**: ROADMAP-Issue 同期とProject統合
2. **`issue-automation.yml`**: Issue管理・ラベル・Epic連携自動化
3. **`roadmap-health.yml`**: 週次健全性チェックと進捗レポート

---

## 📋 日常運用手順

### 🌅 毎日の作業 (5分)

#### 1. Project Board 確認
```bash
# Active Sprint ビュー確認
https://github.com/n-takatsu/mcp-rs/projects/1?view=Active-Sprint
```

**チェック項目**:
- [ ] `In Progress` カラムの Sub-Issues 進捗
- [ ] `Blocked` 状態の Issues 有無
- [ ] P0 Critical Issues の対応状況

#### 2. Issue トリアージ (新規Issue対応)
- 新規 Issue は **自動ラベル付け** される
- Epic/Sub-Issue は **自動的にProject追加** される  
- **手動確認が必要な項目**:
  - [ ] Priority 設定 (P0/P1/P2/P3)
  - [ ] Assignee 割り当て
  - [ ] Epic Parent 関連付け (Sub-Issue の場合)

### 📅 週次作業 (30分)

#### 1. Epic Dashboard レビュー
```bash
# Epic 進捗確認
https://github.com/n-takatsu/mcp-rs/projects/1?view=Epic-Dashboard
```

**レビューポイント**:
- [ ] 各Epic の Sub-Issue 完了率
- [ ] Milestone 期日との進捗比較
- [ ] ブロッカーとリスクの特定

#### 2. 週次健全性レポート確認
- **自動生成される健全性チェックIssue** を確認
- アラートと推奨アクションを実行
- 必要に応じて Priority やスコープを調整

### 🎯 マイルストーン作業 (月次)

#### 1. リリース計画レビュー
```bash  
# Milestone 進捗確認
gh issue list --milestone "v0.2.0-beta" --json number,title,state
```

#### 2. ROADMAP.md 更新
- **自動更新**: 進捗インジケーター、Issue リンク
- **手動更新**: 戦略変更、新機能追加、スコープ調整

---

## 🔧 自動化機能詳細

### 📊 Issue 自動処理

#### Epic Issues (`[EPIC]` タイトル)
```yaml
自動実行内容:
- ラベル: "epic,priority-high,roadmap-tracked"  
- Project 追加: 自動
- フィールド設定: Issue Type = "Epic"
- Milestone: タイトルベース自動判定
```

#### Sub-Issues (`[SUB]` タイトル)  
```yaml
自動実行内容:
- ラベル: "sub-issue,enhancement,roadmap-tracked"
- Project 追加: 自動
- Epic 関連付け: Parent Epic 自動抽出・リンク
- Milestone 継承: Epic から自動継承
- Assignee 継承: Epic から自動継承 (オプション)
```

#### Critical Issues
```yaml
トリガー: "priority-critical", "security", "bug" ラベル
自動実行内容:  
- Assignee: @n-takatsu 自動割り当て
- 緊急対応コメント自動追加
- エスカレーション手順通知
```

### 🔗 Epic-Sub Issue 連携

#### Sub-Issue 作成時
1. **Epic 自動検出**: `Parent Epic: #XX` を Issue Body から抽出
2. **Epic コメント**: Epic Issue に Sub-Issue 作成通知
3. **設定継承**: Milestone、Assignee を Epic から継承

#### Sub-Issue 完了時  
1. **進捗更新**: Epic Issue に進捗統計コメント
2. **完了検知**: 全 Sub-Issue 完了時に Epic 完了通知
3. **自動クローズ**: PR マージ時の Sub-Issue 自動クローズ (オプション)

### 📈 進捗レポート自動生成

#### 週次健全性チェック (月曜日 9:00 JST)
```yaml
生成内容:
- 全体プロジェクト健全性
- Milestone 別進捗統計  
- Critical Issue アラート
- 長期オープン Issue 警告
- 週次開発ベロシティ分析
- 推奨アクションアイテム
```

#### ROADMAP.md 自動更新
```yaml
更新内容:  
- 進捗インジケーター (75% → 85%)
- GitHub Issues リンク (NEW → 実際の Issue番号)
- 最終更新日 (自動)
- Repository Topics (roadmap ベース)
```

---

## 🎮 手動操作ガイド

### 🆕 新機能追加時の手順

#### 1. Epic Issue 作成
```bash
# Epic Issue 作成テンプレート
gh issue create --title "[EPIC] 新機能名" \
  --body-file .github/templates/epic-issue-template.md \
  --milestone "v0.X.0" \
  --label "epic"
```

#### 2. Sub-Issues 作成
```bash  
# Sub-Issue 作成テンプレート  
gh issue create --title "[SUB] 実装タスク名" \
  --body "Parent Epic: #XX" \
  --milestone "v0.X.0" \
  --label "sub-issue"
```

### 🔄 緊急対応時の手順

#### P0 Critical Issue 処理
1. **即座に P0 ラベル追加**: `gh issue edit XX --add-label "priority-p0-critical"`
2. **自動エスカレーション**: 2時間以内初期対応、4時間以内影響評価
3. **ホットフィックス**: 必要に応じてブランチ作成・PR・緊急リリース

### 📊 カスタムレポート生成
```bash
# 特定Milestone の詳細レポート
gh issue list --milestone "v0.2.0-beta" --json number,title,state,assignees

# Epic 別 Sub-Issue 一覧
gh issue list --label "sub-issue" --json number,title,body | \
  jq '.[] | select(.body | contains("Parent Epic: #17"))'

# 週次完了統計  
gh issue list --state closed --json closedAt | \
  jq '[.[] | select(.closedAt > "'$(date -d "7 days ago" '+%Y-%m-%d')'T00:00:00Z")] | length'
```

---

## ⚠️ トラブルシューティング

### 🚨 よくある問題と解決法

#### Issue が Project に自動追加されない
```bash
# 手動でProject追加
gh project item-add 1 --owner n-takatsu --url "https://github.com/n-takatsu/mcp-rs/issues/XX"
```

#### Epic-Sub Issue リンクが正しく動作しない
- **確認**: Sub-Issue の Body に `Parent Epic: #XX` が正確に記載されているか
- **修正**: Issue Body を編集して正しいフォーマットに修正

#### ワークフロー権限エラー  
```bash
# GitHub CLI 権限更新
gh auth refresh -s project,read:project,write:discussion
```

#### 週次レポートが生成されない
- **確認**: ワークフロー実行履歴をチェック
- **手動実行**: `gh workflow run roadmap-health.yml`

### 🔧 メンテナンス作業

#### 月次クリーンアップ
- [ ] 完了した週次健全性チェック Issues をアーカイブ  
- [ ] 古い Discussion をピン留めから外す
- [ ] Project Board ビューの最適化

#### 四半期レビュー
- [ ] ワークフロー効率性の評価
- [ ] 自動化ルールの最適化  
- [ ] カスタムフィールドとビューの見直し

---

## 📈 成功指標とKPI

### 🎯 自動化効率指標
- **Issue 処理時間**: 手動→自動化による短縮効果
- **Epic 進捗可視性**: リアルタイム進捗追跡精度
- **ROADMAP 同期精度**: 自動更新の正確性

### 📊 プロジェクト健全性指標  
- **Milestone 達成率**: 期日内完了率
- **Critical Issue 対応時間**: P0 Issue の平均解決時間
- **開発ベロシティ**: 週次 Issue 完了数の推移

### 🔄 継続改善指標
- **手動作業削減**: 自動化による工数削減効果  
- **品質向上**: バグ検出・対応の迅速化
- **チーム生産性**: 開発フォーカス時間の増加

---

## 🔗 関連リソース

### 📋 設定ファイル
- [`.github/workflows/roadmap-sync.yml`](.github/workflows/roadmap-sync.yml)
- [`.github/workflows/issue-automation.yml`](.github/workflows/issue-automation.yml)  
- [`.github/workflows/roadmap-health.yml`](.github/workflows/roadmap-health.yml)
- [`.github/GITHUB_PROJECTS_SETUP.md`](.github/GITHUB_PROJECTS_SETUP.md)

### 🎯 関連ドキュメント
- [ROADMAP.md](../ROADMAP.md) - プロジェクト戦略
- [MILESTONE_CREATION_GUIDE.md](.github/MILESTONE_CREATION_GUIDE.md) - Milestone 管理
- [GITHUB_PROJECTS_INTEGRATION.md](.github/GITHUB_PROJECTS_INTEGRATION.md) - Projects 統合

### 🔧 外部ツール
- **GitHub CLI**: Issue・Project・Milestone 管理
- **GitHub Projects v2**: カスタムフィールドとビュー管理
- **GitHub Actions**: 自動化ワークフロー実行
- **GitHub Discussions**: 週次レポートとコミュニケーション

---

*この運用ガイドは、自動化システムの効果的活用と継続的改善を支援します。定期的に見直しと最適化を行ってください。*