# Claude Desktop統合ガイド

mcp-rsをClaude Desktopから使うための設定と、うまく動かないときの確かめ方です。

## しくみ

Claude Desktopは、`claude_desktop_config.json`に書いたコマンドでmcp-rsを起動し、標準入出力（STDIO）でMCPのメッセージをやり取りします。STDIOモードのmcp-rsは、標準出力にMCPのメッセージだけを書き、ログはファイル（と標準エラー）に書くので、ログの設定で通信が壊れることはありません。

## 1. 資格情報を、OSの資格情報ストアに登録する

WordPressのユーザー名とアプリケーションパスワードは、OSの資格情報ストア（Windowsの資格情報マネージャー、macOSのキーチェーン、LinuxのSecret Service）に置くのがおすすめです（v0.24.0以降）。環境変数を使わないので、Claude Desktop・Claude Code・ターミナルのどれから起動しても、同じ1か所から読みます。パスワードを変えたときも、直すのは1か所です。

ターミナル（PowerShell等）で、名前（例: サイトのホスト名）を付けて登録します。ユーザー名とアプリケーションパスワードを聞かれます（パスワードは画面に出ません）。

```powershell
& "C:\Program Files\MCP-rs\mcp-rs.exe" wordpress-keyring set example.com
```

- パスワードを変えるときは、もう一度`set`します。`show`で登録したユーザー名を、`delete`で削除できます
- Windowsでは、資格情報マネージャーの「Windows資格情報」→「汎用資格情報」に`example.com.mcp-rs`として出ます

## 2. mcp-rsの設定ファイル

設定ファイルは、例えば`%APPDATA%\Claude\mcp-rs\mcp-config.toml`に置きます。

```toml
[transport]
type = "stdio"

[server]
log_level = "warn"

[handlers.wordpress]
enabled = true                  # 必須。無いとWordPressのツールを登録しない（エラーにもならない）
url = "https://example.com"
keyring = "example.com"         # 1. で登録した名前
```

- `keyring`を書いたときは、`username`・`password`・`encrypted_credentials`は書きません（一緒に書くと起動しません）
- 環境変数で渡すこともできます（`password = "${WORDPRESS_PASSWORD}"`）。その場合は、`${...}`で読むすべての変数を、下の`env`に書きます（下の注意を参照）。パスワードを暗号化して書く方法は[Security](https://n-takatsu.github.io/mcp-rs/docs/security/#encrypted-wordpress-credentials)を参照してください
- データベース（`[database]`）・プラグイン（`[[plugins]]`）も同じファイルに書きます。[Database](https://n-takatsu.github.io/mcp-rs/docs/database/)・[Plugins](https://n-takatsu.github.io/mcp-rs/docs/plugins/)を参照してください

## 3. Claude Desktopの設定ファイル

`claude_desktop_config.json`（Claude Desktopの「設定」→「開発者」→「設定を編集」）に、mcp-rsを絶対パスで登録します。資格情報ストアを使うときは、`env`は要りません。

**Windows:** JSONの中では、パスの`\`を`\\`と書くか、`/`で書きます。

```json
{
  "mcpServers": {
    "mcp-rs": {
      "command": "C:/Program Files/MCP-rs/mcp-rs.exe",
      "args": ["--config", "C:/Users/you/AppData/Roaming/Claude/mcp-rs/mcp-config.toml"]
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
      "args": ["--config", "/path/to/mcp-config.toml"]
    }
  }
}
```

> ⚠️ Claude Desktopは、mcp-rsに`PATH`等の決まった環境変数と、`env`に書いたものしか渡しません。OSのユーザー環境変数に設定してあっても、mcp-rsには届きません（Claude Code・ターミナルは、親のプロセスの環境変数をそのまま渡します）。設定ファイルで`${...}`を使うときは、読むすべての変数を`env`に書きます。例えば`configs/production/claude-desktop.toml`は`WORDPRESS_URL`・`WORDPRESS_USERNAME`・`WORDPRESS_PASSWORD`の3つを読むので、3つとも要ります。`env`は値を渡すだけで、どの変数を読むかは設定ファイルの`${...}`で決まります。`--config`を付けると、`WORDPRESS_*`の環境変数を`${...}`を通してしか読みません。WordPressの`${...}`の値が無いと、mcp-rsは起動せず、Claude Desktopには「Server disconnected」と出ます（`[database]`の値が無い場合は、データベースのツールだけを出さずに起動します）。

設定・資格情報・環境変数を変えたら、Claude Desktopを終了して起動し直します。ウィンドウを閉じてもタスクトレイ（メニューバー）に残るので、そこから「終了」します。

## 4. 確かめる

Claude Desktopに「mcp_rs_statusを呼んで」と頼むと、次のことが分かります。

- mcp-rsのバージョンと、使っている設定ファイル（`--config`で渡したもの）
- ログと監査ログのファイルの場所
- 動いているハンドラー（WordPress・データベース）と、起動しなかったものとその理由

## トラブルシューティング

### 「Server disconnected」と出る

mcp-rsが起動直後に終了しています。設定の読み込みで失敗したときは、ログの設定より前なので、mcp-rsのログファイルには何も残りません。理由は標準エラーに出て、Claude Desktopのログにだけ残ります。「設定」→「開発者」で、mcp-rsのログを開いて確かめます（Microsoft Store版は、以前の`%APPDATA%\Claude\logs`には書きません）。よくある理由:

- `handlers.wordpress.keyring: no WordPress credentials named ...`: 資格情報ストアに、その名前で登録されていない。`mcp-rs wordpress-keyring show <名前>`で確かめる
- `environment variable '...' is not set`: `${...}`の環境変数が`env`に無い
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
