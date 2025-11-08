# 🚀 GitHub Projects v2 手動セットアップガイド

> **認証問題の回避**: Web UI でのプロジェクト作成手順  
> **対象**: mcp-rs ROADMAP Management システム  
> **完了時間**: 約15分  

## 📋 ステップバイステップ手順

### 🎯 Step 1: プロジェクトボード作成

#### 1.1 GitHub Projects ページにアクセス
```
URL: https://github.com/n-takatsu/mcp-rs/projects
```

#### 1.2 新規プロジェクト作成
1. **"New project"** ボタンをクリック
2. **"Board"** テンプレートを選択
3. 設定値を入力:
   - **Project name**: `mcp-rs ROADMAP Management`
   - **Description**: `ROADMAP v1.1に基づく統合プロジェクト管理。Epic Issues、Sub-Issues、Milestonesの一元管理システム。`

### 🔧 Step 2: カスタムフィールド設定

#### 2.1 Priority フィールド（Single Select）
- **Field name**: `Priority`
- **Options**:
  - `P0 (Critical)` - 色: `#DC2626` (赤)
  - `P1 (High)` - 色: `#EA580C` (オレンジ)
  - `P2 (Medium)` - 色: `#D97706` (琥珀)
  - `P3 (Low)` - 色: `#65A30D` (緑)

#### 2.2 Issue Type フィールド（Single Select）
- **Field name**: `Issue Type`
- **Options**:
  - `Epic` - 色: `#7C3AED` (紫)
  - `Sub-Issue` - 色: `#2563EB` (青)
  - `Bug` - 色: `#DC2626` (赤)
  - `Enhancement` - 色: `#059669` (エメラルド)

#### 2.3 Release Version フィールド（Single Select）
- **Field name**: `Release Version`
- **Options**:
  - `v0.2.0-beta` - 色: `#1D4ED8` (青)
  - `v0.3.0` - 色: `#7C2D12` (茶)
  - `v1.0.0` - 色: `#BE185D` (ピンク)
  - `Future` - 色: `#6B7280` (グレー)

#### 2.4 Implementation Phase フィールド（Single Select）
- **Field name**: `Implementation Phase`
- **Options**:
  - `Planning` - 色: `#6B7280` (グレー)
  - `In Progress` - 色: `#D97706` (琥珀)
  - `Testing` - 色: `#2563EB` (青)
  - `Completed` - 色: `#059669` (エメラルド)
  - `Blocked` - 色: `#DC2626` (赤)

#### 2.5 Estimated Effort フィールド（Single Select）
- **Field name**: `Estimated Effort`
- **Options**:
  - `1-2 days` - 色: `#10B981` (緑)
  - `1 week` - 色: `#3B82F6` (青)
  - `2-3 weeks` - 色: `#F59E0B` (黄)
  - `1+ months` - 色: `#EF4444` (赤)

#### 2.6 Business Value フィールド（Single Select）
- **Field name**: `Business Value`
- **Options**:
  - `Critical` - 色: `#DC2626` (赤)
  - `High` - 色: `#EA580C` (オレンジ)
  - `Medium` - 色: `#D97706` (琥珀)
  - `Low` - 色: `#65A30D` (緑)

### 📊 Step 3: ビュー設定

#### 3.1 ROADMAP Overview（Board View）
- **View name**: `ROADMAP Overview`
- **View type**: Board
- **Group by**: `Release Version`
- **Filter**: `is:open`
- **Sort**: Priority (P0 → P3), 作成日

#### 3.2 Epic Dashboard（Table View）
- **View name**: `Epic Dashboard`
- **View type**: Table
- **Columns**: Title, Assignees, Status, Priority, Release Version, Implementation Phase, Estimated Effort, Business Value
- **Filter**: `label:epic is:open`
- **Sort**: Priority, Release Version

#### 3.3 Active Sprint（Board View）
- **View name**: `Active Sprint`
- **View type**: Board
- **Group by**: `Implementation Phase`
- **Filter**: `is:open -label:epic milestone:"v0.2.0-beta"`
- **Sort**: Priority, 更新日

#### 3.4 Sub-Issues Tracking（Table View）
- **View name**: `Sub-Issues Tracking`
- **View type**: Table
- **Columns**: Title, Priority, Implementation Phase, Estimated Effort, Assignees, Status
- **Filter**: `is:open -label:epic`
- **Sort**: Priority

### 🔗 Step 4: Issues の Project 追加（CLI使用）

プロジェクト作成完了後、プロジェクト番号を確認してから以下を実行：

```bash
# プロジェクト番号確認（Web UIで確認）
# URL例: https://github.com/users/n-takatsu/projects/1 → PROJECT_NUMBER=1

# Epic Issues を Project に追加
gh project item-add 1 --owner n-takatsu --url "https://github.com/n-takatsu/mcp-rs/issues/17"
gh project item-add 1 --owner n-takatsu --url "https://github.com/n-takatsu/mcp-rs/issues/39"
gh project item-add 1 --owner n-takatsu --url "https://github.com/n-takatsu/mcp-rs/issues/40"
gh project item-add 1 --owner n-takatsu --url "https://github.com/n-takatsu/mcp-rs/issues/41"

# Sub-Issues を Project に追加
for ($i=42; $i -le 55; $i++) {
  gh project item-add 1 --owner n-takatsu --url "https://github.com/n-takatsu/mcp-rs/issues/$i"
}
```

## ✅ 完了チェックリスト

### プロジェクト基盤
- [ ] プロジェクトボード `mcp-rs ROADMAP Management` 作成完了
- [ ] 6つのカスタムフィールド設定完了
- [ ] 4つの主要ビュー作成完了

### Issues 統合
- [ ] Epic Issues (4件) 追加完了
  - [ ] #17: Advanced Security Features
  - [ ] #39: Docker/Kubernetes統合
  - [ ] #40: WebSocket Transport & AI統合
  - [ ] #41: エンタープライズ機能
- [ ] Sub-Issues (14件) 追加完了 (#42-#55)

### フィールド設定
- [ ] Epic Issues のフィールド値設定
- [ ] Sub-Issues のフィールド値設定
- [ ] Priority と Release Version の正確な設定

### 自動化確認
- [ ] GitHub Actions ワークフロー動作確認
- [ ] Issue 自動ラベル付けテスト
- [ ] Epic-Sub Issue 関連付けテスト

## 🔧 トラブルシューティング

### プロジェクト番号が分からない場合
1. プロジェクトのURL を確認: `https://github.com/users/n-takatsu/projects/X`
2. `X` がプロジェクト番号

### Issues が追加されない場合
```bash
# 手動で1件ずつ追加
gh project item-add PROJECT_NUMBER --owner n-takatsu --url "ISSUE_URL"
```

### 権限エラーの場合
```bash
# 認証更新（現在は不要だがプロジェクト権限追加時）
gh auth refresh -s project,read:project
```

## 🚀 次のアクション

### 1. プロジェクト作成完了後
- ワークフロー `.github/workflows/roadmap-sync.yml` の `PROJECT_NUMBER` 更新
- 初回 Issue 自動追加の実行

### 2. 運用開始
- [`AUTOMATION_OPERATIONS_GUIDE.md`](.github/AUTOMATION_OPERATIONS_GUIDE.md) に従った日常運用
- 週次健全性チェックの確認

### 3. 継続改善
- プロジェクトビューの最適化
- カスタムフィールドの調整
- 自動化ルールの改良

---

**完了後の確認**: プロジェクト番号をお知らせください。ワークフローファイルを更新して完全な自動化システムを有効化します。