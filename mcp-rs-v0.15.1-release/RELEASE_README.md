# MCP-RS v0.15.1 Release Package

**Release Date**: 2025年11月9日  
**Version**: v0.15.1  
**Major Features**: Claude Desktop MCP Integration + HTTP JSON-RPC Server

## 🎯 **新機能 (v0.15.1)**

### 🤖 **Claude Desktop統合**
- **STDIO MCP Protocol**: Claude Desktopとの直接統合
- **WordPress リソースアクセス**: カテゴリ・タグの取得
- **スタンドアロンパッケージ**: 完全な配布用パッケージ

### 🌐 **HTTP JSON-RPC Server**
- **Axum Framework**: 高性能HTTPサーバー
- **JSON-RPC 2.0**: 標準プロトコル準拠
- **CORS対応**: クロスオリジンリクエスト可能
- **AI Agent対応**: Claude.ai web_fetchツール互換

### 🔧 **デュアルサーバーアーキテクチャ**
- **STDIO mode** (`stdio = true`): Claude Desktop用
- **HTTP+TCP mode** (`stdio = false`): 
  - TCP: `127.0.0.1:8080` (既存クライアント)
  - HTTP: `127.0.0.1:8081` (AI Agent用)

## 📦 **パッケージ内容**

```
mcp-rs-v0.15.1-release/
├── 🔧 mcp-rs.exe                           # 実行ファイル (6.26MB)
├── ⚙️ mcp-config.toml                      # 汎用設定ファイル (HTTP+TCP)
├── 🎯 mcp-config-claude.toml               # Claude Desktop専用設定 (STDIO)
├── 🔗 claude_desktop_config_example.json   # Claude Desktop統合設定
├── � start-claude-mcp-server.bat          # Claude Desktop起動スクリプト (Windows)
├── 🚀 start-claude-mcp-server.ps1          # Claude Desktop起動スクリプト (PowerShell)
├── 🌐 start-http-mcp-server.bat            # HTTP+TCP起動スクリプト (Windows)
├── �📝 README.md                           # メインドキュメント  
├── 🧪 test-*.ps1                          # テストスクリプト群
├── 🌐 test-get-endpoints.html             # HTTP APIテストページ
└── 📋 RELEASE_README.md                   # このファイル
```

## 🚀 **クイックスタート**

### Claude Desktop統合 (推奨)

#### **ステップ1: 設定ファイル配置**
```powershell
# Claude Desktop設定ディレクトリを作成
mkdir $env:APPDATA\Claude -Force

# 設定ファイルをコピー
copy claude_desktop_config_example.json $env:APPDATA\Claude\claude_desktop_config.json
```

#### **ステップ2: claude_desktop_config.json 編集**
```json
{
  "mcpServers": {
    "mcp-rs-wordpress": {
      "command": "C:/Users/YOUR_USERNAME/Desktop/mcp-rs-v0.15.1-final/mcp-rs.exe",
      "args": [
        "--config",
        "C:/Users/YOUR_USERNAME/Desktop/mcp-rs-v0.15.1-final/mcp-config-claude.toml"
      ],
      "env": {
        "RUST_LOG": "error"
      }
    }
  }
}
```

