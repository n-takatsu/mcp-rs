# Claude Desktop統合ガイド

MCP-RSをClaude Desktopと統合する際の重要な設定と注意点について説明します。

## 🚨 重要: STDIO通信での注意事項

**Claude DesktopはSTDIO（標準入出力）を使用してMCPサーバーと通信します。この際、標準出力にJSON以外の内容（ログメッセージ等）が混在すると通信が破綻します。**

## ❌ 問題のある設定例

```toml

## 🚫 これは動作しません - Claude Desktopでは使用不可

[server]
stdio = true
log_level = "info"  

## コンソール出力が有効

[server.log_module]
separation = "single"  

## コンソールにログが出力される

```

この設定では、以下のようなログがJSONレスポンスと混在してしまいます：

```

{"jsonrpc":"2.0","id":1,"result":{"status":"accepted"}}
2025-11-09T09:32:13.156903Z  INFO mcp_rs: ✅ MCP-RSサーバー起動完了

```

## ✅ Claude Desktop用の正しい設定

```toml

## Claude Desktop用設定: configs/production/claude-desktop.toml

[server]
stdio = true           

## Claude DesktopはSTDIO通信

log_level = "error"    

## エラーレベルのみ（推奨）

## ログ出力設定

[server.log_retention]
policy = "external"

[server.log_module]
separation = "separated"  

## ファイル出力のみ（コンソール出力なし）

[transport]
transport_type = "stdio"  

## STDIO Transport必須

[handlers.wordpress]
enabled = true
url = "${WORDPRESS_URL}"
username = "${WORDPRESS_USERNAME}"
password = "${WORDPRESS_PASSWORD}"
timeout_seconds = 30

```

## 🔧 Claude Desktop設定ファイル

**Windows:**

```json

{
  "mcpServers": {
    "mcp-rs": {
      "command": "C:\\path\\to\\mcp-rs.exe",
      "args": ["--config", "C:\\path\\to\\configs\\production\\claude-desktop.toml"],
      "env": {
        "WORDPRESS_URL": "https://your-wordpress-site.com",
        "WORDPRESS_USERNAME": "your_username",
        "WORDPRESS_PASSWORD": "your_application_password"
      }
    }
  }
}

```

**macOS:**

```json

{
  "mcpServers": {
    "mcp-rs": {
      "command": "/path/to/mcp-rs",
      "args": ["--config", "/path/to/configs/production/claude-desktop.toml"],
      "env": {
        "WORDPRESS_URL": "https://your-wordpress-site.com",
        "WORDPRESS_USERNAME": "your_username",
        "WORDPRESS_PASSWORD": "your_application_password"
      }
    }
  }
}

```

**Linux:**

```json

{
  "mcpServers": {
    "mcp-rs": {
      "command": "/path/to/mcp-rs",
      "args": ["--config", "/path/to/configs/production/claude-desktop.toml"],
      "env": {
        "WORDPRESS_URL": "https://your-wordpress-site.com",
        "WORDPRESS_USERNAME": "your_username",
        "WORDPRESS_PASSWORD": "your_application_password"
      }
    }
  }
}

```

> ⚠️ **重要**: Claude Desktopが起動する子プロセスは、OSのユーザー環境変数を自動的に継承するとは限りません。
> 設定ファイル側で `${WORDPRESS_URL}` のようにプレースホルダを使っていても、上記のように
> `mcpServers.<name>.env` に明示的にWordPress認証情報を書かないと、変数展開に失敗して
> サーバーが起動直後に終了し「Server disconnected」エラーになります。

## 📊 ログ管理のベストプラクティス

## Claude Desktop環境

1. **コンソール出力を最小限に**: `log_level = "error"`
2. **ファイル出力を使用**: モジュール別分離でログをファイルに記録
3. **ログディレクトリ**: `実行ファイルと同じディレクトリ/logs/`

## Web UI環境（HTTP Transport）

