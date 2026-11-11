# Claude Desktop統合ガイド

mcp-rsをClaude Desktopから使うための設定と、うまく動かないときの確かめ方です。

## しくみ

Claude Desktopは、`claude_desktop_config.json`に書いたコマンドでmcp-rsを起動し、標準入出力（STDIO）でMCPのメッセージをやり取りします。STDIOモードのmcp-rsは、標準出力にMCPのメッセージだけを書き、ログはファイル（と標準エラー）に書くので、ログの設定で通信が壊れることはありません。

## 1. mcp-rsの設定ファイル

```toml
[transport]
type = "stdio"

[server]
log_level = "warn"

[handlers.wordpress]
enabled = true
url = "https://your-wordpress-site.com"
username = "your_username"
password = "${WORDPRESS_PASSWORD}"
```

- `${WORDPRESS_PASSWORD}`は、起動したときの環境変数から読みます（下の`env`で渡します）。パスワードを暗号化して書く方法は[Security](https://n-takatsu.github.io/mcp-rs/docs/security/#encrypted-wordpress-credentials)を参照してください
- データベース（`[database]`）・プラグイン（`[[plugins]]`）も同じファイルに書きます。[Database](https://n-takatsu.github.io/mcp-rs/docs/database/)・[Plugins](https://n-takatsu.github.io/mcp-rs/docs/plugins/)を参照してください

## 2. Claude Desktopの設定ファイル

`claude_desktop_config.json`（Claude Desktopの「設定」→「開発者」→「設定を編集」）に、mcp-rsを絶対パスで登録します。

**Windows:**

```json
{
  "mcpServers": {
    "mcp-rs": {
      "command": "C:\\Program Files\\MCP-rs\\mcp-rs.exe",
      "args": ["--config", "C:\\Users\\you\\mcp-rs\\mcp-config.toml"],
      "env": {
        "WORDPRESS_PASSWORD": "your application password"
      }
    }
  }
}
```

**macOS・Linux:**

```json
{
  "mcpServers": {
    "mcp-rs": {
      "command": "/path/to/mcp-rs",
      "args": ["--config", "/path/to/mcp-config.toml"],
      "env": {
        "WORDPRESS_PASSWORD": "your application password"
      }
    }
  }
}
```

> ⚠️ Claude Desktopが起動するmcp-rsは、OSのユーザーの環境変数を受け継ぐとは限りません。`${...}`で読む値は、`env`に書いてください。WordPressの`${...}`の値が無いと、mcp-rsは起動せず、Claude Desktopには「Server disconnected」と出ます（`[database]`の値が無い場合は、データベースのツールだけを出さずに起動します）。

設定を変えたら、Claude Desktopを終了して起動し直します。

## 3. 確かめる

Claude Desktopに「mcp_rs_statusを呼んで」と頼むと、次のことが分かります。

- mcp-rsのバージョンと、使っている設定ファイル（`--config`で渡したもの）
- ログと監査ログのファイルの場所
- 動いているハンドラー（WordPress・データベース）と、起動しなかったものとその理由

## トラブルシューティング

### 「Server disconnected」と出る

mcp-rsが起動直後に終了しています。理由は標準エラーに出るので、Claude Desktopのログ（`mcp-server-mcp-rs.log`。「設定」→「開発者」からフォルダを開けます）か、mcp-rsのログで確かめます。よくある理由:

- `${...}`の環境変数が無い: `env`に書いたか確かめる
- `--config`のパスが違う、パスの無い`--config`、知らないオプション: `args`を確かめる
- 設定ファイルの書き方の誤り: エラーの文に、どのキーかが出ます

### ログのファイルの場所

mcp-rsは、次のうち書き込める最初のフォルダにログを書きます。

1. mcp-rsの実行ファイルのフォルダの`logs`
2. ユーザーのフォルダ: Windowsは`%USERPROFILE%\.mcp-rs\logs`、それ以外は`$XDG_STATE_HOME/mcp-rs/logs`か`~/.local/state/mcp-rs/logs`。`C:\Program Files\MCP-rs`にインストールした場合は、常にここ
3. 今のフォルダの`logs`、一時フォルダの`mcp-rs/logs`

ファイル名は`mcp-rs.log.2026-11-09`のように日付で終わります。`mcp_rs_status`でも確かめられます。

### ログが2重に見える

Claude Desktopは、1つの登録に対してmcp-rsを2つ起動することがあります。Claude Desktopの動きで、設定の問題ではありません。各行の`[pid …]`で、どちらのプロセスかが分かります。

### WordPressのツールが使えない

- `mcp_rs_status`で、WordPressが起動しなかった理由を確かめる
- Application Passwordが、そのユーザーで有効かを確かめる（WordPressの「ユーザー」→「プロフィール」）
- WordPressのツールの`wordpress_health_check`で、接続・認証・権限を確かめる

## 関連ドキュメント

- [Claude Code統合ガイド](./CLAUDE_CODE_INTEGRATION.md)
- [WordPress](https://n-takatsu.github.io/mcp-rs/docs/wordpress/)
- [Security](https://n-takatsu.github.io/mcp-rs/docs/security/)
- [API Reference](https://n-takatsu.github.io/mcp-rs/docs/api/)
