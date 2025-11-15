# 🎯 GitHub Projects - Issues一括追加スクリプト

param(
    [Parameter(Mandatory=$true)]
    [string]$ProjectNumber
)

Write-Host "🚀 mcp-rs ROADMAP Management - Issues一括追加開始" -ForegroundColor Green
Write-Host "プロジェクト番号: $ProjectNumber" -ForegroundColor Yellow

# Epic Issues の追加
Write-Host "`n📋 Epic Issues をプロジェクトに追加中..." -ForegroundColor Cyan

$epicIssues = @(17, 39, 40, 41)
$epicTitles = @(
    "#17: Advanced Security Features",
    "#39: Docker/Kubernetes統合",
    "#40: WebSocket Transport & AI統合",
    "#41: エンタープライズ機能"
)

for ($i = 0; $i -lt $epicIssues.Length; $i++) {
    $issue = $epicIssues[$i]
    $title = $epicTitles[$i]

    try {
        Write-Host "追加中: $title" -ForegroundColor White
        gh project item-add $ProjectNumber --owner n-takatsu --url "https://github.com/n-takatsu/mcp-rs/issues/$issue"
        Write-Host "✅ 完了: $title" -ForegroundColor Green
        Start-Sleep -Seconds 1
    }
    catch {
        Write-Host "❌ 失敗: $title - $($_.Exception.Message)" -ForegroundColor Red
    }
}

# Sub-Issues の追加
Write-Host "`n🔧 Sub-Issues をプロジェクトに追加中..." -ForegroundColor Cyan

$subIssues = 42..55

foreach ($issue in $subIssues) {
    $category = switch ($issue) {
        {$_ -in 42..47} { "v0.2.0-beta" }
        {$_ -in 48..50} { "v0.3.0" }
        {$_ -in 51..55} { "v1.0.0" }
    }

    try {
        Write-Host "追加中: Sub-Issue #$issue ($category)" -ForegroundColor White
        gh project item-add $ProjectNumber --owner n-takatsu --url "https://github.com/n-takatsu/mcp-rs/issues/$issue"
        Write-Host "✅ 完了: Sub-Issue #$issue" -ForegroundColor Green
        Start-Sleep -Seconds 0.5
    }
    catch {
        Write-Host "❌ 失敗: Sub-Issue #$issue - $($_.Exception.Message)" -ForegroundColor Red
    }
}

Write-Host "`n🎉 Issues 追加完了！" -ForegroundColor Green

# 統計表示
Write-Host "`n📊 追加されたIssues:" -ForegroundColor Magenta
Write-Host "├─ Epic Issues: 4件 (#17, #39, #40, #41)" -ForegroundColor White
Write-Host "├─ v0.2.0-beta Sub-Issues: 6件 (#42-#47)" -ForegroundColor Blue
Write-Host "├─ v0.3.0 Sub-Issues: 3件 (#48-#50)" -ForegroundColor Yellow
Write-Host "├─ v1.0.0 Sub-Issues: 5件 (#51-#55)" -ForegroundColor Magenta
Write-Host "└─ 合計: 18件" -ForegroundColor Green

Write-Host "`n🔗 プロジェクトURL: https://github.com/n-takatsu/mcp-rs/projects/$ProjectNumber" -ForegroundColor Blue

Write-Host "`n📋 次のステップ:" -ForegroundColor Yellow
Write-Host "1. カスタムフィールドの設定（Priority, Issue Type, Release Version等）" -ForegroundColor White
Write-Host "2. ビューの作成（Epic Dashboard, Active Sprint等）" -ForegroundColor White
Write-Host "3. 各Issueのフィールド値設定" -ForegroundColor White
Write-Host "4. ワークフローのPROJECT_NUMBER更新" -ForegroundColor White

Write-Host "`n💡 詳細設定手順: .github/PROJECTS_MANUAL_SETUP.md を参照" -ForegroundColor Cyan