```toml

## Web UI用設定: mcp-config-webui.toml

[server]
stdio = false          

## HTTPを使用

log_level = "info"     

## 詳細ログ可能

[server.log_module]
separation = "separated"  

## モジュール別ログファイル

[transport]
transport_type = "http"

[transport.http]
addr = "127.0.0.1"
port = 8081

```

## 🔄 Transport方式の選択

| 環境 | Transport | 設定 | ログ出力 |
|------|-----------|------|----------|
| Claude Desktop | STDIO | `transport_type = "stdio"` | ファイルのみ |
| Web UI | HTTP | `transport_type = "http"` | コンソール + ファイル |
| Custom Client | WebSocket | `transport_type = "websocket"` | 柔軟な設定 |

## 🛠️ トラブルシューティング

## Claude Desktopで「サーバーに接続できません」エラー

**原因**: 標準出力にログメッセージが混在している

**解決策**:
1. `log_level = "error"`に設定
2. コンソール出力を無効化
3. ログはファイル出力のみ使用

```toml

## 修正例

[server]
stdio = true
log_level = "error"  

## ERRORレベルのみ

[server.log_module]
separation = "separated"  

## ファイル出力

```

## ログファイルの場所がわからない

ログファイルは以下の場所に作成されます：
1. 実行ファイルと同じディレクトリの`logs/`フォルダ（優先）
2. カレントディレクトリの`logs/`フォルダ
3. システムテンプディレクトリの`mcp-rs/logs/`フォルダ

## WordPressハンドラーが動作しない

1. **`claude_desktop_config.json` の `env` を確認**:
   `mcpServers.<name>.env` に `WORDPRESS_URL` / `WORDPRESS_USERNAME` / `WORDPRESS_PASSWORD`
   が明示的に設定されているか確認してください（上記「🔧 Claude Desktop設定ファイル」参照）。
   OSの環境変数を設定しただけでは、Claude Desktopの子プロセスに渡らない場合があります。

2. **環境変数の確認**:

   ```bash
   # Windows (PowerShell)
   echo $env:WORDPRESS_URL

   # Linux/macOS
   echo $WORDPRESS_URL
   ```

3. **設定ファイルの検証**:
   `[handlers.wordpress]` テーブル配下に `url` / `username` / `password` が存在し、
   `WordPressConfig` 構造体（`src/config.rs`）が認識するフィールド名
   （`enabled` / `timeout_seconds` / `rate_limit`）のみを使用しているか確認してください。

4. **ログファイルの確認**:

   ```bash
   # WordPress関連ログ（モジュール分離設定時）
   tail -f logs/wordpress.log
   ```

## 📝 設定テンプレート

## Claude Desktop用最小設定

```toml

stdio = true
log_level = "error"

[transport]
transport_type = "stdio"

[handlers.wordpress]
enabled = true
url = "${WORDPRESS_URL}"
username = "${WORDPRESS_USERNAME}"
password = "${WORDPRESS_PASSWORD}"

```

## 本番環境用完全設定

```toml

stdio = true
log_level = "warn"

[server.log_retention]
policy = "external"  

## OS/ログ管理ツール任せ

[server.log_module]
separation = "separated"  

## モジュール別ファイル分離

[transport]
transport_type = "stdio"

[handlers.wordpress]
enabled = true
url = "${WORDPRESS_URL}"
username = "${WORDPRESS_USERNAME}"
password = "${WORDPRESS_PASSWORD}"
timeout_seconds = 30

[handlers.wordpress.rate_limit]
enabled = true
requests_per_second = 10
burst_size = 20

```

## 🔗 関連ドキュメント

- [アーキテクチャガイド](./architecture.md)
- [WordPress統合ガイド](./wordpress-guide.md)
- [セキュリティガイド](./security-guide.md)
- [API リファレンス](./api-reference.md)
- [データベース設定ガイド](./database-guide.md)
