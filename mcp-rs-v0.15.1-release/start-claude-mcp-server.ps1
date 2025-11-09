# MCP-RS Server for Claude Desktop (STDIO Mode)
# Claude Desktop専用 - STDIOモードでMCPサーバーを起動

Write-Host "========================================" -ForegroundColor Cyan
Write-Host "MCP-RS Server for Claude Desktop" -ForegroundColor Cyan  
Write-Host "STDIO Mode - Version 0.15.1" -ForegroundColor Cyan
Write-Host "========================================" -ForegroundColor Cyan
Write-Host

# 設定ファイルの確認
if (-not (Test-Path "mcp-config-claude.toml")) {
    Write-Host "[ERROR] mcp-config-claude.toml not found" -ForegroundColor Red
    Write-Host "Please ensure mcp-config-claude.toml is in the same directory" -ForegroundColor Red
    Read-Host "Press Enter to exit"
    exit 1
}

# 実行ファイルの確認  
if (-not (Test-Path "mcp-rs.exe")) {
    Write-Host "[ERROR] mcp-rs.exe not found" -ForegroundColor Red
    Write-Host "Please ensure mcp-rs.exe is in the same directory" -ForegroundColor Red
    Read-Host "Press Enter to exit"
    exit 1
}

Write-Host "[INFO] Starting MCP Server in STDIO mode..." -ForegroundColor Green
Write-Host "[INFO] Config: mcp-config-claude.toml" -ForegroundColor Yellow
Write-Host "[INFO] Mode: Claude Desktop Integration" -ForegroundColor Yellow
Write-Host

Write-Host "[NOTICE] This server runs in STDIO mode for Claude Desktop" -ForegroundColor Magenta
Write-Host "[NOTICE] No HTTP/TCP ports will be opened" -ForegroundColor Magenta
Write-Host "[NOTICE] Press Ctrl+C to stop the server" -ForegroundColor Magenta
Write-Host

try {
    # Claude Desktop用STDIO モードでサーバー起動
    & ".\mcp-rs.exe" --config "mcp-config-claude.toml"
}
catch {
    Write-Host "[ERROR] Failed to start MCP Server: $_" -ForegroundColor Red
}
finally {
    Write-Host
    Write-Host "[INFO] MCP Server stopped" -ForegroundColor Yellow
    Read-Host "Press Enter to exit"
}