# Claude Code統合ガイド

MCP-RSを[Claude Code](https://claude.com/claude-code)に登録し、WordPressのツールを使えるようにする手順です。

Claude CodeもClaude Desktopと同じく、STDIO（標準入出力）でmcp-rsを起動して通信します。サーバー側の設定は[Claude Desktop統合ガイド](./CLAUDE_DESKTOP_INTEGRATION.md)と共通で、違うのは登録方法だけです。

## 前提

- **mcp-rsの実行ファイル**: GitHub Releaseに実行ファイルは添付していないため、ソースからビルドします

  ```bash
  git clone https://github.com/n-takatsu/mcp-rs.git
  cd mcp-rs
  cargo build --release
  ```

  実行ファイルは`target/release/mcp-rs`（Windowsは`target\release\mcp-rs.exe`）にできます。

- **WordPress**: HTTPSで公開されているサイトと、[Application Password](https://make.wordpress.org/core/2020/11/05/application-passwords-integration-guide/)が必要です。mcp-rsはHTTPSの接続先しか受け付けないため、HTTPのローカル環境（開発用のWordPress等）にはそのままでは接続できません。

## 設定ファイル

STDIO用の設定ファイル`configs/production/claude-desktop.toml`を使います（Claude Code用にも、このまま使えます）。

STDIOでは標準出力をJSON-RPCの通信専用に使います。標準出力にログが混ざると通信が壊れるため、この設定ではログをファイルにのみ出力します。

WordPressの接続情報は、設定ファイル内で環境変数として参照しています。

```toml
[transport]
type = "stdio"

[handlers.wordpress]
enabled = true
url = "${WORDPRESS_URL}"
username = "${WORDPRESS_USERNAME}"
password = "${WORDPRESS_PASSWORD}"
```

この環境変数は、次のClaude Codeへの登録で渡します。

## 登録方法1: `claude mcp add`

**macOS / Linux:**

```bash
claude mcp add mcp-rs --scope user \
  -e WORDPRESS_URL=https://your-wordpress-site.com \
  -e WORDPRESS_USERNAME=your_username \
  -e WORDPRESS_PASSWORD="your application password" \
  -- /path/to/mcp-rs/target/release/mcp-rs --config /path/to/mcp-rs/configs/production/claude-desktop.toml
```

**Windows（PowerShell）:**

```powershell
claude mcp add mcp-rs --scope user `
  -e WORDPRESS_URL=https://your-wordpress-site.com `
  -e WORDPRESS_USERNAME=your_username `
  -e WORDPRESS_PASSWORD="your application password" `
  -- C:\path\to\mcp-rs\target\release\mcp-rs.exe --config C:\path\to\mcp-rs\configs\production\claude-desktop.toml
```

- `--`より後ろが、mcp-rsの起動コマンドです。`--`を省くと、mcp-rsの`--config`がClaude Codeのオプションとして解釈されてしまいます
- 実行ファイルと設定ファイルは**絶対パス**で指定してください。Claude Codeがmcp-rsを起動するときの作業ディレクトリは、実行する場所によって変わります
- Application Passwordには空白が含まれるため、引用符で囲みます

### 登録範囲（`--scope`）

| 範囲 | 使える人・場所 | 保存先 |
|---|---|---|
| `local`（既定） | 自分だけ、現在のプロジェクトのみ | ユーザーごとのClaude Codeの設定 |
| `user` | 自分だけ、すべてのプロジェクト | ユーザーごとのClaude Codeの設定 |
| `project` | プロジェクトの全員 | プロジェクト直下の`.mcp.json`（リポジトリで共有） |

> ⚠️ **認証情報の扱い**: `-e`で渡した値は、Claude Codeの設定に平文で保存されます。特に`--scope project`では`.mcp.json`に書き込まれ、リポジトリにコミットされる危険があります。プロジェクトで共有する場合は、次の「登録方法2」のように環境変数を参照してください。

## 登録方法2: `.mcp.json`（プロジェクトで共有する場合）

プロジェクト直下に`.mcp.json`を置くと、そのプロジェクトでClaude Codeを使う全員にmcp-rsが登録されます。認証情報は`${...}`で環境変数を参照し、ファイルには書かないでください。

```json
{
  "mcpServers": {
    "mcp-rs": {
      "command": "/path/to/mcp-rs/target/release/mcp-rs",
      "args": ["--config", "/path/to/mcp-rs/configs/production/claude-desktop.toml"],
      "env": {
        "WORDPRESS_URL": "${WORDPRESS_URL}",
        "WORDPRESS_USERNAME": "${WORDPRESS_USERNAME}",
        "WORDPRESS_PASSWORD": "${WORDPRESS_PASSWORD}"
      }
    }
  }
}
```

- `${WORDPRESS_PASSWORD}`等は、Claude Codeを起動した環境の環境変数から展開されます。各自が自分の環境で設定してください
- `.mcp.json`のサーバーは、初めて使うときにClaude Codeが承認を求めます

## 認証情報を暗号化する場合

v0.18.0以降は、設定ファイルにApplication Passwordを暗号化して保存できます（`encrypted_credentials`）。この場合、Claude Codeに渡すのは復号用のマスターパスワード（`WORDPRESS_MASTER_PASSWORD`）だけになります。

```bash
claude mcp add mcp-rs --scope user \
  -e WORDPRESS_MASTER_PASSWORD="your master password" \
  -- /path/to/mcp-rs/target/release/mcp-rs --config /path/to/your-config.toml
```

暗号化した値は、`mcp-rs encrypt-wordpress-credentials`（v0.18.1以降）で作れます。ユーザー名・Application Password・マスターパスワードを入力すると、設定ファイルに貼り付ける`[handlers.wordpress.encrypted_credentials]`のセクションが出力されます（パスワードは入力中も表示されません）。詳しくは[Security](https://n-takatsu.github.io/mcp-rs/docs/security/#encrypted-wordpress-credentials)を参照してください。

## 接続の確認

```bash
# 登録済みのMCPサーバーと接続状態の一覧
claude mcp list

# 登録内容の詳細
claude mcp get mcp-rs
```

Claude Codeの中では`/mcp`で接続状態を確認できます。接続できていれば、Claude Codeに「WordPressのヘルスチェックをして」等と頼むと、`wordpress_health_check`ツールが呼び出されます。

登録を削除するときは`claude mcp remove mcp-rs`を実行します（`--scope`を指定して登録した場合は、同じ範囲を指定します）。

## トラブルシューティング

### 接続できない（`claude mcp list`で失敗と表示される）

1. **パス**: 実行ファイルと設定ファイルを絶対パスで指定しているか確認してください
2. **設定ファイル**: `[transport]`が`stdio`で、ログを標準出力に出す設定になっていないか確認してください（[Claude Desktop統合ガイド](./CLAUDE_DESKTOP_INTEGRATION.md)の「問題のある設定例」参照）
3. **手動で起動**: 登録したコマンドをターミナルでそのまま実行し、起動時のエラーを確認してください。正常に起動すると、入力を待って止まります（`Ctrl+C`で終了）

### WordPressのツールが表示されない・使えない

1. **環境変数**: `WORDPRESS_URL`・`WORDPRESS_USERNAME`・`WORDPRESS_PASSWORD`を渡しているか確認してください。設定ファイルの`${...}`が展開できないと、その値は`[ERROR:変数名]`という文字列に置き換わります。`WORDPRESS_URL`の場合は接続先が不正になって起動自体が失敗し、`WORDPRESS_USERNAME`・`WORDPRESS_PASSWORD`の場合は起動はしますがWordPressの認証で失敗します
2. **HTTPS**: `WORDPRESS_URL`が`https://`で始まっているか確認してください
3. **ヘルスチェック**: `wordpress_health_check`の結果で、接続・認証・権限のどこで失敗しているかを確認してください

## 🔗 関連ドキュメント

- [Claude Desktop統合ガイド](./CLAUDE_DESKTOP_INTEGRATION.md)
- [WordPress Integration（Pages）](https://n-takatsu.github.io/mcp-rs/docs/wordpress/)
- [Security（Pages）](https://n-takatsu.github.io/mcp-rs/docs/security/)
