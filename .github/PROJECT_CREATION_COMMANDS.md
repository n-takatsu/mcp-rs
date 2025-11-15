# 🎯 プロジェクト作成完了 - 実行コマンド

## 📋 プロジェクトボード作成後の実行手順

### 1. プロジェクト番号確認
Web UIでプロジェクト作成後、URLから番号を確認：
```
例: https://github.com/users/n-takatsu/projects/1
→ プロジェクト番号 = 1
```

### 2. Issues 一括追加実行
PowerShellで以下を実行：

```powershell
# スクリプトの実行権限設定（初回のみ）
Set-ExecutionPolicy -ExecutionPolicy RemoteSigned -Scope CurrentUser

# Issues追加実行（プロジェクト番号を実際の値に置き換え）
.\.github\add-issues-to-project-detailed.ps1 -ProjectNumber 1
```

### 3. 自動化ワークフローの有効化
プロジェクト番号確認後、以下のファイルを更新：

`.github/workflows/roadmap-sync.yml` の PROJECT_NUMBER を更新：
```yaml
env:
  PROJECT_NUMBER: 1  # 実際のプロジェクト番号に変更
```

### 4. 動作確認
```powershell
# 新規テストIssue作成で自動化確認
gh issue create --title "[TEST] 自動化テスト" --body "プロジェクト自動追加テスト"
```

## ⚡ ワンライナー実行（プロジェクト番号判明後）

```powershell
# プロジェクト番号を1と仮定した場合の完全実行
$PROJECT = 1; .\.github\add-issues-to-project-detailed.ps1 -ProjectNumber $PROJECT; (Get-Content .github\workflows\roadmap-sync.yml) -replace 'PROJECT_NUMBER: .*', "PROJECT_NUMBER: $PROJECT" | Set-Content .github\workflows\roadmap-sync.yml; Write-Host "🎉 セットアップ完了！プロジェクトURL: https://github.com/n-takatsu/mcp-rs/projects/$PROJECT" -ForegroundColor Green
```

---

**現在の作業**: Web UIでプロジェクトボードを作成し、プロジェクト番号をお知らせください。即座に18件のIssuesを追加し、完全な自動化システムを有効化します。
