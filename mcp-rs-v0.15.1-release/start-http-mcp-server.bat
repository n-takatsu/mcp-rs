@echo off
REM MCP-RS Server for HTTP+TCP Mode
REM AI Agent用 - HTTP JSON-RPC + TCPモードでMCPサーバーを起動

echo ========================================
echo MCP-RS Server for AI Agents
echo HTTP+TCP Mode - Version 0.15.1
echo ========================================
echo.

REM 設定ファイルの確認
if not exist "mcp-config.toml" (
    echo [ERROR] mcp-config.toml not found
    echo Please ensure mcp-config.toml is in the same directory
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

echo [INFO] Starting MCP Server in HTTP+TCP mode...
echo [INFO] Config: mcp-config.toml
echo [INFO] Mode: AI Agent Integration
echo.
echo [NOTICE] Server will be available at:
echo [NOTICE]   TCP Server: 127.0.0.1:8080
echo [NOTICE]   HTTP Server: 127.0.0.1:8081
echo [NOTICE] Press Ctrl+C to stop the server
echo.

REM AI Agent用HTTP+TCP モードでサーバー起動
mcp-rs.exe --config mcp-config.toml

echo.
echo [INFO] MCP Server stopped
pause