**重要**: 
- `YOUR_USERNAME` を実際のWindowsユーザー名に変更
- パス区切りは `/` を使用 (`\` ではなく)
- `mcp-config-claude.toml` を使用 (STDIO専用設定)
- `RUST_LOG: "error"` で最小限のログレベル設定

#### **ステップ3: WordPress設定**
`C:/Users/YOUR_USERNAME/Desktop/mcp-rs-v0.15.1-final/mcp-config-claude.toml` を編集:
```toml
[handlers.wordpress]
url = "https://your-wordpress-site.com"
username = "your_username"
password = "your_app_password"  # WordPress Application Password
enabled = true
```

#### **ステップ4: MCPサーバー起動（オプション）**
**Claude Desktopが自動起動しない場合は手動起動**:
```batch
# Windows バッチファイル
start-claude-mcp-server.bat

# または PowerShell
./start-claude-mcp-server.ps1
```

#### **ステップ5: 動作確認**
1. Claude Desktop を**完全終了**
2. Claude Desktop を再起動  
3. 新しい会話で実行:
   ```
   WordPressサイトのカテゴリ一覧を取得してください
   ```

### HTTP JSON-RPC サーバー (AI Agent用)
```batch
# 簡単起動 (Windows)
start-http-mcp-server.bat

# 手動起動
mcp-rs.exe --config mcp-config.toml

# APIテスト
curl -X POST http://127.0.0.1:8081/mcp \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"resources/read","params":{"uri":"wordpress://categories"},"id":1}'
```

## ⚙️ **設定**

### WordPress REST API設定
```toml
[handlers.wordpress]
url = "https://your-wordpress-site.com"
username = "your_username"
password = "your_app_password"  # WordPress Application Password
enabled = true
```

### Claude Desktop設定
設定ファイル: `%APPDATA%\Claude\claude_desktop_config.json`

```json
{
  "mcpServers": {
    "mcp-rs-wordpress": {
      "command": "C:/Users/YOUR_USERNAME/Desktop/mcp-rs-v0.15.1-final/mcp-rs.exe",
      "args": [
        "--config",
        "C:/Users/YOUR_USERNAME/Desktop/mcp-rs-v0.15.1-final/mcp-config-claude.toml"
      ],
      "env": {
        "RUST_LOG": "error"
      }
    }
  }
}
```

**設定のポイント**:
- **設定ファイル場所**: Windows: `%APPDATA%\Claude\claude_desktop_config.json`
- **実行ファイルパス**: 絶対パス推奨、パス区切りは `/`
- **専用設定**: `mcp-config-claude.toml` (STDIO mode)
- **環境変数**: `RUST_LOG="error"` で最小限のログレベル
- **パッケージ**: `mcp-rs-v0.15.1-final` ディレクトリを使用

## 🧪 **テストツール & 起動スクリプト**

### **起動スクリプト**
- `start-claude-mcp-server.bat` - Claude Desktop用起動 (Windows)
- `start-claude-mcp-server.ps1` - Claude Desktop用起動 (PowerShell)  
- `start-http-mcp-server.bat` - HTTP+TCP用起動 (Windows)

### **テストスクリプト**
- `test-http-jsonrpc.ps1` - HTTP JSON-RPC完全テスト
- `test-categories-stdio.ps1` - STDIO mode テスト  
- `test-categories-tcp.ps1` - TCP mode テスト
- `test-get-endpoints.html` - ブラウザ用APIテスト

## 🔍 **トラブルシューティング**

### Claude Desktop統合の問題

#### **1. 設定ファイル確認**
```powershell
# 設定ファイルの存在確認
Test-Path "$env:APPDATA\Claude\claude_desktop_config.json"

# 設定ファイルの内容確認
Get-Content "$env:APPDATA\Claude\claude_desktop_config.json"
```

#### **2. パス設定の確認**
- ❌ 間違い: `"C:\Users\takat\Desktop\mcp-rs-server\mcp-rs.exe"`
- ✅ 正しい: `"C:/Users/takat/Desktop/mcp-rs-v0.15.1-final/mcp-rs.exe"`
- ❌ 間違い: `["--config", "mcp-config.toml"]`  
- ✅ 正しい: `["--config", "C:/Users/takat/Desktop/mcp-rs-v0.15.1-final/mcp-config-claude.toml"]`
- ❌ 間違い: `"RUST_LOG": "info"`
- ✅ 正しい: `"RUST_LOG": "error"`

#### **3. Claude Desktop プロセス確認**
```powershell
# Claude Desktop完全終了
taskkill /f /im "claude.exe" 2>$null

# プロセス確認
Get-Process -Name "claude" -ErrorAction SilentlyContinue
```

#### **4. MCP接続診断**
- Claude Desktop: Settings → Developer → MCP Logs  
- エラーメッセージの確認
- サーバー起動ログの確認

### HTTP サーバーアクセスエラー
1. **ポート確認**: 8081番ポートが利用可能か
2. **設定確認**: WordPress認証情報
3. **ファイアウォール**: ローカル接続許可
4. **CORS**: クロスオリジンリクエスト設定

## 📊 **システム要件**

- **OS**: Windows 10/11, Linux, macOS
- **Memory**: 最小128MB RAM
- **Storage**: 50MB以上の空き容量
- **Network**: WordPress REST APIアクセス用

## 🔄 **v0.15.0からの変更点**

- ✅ Claude Desktop MCP統合機能追加
- ✅ HTTP JSON-RPC サーバー実装
- ✅ デュアルサーバーアーキテクチャ
- ✅ 包括的テストスイート
- ✅ 詳細ドキュメント整備
- ✅ Windows パス互換性修正
- ✅ コード品質向上 (Clippy, rustfmt準拠)

## 📞 **サポート**

- **GitHub Issues**: https://github.com/n-takatsu/mcp-rs/issues
- **Documentation**: https://n-takatsu.github.io/mcp-rs/
- **License**: MIT OR Apache-2.0

---

**🎉 Claude Desktop統合により、AI AgentがWordPressリソースに直接アクセス可能になりました！**