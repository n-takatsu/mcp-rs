@echo off
REM MCP-RS Server for Claude Desktop (STDIO Mode)
REM Claude Desktop専用 - STDIOモードでMCPサーバーを起動

echo ========================================
echo MCP-RS Server for Claude Desktop
echo STDIO Mode - Version 0.15.1
echo ========================================
echo.

REM 設定ファイルの確認
if not exist "mcp-config-claude.toml" (
    echo [ERROR] mcp-config-claude.toml not found
    echo Please ensure mcp-config-claude.toml is in the same directory
    pause
    exit /b 1
)

REM 実行ファイルの確認
if not exist "mcp-rs.exe" (
    echo [ERROR] mcp-rs.exe not found
    echo Please ensure mcp-rs.exe is in the same directory
    pause
    exit /b 1
)

echo [INFO] Starting MCP Server in STDIO mode...
echo [INFO] Config: mcp-config-claude.toml
echo [INFO] Mode: Claude Desktop Integration
echo.
echo [NOTICE] This server runs in STDIO mode for Claude Desktop
echo [NOTICE] No HTTP/TCP ports will be opened
echo [NOTICE] Press Ctrl+C to stop the server
echo.

REM Claude Desktop用STDIO モードでサーバー起動
mcp-rs.exe --config mcp-config-claude.toml

echo.
echo [INFO] MCP Server stopped
pause