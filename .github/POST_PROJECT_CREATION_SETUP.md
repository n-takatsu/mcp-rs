# 🎉 プロジェクト作成完了後の設定ガイド

## 🎯 プロジェクト作成直後にやること

## 1. プロジェクト番号確認

作成後のURL例: `https://github.com/users/n-takatsu/projects/1`
→ プロジェクト番号 = `1`

## 2. カスタムフィールド追加（重要）

プロジェクトボード右上の **⚙️ Settings** から以下を追加：

### 📊 Priority (Single select)

- **P0 (Critical)** - 🔴 赤 `#DC2626`
- **P1 (High)** - 🟠 オレンジ `#EA580C`
- **P2 (Medium)** - 🟡 琥珀 `#D97706`
- **P3 (Low)** - 🟢 緑 `#65A30D`

### 🎯 Issue Type (Single select)

- **Epic** - 🟣 紫 `#7C3AED`
- **Sub-Issue** - 🔵 青 `#2563EB`
- **Bug** - 🔴 赤 `#DC2626`
- **Enhancement** - 🟢 エメラルド `#059669`

### 📦 Release Version (Single select)

- **v0.2.0-beta** - 🔵 青 `#1D4ED8`
- **v0.3.0** - 🟤 茶 `#7C2D12`
- **v1.0.0** - 🟣 ピンク `#BE185D`
- **Future** - ⚪ グレー `#6B7280`

### 📅 Implementation Phase (Single select)

- **Planning** - ⚪ グレー `#6B7280`
- **In Progress** - 🟡 琥珀 `#D97706`
- **Testing** - 🔵 青 `#2563EB`
- **Completed** - 🟢 エメラルド `#059669`
- **Blocked** - 🔴 赤 `#DC2626`

### ⏱️ Estimated Effort (Single select)

- **1-2 days** - 🟢 緑 `#10B981`
- **1 week** - 🔵 青 `#3B82F6`
- **2-3 weeks** - 🟡 黄 `#F59E0B`
- **1+ months** - 🔴 赤 `#EF4444`

### 💰 Business Value (Single select)

- **Critical** - 🔴 赤 `#DC2626`
- **High** - 🟠 オレンジ `#EA580C`
- **Medium** - 🟡 琥珀 `#D97706`
- **Low** - 🟢 緑 `#65A30D`

## 3. カスタムビュー作成

### 🗺️ ROADMAP Overview (Board View)

- **View name**: `ROADMAP Overview`
- **Group by**: `Release Version`
- **Filter**: `is:open`

### 🎯 Epic Dashboard (Table View)

- **View name**: `Epic Dashboard`
- **Filter**: `label:epic`
- **Columns**: Title, Priority, Release Version, Implementation Phase, Assignees

### ⚡ Active Sprint (Board View)

- **View name**: `Active Sprint`
- **Group by**: `Implementation Phase`
- **Filter**: `milestone:"v0.2.0-beta"`

## 4. Epic Issues フィールド設定

### Epic #17 (Advanced Security)

- Priority: **P1 (High)**
- Issue Type: **Epic**
- Release Version: **v0.2.0-beta**
- Implementation Phase: **In Progress**
- Business Value: **High**

### Epic #39 (Docker/K8s)

- Priority: **P0 (Critical)**
- Issue Type: **Epic**
- Release Version: **v0.2.0-beta**
- Implementation Phase: **Planning**
- Business Value: **Critical**

### Epic #40 (WebSocket/AI)

- Priority: **P1 (High)**
- Issue Type: **Epic**
- Release Version: **v0.3.0**
- Implementation Phase: **Planning**
- Business Value: **High**

### Epic #41 (Enterprise)

- Priority: **P3 (Low)**
- Issue Type: **Epic**
- Release Version: **v1.0.0**
- Implementation Phase: **Planning**
- Business Value: **Medium**

## 5. Sub-Issues 一括設定

### v0.2.0-beta Sub-Issues (#42-#47)

- Priority: **P0-P1**
- Issue Type: **Sub-Issue**
- Release Version: **v0.2.0-beta**
- Implementation Phase: **Planning**
- Business Value: **High**

### v0.3.0 Sub-Issues (#48-#50)

- Priority: **P1-P2**
- Issue Type: **Sub-Issue**
- Release Version: **v0.3.0**
- Implementation Phase: **Planning**
- Business Value: **Medium-High**

### v1.0.0 Sub-Issues (#51-#55)

- Priority: **P2-P3**
- Issue Type: **Sub-Issue**
- Release Version: **v1.0.0**
- Implementation Phase: **Planning**
- Business Value: **Medium**

## 6. 自動化ワークフロー有効化

プロジェクト番号確認後、PowerShellで実行：

```powershell

## プロジェクト番号を実際の値に設定

$PROJECT_NUMBER = "1"  

## 実際の番号に変更

## ワークフローファイル更新

(Get-Content .github\workflows\roadmap-sync.yml) -replace 'PROJECT_NUMBER: .*', "PROJECT_NUMBER: $PROJECT_NUMBER" | Set-Content .github\workflows\roadmap-sync.yml

## 設定確認

Write-Host "✅ 自動化システム有効化完了！" -ForegroundColor Green
Write-Host "🔗 プロジェクトURL: https://github.com/n-takatsu/mcp-rs/projects/$PROJECT_NUMBER" -ForegroundColor Blue
```

## 🎉 完成時の機能

## 📊 **リアルタイム可視化**

- Epic 別進捗追跡
- Milestone 達成状況
- 優先度別タスク管理

## 🤖 **自動化機能**

- 新規Issue の自動分類
- Epic-Sub Issue 関連付け
- 週次健全性レポート生成

## 📈 **継続改善**

- 進捗データ分析
- ボトルネック特定
- 自動最適化提案

---

**次のアクション**: "Create project" をクリックして、プロジェクト番号をお知らせください！
