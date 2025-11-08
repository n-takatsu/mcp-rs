# 🎯 Issue-Project 統合スクリプト（PowerShell版）

# プロジェクト番号を設定（Web UIでプロジェクト作成後に更新）
$PROJECT_NUMBER = "REPLACE_WITH_ACTUAL_PROJECT_NUMBER"

Write-Host "🎯 mcp-rs ROADMAP Management - Issue Integration" -ForegroundColor Green
Write-Host "プロジェクト番号: $PROJECT_NUMBER" -ForegroundColor Yellow

# Epic Issues をプロジェクトに追加
Write-Host "`n📋 Epic Issues をプロジェクトに追加中..." -ForegroundColor Cyan

$epicIssues = @(17, 39, 40, 41)
foreach ($issue in $epicIssues) {
    try {
        gh project item-add $PROJECT_NUMBER --owner n-takatsu --url "https://github.com/n-takatsu/mcp-rs/issues/$issue"
        Write-Host "✅ Epic Issue #$issue 追加完了" -ForegroundColor Green
    }
    catch {
        Write-Host "❌ Epic Issue #$issue 追加失敗: $($_.Exception.Message)" -ForegroundColor Red
    }
}

# Sub-Issues をプロジェクトに追加
Write-Host "`n🔧 Sub-Issues をプロジェクトに追加中..." -ForegroundColor Cyan

$subIssues = 42..55
foreach ($issue in $subIssues) {
    try {
        gh project item-add $PROJECT_NUMBER --owner n-takatsu --url "https://github.com/n-takatsu/mcp-rs/issues/$issue"
        Write-Host "✅ Sub-Issue #$issue 追加完了" -ForegroundColor Green
    }
    catch {
        Write-Host "❌ Sub-Issue #$issue 追加失敗: $($_.Exception.Message)" -ForegroundColor Red
    }
}

Write-Host "`n🎉 Issue-Project 統合完了！" -ForegroundColor Green
Write-Host "次のステップ: Web UI でカスタムフィールドの値を設定してください。" -ForegroundColor Yellow

# 統計表示
Write-Host "`n📊 統合された Issues:" -ForegroundColor Magenta
Write-Host "- Epic Issues: 4件 (#17, #39, #40, #41)" -ForegroundColor White
Write-Host "- Sub-Issues: 14件 (#42-#55)" -ForegroundColor White
Write-Host "- Total: 18件" -ForegroundColor White

Write-Host "`n🔗 プロジェクトURL: https://github.com/n-takatsu/mcp-rs/projects/$PROJECT_NUMBER" -ForegroundColor Blue