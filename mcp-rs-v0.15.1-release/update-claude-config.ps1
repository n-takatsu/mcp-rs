# Claude Desktop MCP 設定更新スクリプト
# mcp-rs-v0.15.1-final 用の正しい設定をClaude Desktopに適用

Write-Host "🔧 Claude Desktop MCP 設定更新スクリプト" -ForegroundColor Cyan
Write-Host "Version: mcp-rs-v0.15.1-final" -ForegroundColor Green
Write-Host ""

# ユーザー名を取得
$username = $env:USERNAME
Write-Host "💡 検出されたユーザー名: $username" -ForegroundColor Yellow

# 設定ファイルのパス
$configDir = "$env:APPDATA\Claude"
$configFile = "$configDir\claude_desktop_config.json"

Write-Host "📁 設定ファイルパス: $configFile" -ForegroundColor Blue

# ディレクトリが存在しない場合は作成
if (-not (Test-Path $configDir)) {
    New-Item -ItemType Directory -Path $configDir -Force | Out-Null
    Write-Host "✅ Claude設定ディレクトリを作成しました" -ForegroundColor Green
}

# 正しい設定を作成
$config = @{
    mcpServers = @{
        "mcp-rs-wordpress" = @{
            command = "C:/Users/$username/Desktop/mcp-rs-v0.15.1-final/mcp-rs.exe"
            args = @(
                "--config",
                "C:/Users/$username/Desktop/mcp-rs-v0.15.1-final/mcp-config-claude.toml"
            )
            env = @{
                "RUST_LOG" = "error"
            }
        }
    }
}

# JSONファイルに書き込み
try {
    $config | ConvertTo-Json -Depth 10 | Out-File $configFile -Encoding UTF8 -Force
    Write-Host "✅ Claude Desktop設定を更新しました" -ForegroundColor Green
    Write-Host ""
    Write-Host "📋 設定内容:" -ForegroundColor Cyan
    Write-Host "- サーバー名: mcp-rs-wordpress" -ForegroundColor White
    Write-Host "- 実行ファイル: C:/Users/$username/Desktop/mcp-rs-v0.15.1-final/mcp-rs.exe" -ForegroundColor White
    Write-Host "- 設定ファイル: mcp-config-claude.toml" -ForegroundColor White
    Write-Host "- ログレベル: error" -ForegroundColor White
    Write-Host ""
    Write-Host "🚀 次のステップ:" -ForegroundColor Yellow
    Write-Host "1. Claude Desktop を完全終了" -ForegroundColor White
    Write-Host "2. Claude Desktop を再起動" -ForegroundColor White
    Write-Host "3. 新しい会話で 'WordPressのカテゴリ一覧を取得してください' を実行" -ForegroundColor White
}
catch {
    Write-Host "❌ 設定ファイルの更新に失敗しました: $_" -ForegroundColor Red
}

Write-Host ""
Write-Host "📝 設定確認:" -ForegroundColor Cyan
Write-Host "Get-Content '$configFile'" -ForegroundColor Gray
