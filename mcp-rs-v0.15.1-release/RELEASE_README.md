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
├── ⚙️ mcp-config.toml                      # 設定ファイル
├── 🔗 claude_desktop_config_example.json   # Claude Desktop統合設定
├── 📝 README.md                           # メインドキュメント  
├── 🧪 test-*.ps1                          # テストスクリプト群
├── 🌐 test-get-endpoints.html             # HTTP APIテストページ
└── 📋 RELEASE_README.md                   # このファイル
```

## 🚀 **クイックスタート**

### Claude Desktop統合 (推奨)
```bash
# 1. Claude Desktop設定ファイルを配置
copy claude_desktop_config_example.json %APPDATA%\Claude\claude_desktop_config.json

# 2. 設定ファイルのパスを修正 (実際のパスに変更)
# "command": "C:/path/to/mcp-rs.exe"

# 3. Claude Desktop再起動
# 4. 動作確認
# "WordPressサイトのカテゴリ一覧を取得してください"
```

### HTTP JSON-RPC サーバー (AI Agent用)
```bash
# サーバー起動
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
```json
{
  "mcpServers": {
    "mcp-rs-wordpress": {
      "command": "C:/path/to/mcp-rs.exe",
      "args": ["--config", "C:/path/to/mcp-config.toml"],
      "env": { "RUST_LOG": "info" }
    }
  }
}
```

## 🧪 **テストツール**

- `test-http-jsonrpc.ps1` - HTTP JSON-RPC完全テスト
- `test-categories-stdio.ps1` - STDIO mode テスト  
- `test-categories-tcp.ps1` - TCP mode テスト
- `test-get-endpoints.html` - ブラウザ用APIテスト

## 🔍 **トラブルシューティング**

### Claude Desktop統合の問題
1. **設定ファイル確認**: `%APPDATA%\Claude\claude_desktop_config.json`
2. **パス確認**: 実行ファイルの絶対パス使用 (パス区切りは`/`)
3. **Claude Desktop再起動**: 完全終了後に再起動
4. **MCP Logs確認**: Settings → Developer → MCP Logs

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