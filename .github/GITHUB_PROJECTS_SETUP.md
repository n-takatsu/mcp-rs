# 🎯 GitHub Projects v2 設定ガイド

> **対象**: ROADMAP v1.1統合プロジェクト管理システム
> **作成日**: 2025年11月9日
> **ステータス**: 設定準備完了

## 🚀 Projects v2 プロジェクトボード作成

## 1. 基本プロジェクト作成

```bash

## GitHub CLI での作成（権限設定後）

gh auth refresh -s project,read:project
gh project create --title "mcp-rs ROADMAP Management" --owner "n-takatsu"
```

**または Web UI での作成:**
1. https://github.com/n-takatsu/mcp-rs/projects にアクセス
2. "New project" → "Board" を選択
3. プロジェクト名: `mcp-rs ROADMAP Management`
4. 説明: `ROADMAP v1.1に基づく統合プロジェクト管理。Epic Issues、Sub-Issues、Milestonesの一元管理。`

## 2. カスタムフィールド設定

### 📊 Priority フィールド（Single Select）

- **P0 (Critical)** - `#DC2626` (赤)
- **P1 (High)** - `#EA580C` (オレンジ)
- **P2 (Medium)** - `#D97706` (琥珀)
- **P3 (Low)** - `#65A30D` (緑)

### 🎯 Issue Type フィールド（Single Select）

- **Epic** - `#7C3AED` (紫)
- **Sub-Issue** - `#2563EB` (青)
- **Bug** - `#DC2626` (赤)
- **Enhancement** - `#059669` (エメラルド)

### 📦 Release Version フィールド（Single Select）

- **v0.2.0-beta** - `#1D4ED8` (青)
- **v0.3.0** - `#7C2D12` (茶)
- **v1.0.0** - `#BE185D` (ピンク)
- **Future** - `#6B7280` (グレー)

### 📅 Implementation Phase フィールド（Single Select）

- **Planning** - `#6B7280` (グレー)
- **In Progress** - `#D97706` (琥珀)
- **Testing** - `#2563EB` (青)
- **Completed** - `#059669` (エメラルド)
- **Blocked** - `#DC2626` (赤)

### ⏱️ Estimated Effort フィールド（Single Select）

- **1-2 days** - `#10B981` (緑)
- **1 week** - `#3B82F6` (青)
- **2-3 weeks** - `#F59E0B` (黄)
- **1+ months** - `#EF4444` (赤)

### 💰 Business Value フィールド（Single Select）

- **Critical** - `#DC2626` (赤)
- **High** - `#EA580C` (オレンジ)
- **Medium** - `#D97706` (琥珀)
- **Low** - `#65A30D` (緑)

### 🔗 Epic Parent フィールド（Text）

- Epic Issue番号を記録（例: "#17", "#39", "#40", "#41"）

## 3. ビュー設定

### 🗺️ ROADMAP Overview（Board View）

**グループ化**: Release Version
**フィルター**: `is:open`
**並び順**: Priority (P0 → P3), 作成日

### 🎯 Epic Dashboard（Table View）

**表示列**:
- Title, Assignees, Status, Priority, Release Version
- Implementation Phase, Estimated Effort, Business Value

**フィルター**: `label:epic is:open`
**並び順**: Priority, Release Version

### ⚡ Active Sprint（Board View）

**グループ化**: Implementation Phase
**フィルター**: `is:open -label:epic milestone:"v0.2.0-beta"`
**並び順**: Priority, 更新日

### 🔍 Sub-Issues Tracking（Table View）

**表示列**:
- Title, Epic Parent, Assignees, Status, Priority
- Implementation Phase, Estimated Effort

**フィルター**: `is:open -label:epic`
**並び順**: Epic Parent, Priority

## 4. 自動化設定（GitHub Actions対応）

### Issue 自動フィールド設定

```yaml

## .github/workflows/project-automation.yml で使用

fields:
  priority: "P1 (High)"
  issue_type: "Sub-Issue"
  release_version: "v0.2.0-beta"
  implementation_phase: "Planning"
  estimated_effort: "1 week"
  business_value: "High"
```

## 🔗 Issue 統合手順

## 既存 Issues の Project 追加

```bash

## Epic Issues を Project に追加

gh project item-add PROJECT_NUMBER --owner n-takatsu --url "https://github.com/n-takatsu/mcp-rs/issues/17"
gh project item-add PROJECT_NUMBER --owner n-takatsu --url "https://github.com/n-takatsu/mcp-rs/issues/39"
gh project item-add PROJECT_NUMBER --owner n-takatsu --url "https://github.com/n-takatsu/mcp-rs/issues/40"
gh project item-add PROJECT_NUMBER --owner n-takatsu --url "https://github.com/n-takatsu/mcp-rs/issues/41"

## Sub-Issues を Project に追加（#42-#55）

for issue in {42..55}; do
  gh project item-add PROJECT_NUMBER --owner n-takatsu --url "https://github.com/n-takatsu/mcp-rs/issues/$issue"
done
```

## フィールド一括設定

```bash

## Epic Issues のフィールド設定例

gh project item-edit --project-id PROJECT_ID --item-id ITEM_ID \
  --field-id PRIORITY_FIELD_ID --single-select-option-id P0_OPTION_ID \
  --field-id TYPE_FIELD_ID --single-select-option-id EPIC_OPTION_ID
```

## 📋 設定完了チェックリスト

- [ ] プロジェクトボード作成完了
- [ ] 6つのカスタムフィールド設定完了
- [ ] 4つの主要ビュー作成完了
- [ ] Epic Issues (4件) 追加完了
- [ ] Sub-Issues (14件) 追加完了
- [ ] フィールド値一括設定完了
- [ ] 自動化ルール設定完了

## 🎯 運用ガイドライン

## 日次作業

1. Active Sprint ビューで進捗確認
2. Implementation Phase の更新
3. ブロッカーの特定と解決

## 週次作業

1. Epic Dashboard で全体進捗レビュー
2. 優先度調整と리소스재분配
3. 新規 Sub-Issues の追加

## マイルストーン作業

1. ROADMAP Overview での戦略レビュー
2. リリース計画の調整
3. 次期マイルストーンの準備

---

**Next Steps**: GitHub Actions による自動化実装
