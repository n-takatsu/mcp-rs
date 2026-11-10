# mcp-rs Release Notes

## Version History (0.01 Increment Versioning)

Our project follows a detailed 0.01 increment versioning strategy to provide granular tracking of development progress and feature implementation.

## 🧹 v0.20.0 - 使っていないコードの整理

**Release Date:** 2026-11-10
**Focus:** 本体から使われていないコード・機能・依存関係を削除し、実態に合わない説明を直す。設定ファイルの環境変数の扱いを安全にする

### 🧹 使っていないコードの削除（#448）

- 本体（`mcp-rs`バイナリ）とデータベースのハンドラーのどちらからも使われて
  いないモジュールを削除した（約6.1万行）。WordPress・プラグイン・HTTP
  モードの認証等、本体の動作は変わらない
- 使っていない機能（features）と依存関係を削除し、依存するクレートは725個
  から591個に減った。WebSocketのクライアントのTLSをrustlsに揃え、OpenSSLを
  使わなくなった
- 削除した機能（脅威インテリジェンス・LLM・Kubernetes operator等）の
  ドキュメントとHelmチャート等を削除し、README・websiteの説明を実態に
  合わせた。データベースのハンドラーは、まだ本体につながっていない
  ことを明記した（#453）

### 🔒 設定ファイルの環境変数（#454）

- `${VAR}`を展開した値の一部をdebugのログに出していたのをやめ、変数名だけに
  した
- WordPressを有効にした設定で、参照した環境変数が無い場合等は、起動しない
  ようにした。どの項目のどの変数かを出す（値は出さない）

### 📂 ログの出力先（#441）

- `C:\Program Files\MCP-rs`にインストールした場合、ログが
  `%USERPROFILE%\.mcp-rs\logs`に出るのを、フォールバックとして表示しない
  ようにした。出力先は変わらない
- ログのファイル名の日付がUTCであること等を、README・websiteに書いた

### ⚠️ 挙動の変更

- WordPressを有効にした設定で、`${VAR}`の環境変数が無いか、`${`に対応する
  `}`が無いと、起動しない。以前は`[ERROR:VAR]`等の文字列を値にして起動して
  いた
- `--features`で、削除した機能（`websocket`・`security`・`full`等）を指定して
  ビルドしている場合は、外す。既定の機能（`default`）は空になった
- **ライブラリとして使っている場合**: 削除したモジュール（`mcp_rs::McpServer`
  等）を使っていれば、`mcp_rs::mcp::McpServer`等に変える。
  `McpConfig::expand_env_vars`等は`Result`を返す

## 🩺 v0.19.2 - 状態を確かめるツール

**Release Date:** 2026-11-09
**Focus:** AI（Claude Desktop・Claude Code等）から、mcp-rsの状態を確かめられるようにする

### ✅ `mcp_rs_status`ツール（#434）

- mcp-rs自身の状態を返す、読み取り専用のツール。WordPress・プラグインの
  有無にかかわらず、常に使える（ツールの一覧の先頭）
- バージョン・動作モード・プロトコル版・設定ファイル・ログと監査ログの
  書き込み先（ログが切り替わった場合はその理由）・各ハンドラーとプラグインの
  状態（起動できなかった理由を含む）を返す。フォルダの許可が無くても、
  ツールを呼ぶだけで確かめられる
- パスワード・キー・トークン・環境変数の値は含めない。HTTPモードで認証して
  いない呼び出し元には、バージョンだけを返す。呼び出しは監査ログに記録する
- `wordpress_health_check`の出力にも、バージョンの行を加えた

### ⚠️ 挙動の変更

- 知らない引数と、パスの無い`--config`があると、起動しない（#437）。理由と
  使い方を標準エラー出力に出し、終了コード2で終わる。v0.19.1では、ログに
  警告を出して無視していた。Claude Desktop等の設定（`args`）に
  `--config <path>`以外を書いている場合は、起動しなくなるので確かめる
- ツールの一覧（`tools/list`）の先頭に`mcp_rs_status`が加わる。HTTPモードで
  APIキー・トークンごとにツールを制限している場合（`tools`・`scope_tools`）、
  `@read_only`か`mcp_rs_status`を指定すると使える

## 🏷️ v0.19.1 - バージョンの表示

**Release Date:** 2026-11-08
**Focus:** インストールされているバージョンを確かめられるようにする

### ✅ `--version`・`--help`（#431）

- `mcp-rs --version`（`-V`）でバージョン（`mcp-rs 0.19.1`）を、`--help`
  （`-h`）で使い方を表示して終了する。サーバーは起動せず、設定ファイル・
  ログにも触れない。以前は`--version`を無視してサーバーとして起動し、入力を
  待ち続けていた
- `--config=<path>`の書き方も受け付ける

### ⚠️ 挙動の変更

- 知らない引数と、パスの無い`--config`は、ログに警告を出す（これまでどおり
  無視して起動する）。将来のバージョンでは、知らない引数があると起動しない
  ようにする予定。Claude Desktop等の設定（`args`）に`--config <path>`以外の
  引数を書いている場合は、警告が出ないか確かめる

## 🔐 v0.19.0 - HTTPモードの認可・ツールの権限・監査ログ

**Release Date:** 2026-11-08
**Focus:** HTTPモードを組織で安全に使うための機能（OAuth 2.1、ツールの権限、監査ログ）と、MCPのプロトコル版2025-06-18への対応

### ✅ HTTPモードのOAuth 2.1（#420）

- `[transport.http.oauth]`で、外部の認可サーバー（Keycloak・Okta・
  Microsoft Entra ID等）が発行したアクセストークン（JWT）を受け付ける。
  MCPの仕様（2025-06-18の「Authorization」）のリソースサーバーとして動き、
  ログイン（SSO・LDAP/AD・MFA）は認可サーバーが受け持つ
- 署名・発行者・有効期限・対象（`aud`）を確かめる。別のサービス向けの
  トークン、共有鍵（HS256等）・署名なしのトークンは受け付けない
- Protected Resource Metadata（RFC 9728）を公開し、401・403に
  `WWW-Authenticate`を付ける。APIキーと併用できる

### ✅ APIキー・トークンごとのツールの権限（#52）

- APIキーの`tools`、OAuthの`scope_tools`で、使えるツールを指定できる。
  ツール名、`*`を使うパターン（`fs__*`等）、`@read_only`（読み取り専用の
  ツール）が使える。省略時は、これまでどおりすべて使える
- 使えないツールは一覧に出さず、呼び出しは拒否して監査ログに残す

### ✅ 監査ログ（#416、#55）

- 書き込み先を`[audit]`で指定できる。省略時は、設定ファイルと同じフォルダの
  `audit.log`。サイズでのローテーション（既定100MB・5ファイル）を実装した
- ツール呼び出しの記録に、呼び出し元（`api_key:<name>`・`oauth:<client>`・
  STDIOは`local`）、利用者、IPアドレス、所要時間を加えた。HTTPモードの
  認証の失敗も記録する。キー・トークン・ツールの引数は記録しない

### ✅ MCPのプロトコル版2025-06-18（#417）

- クライアントが要求した版（`2025-06-18`・`2024-11-05`）で答える
- ツールのannotations（読み取り専用・破壊的等）、`outputSchema`、
  `structuredContent`を、`2025-06-18`で話すクライアントに渡す。WordPressの
  ツールにもannotationsを付けた

### ✅ プラグインの隔離（#411、#413）

- プラグインを、Dockerコンテナの中（`[plugins.container]`）や、Kubernetesの
  Job（`[plugins.kubernetes]`）として起動できる。既定でネットワークなし・
  読み取り専用のルートファイルシステム・リソースの上限付き

### 🐛 修正

- 監査ログが、mcp-rsの作業フォルダの`audit.log`に書かれていた問題（#416）。
  Claude Desktop等から起動した場合、記録されていなかった可能性が高い
- 監査ログの書き込みが、終わる前にファイルを閉じることがあった問題（#55で発見）
- `HttpTransport::transport_info()`が、待ち受けたアドレスの代わりに設定の
  アドレスを返しえた問題（#265）

### ⚠️ 挙動の変更

- 監査ログの書き込み先が、設定ファイルと同じフォルダの`audit.log`になった。
  起動時のログに書き込み先が出る（`📝 監査ログ: <path>`）。設定ファイルを
  使わずに起動した場合は、ファイルに書かない（`[audit] file_path`に絶対パスを
  指定すれば書く）
- WordPressの読み取りのツールも監査ログに記録する（以前は変更するツールだけ）。
  ログが増えるため、ローテーションの設定を確かめる
- `initialize`で、クライアントが`2025-06-18`を要求すれば`2025-06-18`で答える
  （以前は常に`2024-11-05`）。プラグインには`2025-06-18`を要求する

### 🗑️ 削除

- 本体から使われていなかったコードを削除した: `core`モジュール（#409）、
  `security::auth`のJWT・OAuth2・セッション認証・MFA・`compliance`等（#421）

### 📚 ライブラリとして使う場合

公開APIに互換性の無い変更がある。詳しくはCHANGELOG.mdを参照。

- `mcp::Tool`に項目を追加した（構造体リテラルには`..Default::default()`が要る）
- `transport::http::JsonRpcRequestHandler::handle_jsonrpc`に、呼び出し元
  （`mcp::Caller`）の引数を追加した
- `AuditConfig::default()`はファイルに書かない。`plugin_proxy::start_plugins`は
  監査ログを受け取る
- 上記の削除したモジュール・機能（features）の`mfa`・`mfa-full`

## 🌐 v0.18.4 - HTTPモードの修正

**Release Date:** 2026-11-08
**Focus:** HTTPモードで、MCPのリクエストが処理されていなかった問題の修正

### ✅ HTTPモード（#400）

- HTTPモード（`[transport] type = "http"`）で、MCPのリクエストが処理されず、
  すべて30秒後に`408`になっていた問題を修正。`/mcp`（と`/`）へのPOSTと、
  `enable_websocket_upgrade`が有効なら`/ws`のWebSocketで、STDIOモードと
  同じツール（WordPress・プラグイン）が使える
- 同じ`id`を使う複数のクライアントの応答が入れ替わらないよう、リクエスト
  ごとに直接処理する
- APIキー認証等の受け付け時のチェックは、これまでどおりかかる

### ⚠️ 挙動の変更

- プラグイン（`[[plugins]]`）が、HTTPモードでも起動するようになった
  （v0.18.3ではHTTPモードでは無視していた）。HTTPの待ち受けに届く人は
  誰でもプラグインのツールを呼べるため、自分のマシン以外から届く場合は
  APIキー認証を有効にする
- `[transport] type`が`stdio`・`http`以外（未指定を含む）の場合は、
  STDIOモードで動く（以前は処理しないまま待ち続けていた）

## 🔌 v0.18.3 - プラグイン（段階1）・投稿のスラッグ指定

**Release Date:** 2026-11-08
**Focus:** 別プロセスのMCPサーバーをプラグインとして中継する機能（#392の
段階1）と、投稿・固定ページのスラッグ指定

### ✅ プラグイン（#394、STDIOモードのみ）

- 設定の`[[plugins]]`で指定したコマンドを、別プロセスのMCPサーバーとして
  起動し、そのツールを`<プラグイン名>__<ツール名>`として使えるようにした。
  Node.js等、どの言語で書かれたMCPサーバーでも使える
- プラグインには本体の環境変数を渡さず、`env`で指定したものだけを渡す
  （WordPressの認証情報等は渡らない）。本体側で、レート制限・監査ログ・
  タイムアウト・応答サイズの上限をかける
- 起動できないプラグインは警告を出して飛ばし、本体は起動を続ける。
  異常終了・無応答のプラグインのツールはエラーを返す
- ファイル・ネットワークは隔離しない。信頼できるプラグインにのみ使う
  （コンテナでの隔離は今後、#392）。HTTPモードは#400の修正後に対応する
- 詳しくは`website/docs/plugins.md`

### ⚠️ プラグインを追加するときの注意

- 編集するのは、MCPクライアントの設定で`--config`に指定している設定
  ファイル（Claude デスクトップアプリなら`claude_desktop_config.json`の
  mcp-rsの`args`）。設定ファイルを複数持っている場合、他のファイルを
  編集しても反映されない
- v0.18.1以前のひな形から作った設定ファイルに残っている`[plugins]`
  （`search_paths`等）と`[plugins.*]`は、先に削除する。`[[plugins]]`と
  同じファイルにあるとmcp-rsが起動しない（エラーに、その旨を表示する）。
  プラグインを使わない場合は、残っていても問題ない

### ✅ WordPressツール

- 投稿・固定ページを作成・更新するツールに`slug`引数を追加（#398）。
  日本語のタイトルの記事に、英語のスラッグを付けられる。結果と`get_post`
  には、WordPressが実際に設定したスラッグを表示する

### ✅ MCPプロトコル

- STDIOモードで、登録したすべてのハンドラーのツール・リソースを扱うように
  した（#401）。`initialize`の`serverInfo`は`mcp-rs`と本体のバージョンに
  なった（以前は`mcp-rs-wordpress` 0.1.0）

## 🧹 v0.18.2 - WordPressへの再送の修正・未使用コードの削除

**Release Date:** 2026-11-07
**Focus:** タイムアウト・5xxのときに作成・削除のリクエストを再送していた
問題の修正と、本体に接続されていなかったコードの削除

### ✅ WordPressツール

- WordPressへのリクエストがタイムアウトや5xxで失敗したとき、作成（POST）・
  削除（DELETE）のリクエストを再送しないよう修正（#388）。1回目が処理済み
  だった場合に、`upload_media`でメディアが重複したり、`delete_*`で削除
  できているのに404のエラーになったりしていた。GET・PUTはこれまでどおり
  再送し、接続できなかった場合と429はメソッドを問わず再送する

### ⚠️ 削除したもの

本体に接続されておらず、動いていなかったコードを削除した。mcp-rsを
MCPサーバーとして使う場合の動作は変わらない。ライブラリとして使っている
場合は、以下の公開モジュールが無くなる

- 対話式のセットアップ（`--setup-config`等）と実行時の対話式コントローラー
  （#386）。`mcp_rs::runtime_control`・`config::ConfigSwitcher`を削除。
  設定ファイルは`configs/production/`のものを使う
- Kubernetes連携モジュール`src/k8s/`（`mcp_rs::k8s`）と、対応するCRD
  （#305）。オペレーター（`mcp-operator`）の`MCPServer`等は変更なし
- プラグイン実装（`mcp_rs::plugin`・`plugins`・`plugin_isolation`・
  `docker_runtime`、`docker-runtime`機能、オペレーターのPlugin）と、
  設定の`[plugins]`（#393）。`[plugins]`が設定ファイルに残っていても
  無視される。プラグイン機構は、各プラグインを独立したMCPサーバーとして
  別プロセスで動かす方式で作り直す（#392・#394）

### ✅ その他

- example`wordpress_blog_service_demo`が、Application Passwordの先頭
  4文字を表示していた問題を修正（#386）

## 🛠️ v0.18.1 - WordPressツールの改善・MCPプロトコル準拠の修正

**Release Date:** 2026-11-07
**Focus:** Claude Code・Claude デスクトップアプリから実機（redring.jp）で
使って見つかったWordPressツールの不具合修正と使い勝手の改善、MCPの
通知の扱いの修正

### ⚠️ 挙動の変更

- `create_advanced_post`・`create_post_with_embeds`は、`status`を指定
  しない場合に下書きを作成するようになった（#367）。公開するには
  `status: "publish"`を指定する。ライブラリの`PostCreateParams`の
  `Default`も`draft`になった
- `get_post`は本文を全文返し、既定では編集用の原文（Gutenbergのブロック
  区切りを含む）を返すようになった（#356）。以前の表示に近い形が必要な
  場合は`format: "text"`を指定する

### ✅ WordPressツール

- `upload_media`に`file_path`引数を追加（#380）。手元のファイルを
  指定してアップロードでき、AIがbase64を書き出す必要がなくなった。
  既定では無効で、`upload_allowed_dirs`で許可したフォルダの中だけを読む
- `get_post`にアイキャッチ画像（メディアIDとURL）を表示（#378）
- `delete_media`を`force: true`で呼ぶと、削除に成功しているのに404の
  エラーを返していた問題を修正（#383）。成功の応答を解析できないときに
  同じリクエストを再送していた。再送そのものもやめ、501は再送しない
- `create_post_with_featured_image`が常に失敗していた問題を修正（#367）

### ✅ 設定・認証情報

- `mcp-rs encrypt-wordpress-credentials`サブコマンドを追加（#355）。
  `encrypted_credentials`のセクションを生成する。パスワードは入力中も
  表示せず、コマンドライン引数でも受け付けない
- `--config`を省略して起動したときの設定読み込みの問題を修正（#375）。
  標準出力への出力でSTDIOの通信を壊すおそれ、`~`が展開されない、
  `[transport] type`が読み込めない、の3点

### ✅ MCPプロトコル

- STDIOサーバーが、JSON-RPCの通知にエラー応答を返していた問題を修正
  （#360）
- HTTP transportが、通知に本文付きの応答を返していた問題を修正（#374）。
  本文なしの`202 Accepted`を返す

### ✅ ビルド

- 開発用ビルドのデバッグ情報を行番号のみにし、`target`の肥大化（約70GB）
  を解消（#371）
- 将来非互換警告（`proc-macro-error2`）を解消（#369）

## 🔒 v0.18.0 - セキュリティ強化

**Release Date:** 2026-11-06
**Focus:** セキュリティ観点での不足点の洗い出し（Epic #341ほか）で見つかった、
実装済みだが実際のリクエストパスに配線されていなかったセキュリティ機能の
配線と、依存関係の脆弱性対応

### ⚠️ 挙動の変更

- `upload_media`は許可リストにある形式（画像・PDF・動画・音声）のみを
  受け付けるようになった。SVG・テキスト・Office文書（docx等）・movは
  拒否される。また拡張子がMIMEタイプと一致し、内容がその形式のマジック
  ナンバーを持つ必要がある
- WordPressを有効（`enabled = true`）にしているのに認証情報が無い設定は、
  起動時にエラーになる

### ✅ 認証・アクセス制御

- HTTP/WebSocket transportにAPIキー認証を追加（#342）。それまでは
  リスナーに到達できる任意のクライアントが、認証なしで`delete_post`等の
  変更系ツールを呼び出せた。`auth_enabled = true`で有効化し、
  `Authorization: Bearer <key>`または`X-Api-Key`でキーを送る。失効・
  期限切れのキーも拒否する。既定は無効で、STDIOは対象外
- WordPressツール呼び出しにレート制限を適用（#343）。`RateLimiter`は
  保持されていたが一度も参照されていなかった

### ✅ 監査ログ

- WordPressの変更系ツール（`create_*`/`update_*`/`delete_*`/
  `upload_*`/`set_*`）の呼び出しを、成功・失敗を問わず監査ログに記録
  （#344）。入力検証やレート制限で拒否された呼び出しも記録する
- 監査ログのファイル出力が追記ではなく毎回上書きになっており、最新の
  1件しか残っていなかった既存の不具合を修正

### ✅ 認証情報の保護

- 設定ファイルの`encrypted_credentials`を実際に復号して使うよう配線
  （#338）。復号鍵（マスターパスワード）は`WORDPRESS_MASTER_PASSWORD`
  環境変数からのみ読み込む。3つの設定読み込みパス（起動時・
  `--config`指定時・動的リロード）すべてに対応
- `WordPressConfig`/`WordPressHandler`のDebug出力でパスワードを伏せる

### ✅ メディアアップロードの検証

- `upload_media`にMIME許可リスト・拡張子とMIMEの一致・マジックナンバー
  による内容検証・ファイル名の検証・サイズ上限（10 MiB）を追加（#339）。
  WordPressは保存形式を拡張子で決めるため、拡張子を検証しないと画像と
  称してHTMLを保存させる格納型XSSが成立していた

### ✅ 依存関係の脆弱性

- v0.17.3に含まれていた既知の脆弱性10件のうち9件を依存関係の更新で
  解消（TLSの証明書検証に関わるrustls-webpki・webpki-rootsを含む）。
  修正版のないRUSTSEC-2026-0387（crypto-bigint）は、jsonwebtoken 11.x
  への破壊的な移行が必要なため理由を付けて無視
- `cargo audit`の日次定期実行を追加（#340）。それまでの定期実行は結果を
  握りつぶしており、脆弱性を含むブランチでも成功扱いになっていた。
  develop/mainを監査し、検知時はIssueを自動作成する

## 🛠️ v0.17.3 - WordPress実機バグ修正・ログ出力堅牢性向上

**Release Date:** 2026-10-13
**Focus:** Claude Desktop経由での実機（redring.jp、WordPress + AFFINGER）に
対する継続的な実機テストで見つかったWordPressハンドラーのバグ修正と、
ログ出力まわりの堅牢性向上

### ✅ WordPressハンドラー修正

- 一覧系（`get_posts`/`get_pages`/`get_categories`/`get_tags`/
  `get_comments`/`get_media`）がWordPressの既定`per_page=10`で頭打ちに
  なっていた問題を修正し、全ページ取得するように変更（`get_media`は
  `X-WP-Total`も併せて表示）
- WordPressのエラーレスポンス本文（`code`/`message`）を診断に活用する
  よう修正（以前はHTTPステータスのみで原因不明だった）
- `get_post`/`update_post`/`delete_post`が`/wp/v2/posts/{id}`しか
  叩いておらず、固定ページの読み取り・更新・削除ができなかった問題を
  `post_type`パラメーター追加で修正
- `create_post`系の既定ステータスを`publish`から`draft`に変更し、
  意図しない公開を防止
- 日本語slugの表示がパーセントエンコードされたままだった問題を修正
- 投稿本文のHTMLタグをプレーンテキスト化してから表示するよう変更
- `create_category`/`update_category`に`slug`/`parent`、
  `create_tag`/`update_tag`に`slug`パラメーターを追加
- `upload_media`を`multipart/form-data`方式に変更。生バイナリ+
  `Content-Disposition`方式では非ASCIIファイル名（日本語等）が
  `_`に置換され`unnamed-file`扱いになっていた問題を解消
- `tools/call`の失敗（WordPress APIエラー等）がJSON-RPCレベルの
  エラーとして返っていたのを、MCP仕様に沿った
  `CallToolResult{isError: true, content}`形式に修正

### ✅ ログ出力の堅牢性向上

- ログディレクトリが存在するが書き込み不可な場合（例:
  `C:\Program Files\...`配下に非昇格プロセスから書き込む場合）に
  `rolling::daily()`等の内部`.expect()`でサーバー全体がパニックして
  起動できなくなっていた問題を修正。フォールブルな`Builder::build()`
  に切り替え、失敗時はコンソール出力のみへ縮退するよう変更
- ログ出力先を、設定/既定のディレクトリ→`%USERPROFILE%\.mcp-rs\logs`
  （Windows）→`./logs`→システムテンプディレクトリの順に、実際の
  書き込み＋readback確認込みで解決するフォールバックチェーンを追加
  （`%LOCALAPPDATA%`はMSIXパッケージのClaude Desktopからの子プロセスで
  パス仮想化されうるため避けた）
- `non_blocking()`の書き込みガードが即座にdropされ、起動直後に
  バックグラウンドの書き込みスレッドが終了し、以後のログがほぼ
  すべて実際にはファイルへ届かなくなっていた重大な既存バグを修正
- 起動時に解決したログ出力先（フォールバック理由含む）を常にstderrへ
  告知し、`wordpress_health_check`にもファイルサイズ・最終書き込み
  時刻込みで表示するよう変更
- ファイル出力にANSIエスケープコードが混入していた問題を修正
- 複数のmcp-rsプロセスが同一ログファイルへ同時に書き込む場合に備え、
  各行に`[pid N]`を付与するカスタムフォーマッターを追加
- タイムスタンプをUTCからミリ秒精度・タイムゾーン付きのローカル時刻
  表記に統一
- `main.rs`のバージョン表記ハードコードを`env!("CARGO_PKG_VERSION")`
  に統一

## 🛠️ v0.17.2 - Claude Desktop起動不具合修正

**Release Date:** 2026-10-09
**Focus:** Claude Desktop（STDIO transport）から起動してもMCPサーバーが
リクエストを処理しなかった問題の修正

### ✅ 修正内容

- `Runtime::initialize()`がtransportを起動するだけで、実際にそれを
  読み続ける`Runtime::run()`を一度も呼んでいなかったため、通常起動
  パスではどのtransportでもリクエストが一切処理されていなかった。
  STDIO設定時は独立して動作する`McpServer::run_stdio()`
  （改行区切りJSON-RPCの読み取り・ディスパッチループ）へ
  ルーティングするよう変更
- `configs/production/claude-desktop.toml`等が`type = "stdio"`と
  書いているが`TransportConfig.transport_type`フィールドと一致せず
  黙って`None`にデシリアライズされ、上記の新しい分岐が発火しなかった
  問題を`#[serde(alias = "type")]`追加で修正
- 回帰テスト`test_transport_type_accepts_type_alias`を追加

## 🛠️ v0.17.1 - デプロイCI修正

**Release Date:** 2026-10-03
**Focus:** v0.17.0タグpush時にDockerイメージの公開が失敗していた問題の修正

### ✅ 修正内容

- `deploy.yml`の`docker/metadata-action`が、タグpush時に`{{branch}}`が
  空文字列になることで不正なDockerタグ（`ghcr.io/n-takatsu/mcp-rs:-<sha>`）
  を生成し、Dockerイメージのビルド・公開が失敗していた問題を修正
- 該当するSHAタグパターンをブランチrefの場合のみ有効化

## 🔐 v0.17.0 - エンタープライズセキュリティ強化完成 (Epic #219)

**Release Date:** 2026-10-03
**Focus:** mTLS・ゼロトラスト・HSM・IDS/IPS・量子耐性暗号・GDPR/CCPA対応まで、
多層防御のセキュリティ強化計画（Security Enhancement Phase 3）が全フェーズ完了

### ✅ Phase 3.1: Critical Security

1. **mTLS（相互TLS認証）** (#221, #254)
   - HTTPトランスポートへの完全実装
   - クライアント証明書なしのリクエストを拒否
   - 有効なCA署名証明書を受理
2. **ネットワークポリシーとローカル実行制限** (#220, #256)
   - bind時点でのローカル限定ポリシー適用
3. **なりすまし防止・リプレイ攻撃対策** (#222, #259)
   - HTTP / WebSocket / プラグインブローカーへの統合
4. **TLS/SSL強制とHSTS** (#224, #261)
   - HTTPトランスポートおよびWebSocket(WSS)の両方に強制適用

### ✅ Phase 3.2: High Priority Security

1. **カラムレベル暗号化 + RBAC統合** (#223, #260, #308)
   - `DatabaseHandler`のクエリパスへの実配線
   - 関連するビルディングブロックのバグ修正
2. **プラグイン間E2E暗号化の鍵交換フロー** (#225)
   - X25519 ECDH + HKDF-SHA256
   - Perfect Forward Secrecy・自動鍵ローテーション・グレースピリオド対応
3. **堅牢化修正**
   - `CommunicationBroker`のメッセージ配送不具合
   - Redisの`DEL`コマンドを既定でブロック
   - SQLiteトランザクション分離の修正
   - KubernetesのNetworkPolicyを実CNI下で検証・調整
   - プラグインネットワーク隔離の修正

### ✅ Phase 3.3: Advanced Security

1. **ゼロトラスト・マイクロセグメンテーション** (#226)
   - DBクエリ実行パスへの実配線
2. **HSM統合（PKCS#11）** (#227)
   - カラム暗号化への追加
3. **IDS/IPS** (#228)
   - 既存IDSをHTTPトランスポート層へ実配線
   - IPSブロックを実装

### ✅ Phase 3.4: Future Security（スコープ縮小の上で実装）

1. **量子耐性暗号（ML-KEM-768ハイブリッド鍵交換）** (#230)
   - プラグイン間E2E暗号化にX25519とのハイブリッドKEMコンバイナーとして追加
   - NIST FIPS 203準拠、RustCryptoファミリーの`ml-kem`クレートを採用
2. **GDPR/CCPAコンプライアンス自動化** (#229)
   - mcp-rs自身が保持するアカウントデータ（APIキー・セッション・監査ログ）に
     対する削除権・アクセス権として実装
   - 外部バックエンドDBのデータはスコープ外（データ所有側の責務）

### ✅ その他の統合・改善

- 脅威インテリジェンス統合 (#211)
- Dockerランタイム統合の完成 (#212)
- WebSocket Transport基盤の実装 (#213)
- PostgreSQL最適化 Phase 2 (#214)
- パフォーマンス最適化提案エンジン (#177)
- Analyticsモジュールのリファクタリング
- `cargo clippy --all-targets`警告の解消、WordPress関連の各種修正

## 🚀 v0.16.0 - WebSocket通信強化とプラグイン分離完成

**Release Date:** 2025-12-22  
**Focus:** WebSocket機能の大幅拡張とプラグイン分離システムの完成

### ✅ WebSocket Transport 大規模機能追加 (Issues #193-197)

#### 🌐 WebSocket Server & LLM Integration

**5つの主要機能を実装**:

1. **WebSocket Server Mode** (#197)
   - 双方向WebSocket通信のサーバーモード実装
   - 接続管理とクライアントトラッキング
   - Ping/Pongによるヘルスチェック機能
   - タイムアウト管理と自動切断

2. **LLM Streaming Integration** (#196)
   - **OpenAI GPT-4**: ストリーミングAPI完全対応
   - **Anthropic Claude 3.5 Sonnet**: リアルタイム応答生成
   - トークン使用量の追跡と統計情報
   - エラーハンドリングと自動再試行
   - 3つの実装例:
     - `websocket_echo_server.rs`: 210行（基本エコーサーバー）
     - `websocket_llm_chat.rs`: 204行（LLMチャット）
     - `websocket_load_balanced.rs`: 264行（負荷分散）

3. **Connection Pool & Load Balancing** (#195)
   - **3つの負荷分散アルゴリズム**:
     - RoundRobin: 順次分散
     - LeastConnections: 最小接続数優先
     - Random: ランダム分散
   - ヘルスチェックと自動フェイルオーバー
   - 接続プール管理と再利用
   - 詳細な統計情報とメトリクス

4. **Metrics, Rate Limiting & Compression** (#194)
   - **リアルタイムメトリクス**:
     - メッセージ数、バイト数、レイテンシ
     - 成功率、エラー率の追跡
   - **3種類のレート制限**:
     - TokenBucket: バースト対応
     - LeakyBucket: 均一な処理速度
     - SlidingWindow: 時間窓ベース
   - **メッセージ圧縮**: gzip/deflateで帯域幅削減

5. **Tests, Benchmarks & Documentation** (#193)
   - **統合テスト**: 224行の包括的テスト
   - **ベンチマーク**: 294行の性能測定コード
   - **詳細ドキュメント**: 3つの大型ガイド
     - `websocket-guide.md`: 508行
     - `websocket-performance.md`: 614行
     - `llm-integration-guide.md`: 631行

#### 📊 統計

- **新規ファイル**: 10個（examples 3個、tests 1個、benches 1個、docs 3個、実装2個）
- **追加コード行数**: 約2,950行
- **ドキュメント**: 1,753行の詳細ガイド
- **テスト**: 224テストケース（100% pass）

### ✅ Plugin Isolation System 完成 (Issue #190)

#### 🔌 プラグイン間通信とエラーハンドリング

**主要機能**:

1. **Inter-Plugin Communication** (`inter_plugin_comm.rs`)
   - メッセージベースの通信システム
   - Pub/Subパターンによるイベント配信
   - 型安全なメッセージングAPI
   - 581行の実装

2. **Advanced Error Handling** (`error_handler.rs`)
   - 包括的エラー分類と復旧戦略
   - エラーコンテキストの伝播
   - 自動リトライとサーキットブレーカー
   - 636行の堅牢な実装

3. **Enhanced Monitoring**
   - プラグイン稼働状態の詳細監視
   - リソース使用量トラッキング
   - パフォーマンスメトリクスとアラート

4. **Docker Runtime Support** (#185)
   - Dockerコンテナでのプラグイン実行
   - コンテナライフサイクル管理
   - セキュリティスキャンとコンプライアンス
   - 統合テスト: 270行（100% pass）

#### 📚 プラグイン開発者向けドキュメント

**4つの包括的ガイド**:

1. `docker-runtime-guide.md`: 454行（実装詳細）
2. `plugin-developer-guide.md`: 352行（開発者ガイド）
3. `plugin-security-guide.md`: 403行（セキュリティ）
4. `plugin-troubleshooting-guide.md`: 504行（トラブルシューティング）

**合計**: 1,713行の開発者サポートドキュメント

### 🧪 品質保証

- **全567テスト合格** (100% pass rate)
  - ライブラリテスト: 567/567 ✅
  - WebSocket統合テスト: 224ケース ✅
  - Docker Runtime統合テスト: 270行 ✅
  - プラグイン分離テスト: 432行 ✅
- **Clippy警告ゼロ**: 全モジュールでクリーン
- **フォーマットチェック通過**: cargo fmt準拠
- **ベンチマーク**: WebSocket、プラグイン分離の性能測定完備

### 📦 互換性

- **破壊的変更なし**: 完全な後方互換性を維持
- **Axum 0.8対応**: 最新フレームワークに完全対応
- **既存機能の保持**: 全ての既存機能が正常動作

### 🎯 主な改善点

1. **WebSocket通信の実用性向上**
   - LLMとの統合によりAIアプリケーション開発が容易に
   - 負荷分散とフェイルオーバーでエンタープライズ対応
   - メトリクスとレート制限で運用品質向上

2. **プラグインシステムの成熟**
   - プラグイン間通信により複雑なワークフロー実現
   - エラーハンドリングで安定性向上
   - Docker統合でデプロイメントが柔軟に

3. **開発者体験の向上**
   - 3,466行のドキュメント追加
   - 実践的な実装例3つ
   - トラブルシューティングガイド完備

### 📈 プロジェクト規模

- **総コード行数**: 約70,000行以上
- **ドキュメント**: 40,000行以上
- **テストカバレッジ**: 567テスト（100% pass）
- **ベンチマーク**: 15カテゴリ

---

## 🚀 v0.17.0 - Code Architecture Refactoring

**Release Date:** 2025-12-15
**Focus:** mod.rsアンチパターン解消による保守性・可読性の向上

### ✅ Major Refactoring (Issues #162-166)

#### 🏗️ Module Structure Improvements

**5つの主要モジュールを責務別に分割**:

1. **Analytics Modules** (#162)
   - `analytics/anomaly`: 302行 → 4ファイル (types, detector, realtime, mod)
   - `analytics/prediction`: 259行 → 4ファイル (types, predictor, trend, mod)

2. **Operator Module** (#163)
   - `operator`: 312行 → 6ファイル (types, resources, mcpserver, plugin, security, mod)

3. **Plugin Isolation Module** (#164)
   - `plugin_isolation`: 560行 → 5ファイル (types, config, manager, health, mod)
   - 最大のmod.rsファイルを分割

4. **Security IDS Module** (#165)
   - `security/ids`: 544行 → 4ファイル (types, config, detector, mod)

5. **Transport Module** (#166)
   - `transport`: 260行 → 6ファイル (types, transport_trait, error, config, factory, mod)

#### 📊 統計

- **削除**: 1,351行の巨大mod.rsファイル
- **追加**: 1,453行の明確に分離されたモジュール
- **新規ファイル**: 13個
- **変更ファイル**: 20個

#### ✨ メリット

- **Single Responsibility**: 各ファイルが1つの責務を持つ
- **Maintainability**: 型定義、設定、実装の明確な分離
- **Testability**: テストがより独立し理解しやすく
- **Navigation**: IDE内でのコードナビゲーション改善
- **Modularity**: 再利用性と拡張性の向上

#### 🧪 品質保証

- **全377テスト合格** (100% pass rate)
- **Clippy警告ゼロ**: 全モジュールでクリーン
- **フォーマットチェック通過**: cargo fmt準拠
- **Public API不変**: 破壊的変更なし

#### 📦 関連PR

- PR #168: Operator module refactoring
- PR #169: Plugin isolation module refactoring
- PR #170: Security IDS module refactoring
- PR #171: Transport module refactoring
- PR #172: Analytics modules refactoring (to develop)

---

## 🚀 v0.16.0 - PostgreSQL Phase 2 完成

**Release Date:** 2025-11-23
**Focus:** PostgreSQL統合とマルチデータベース対応の完全実装

### ✅ Phase 2 完了

#### 🗄️ PostgreSQL Engine 実装

- **PostgreSQL Backend**: sqlx 0.8を使用した完全なPostgreSQL対応
- **5個の実装モジュール** (1,254行)
  - `mod.rs`: DatabaseEngine trait実装
  - `connection.rs`: 接続プール管理とヘルスチェック
  - `prepared.rs`: パラメータ化クエリ実行 ($1, $2... プレースホルダ)
  - `transaction.rs`: ACID トランザクション・セーブポイント対応
  - `json_support.rs`: JSON/JSONB 型のネイティブサポート

#### ✨ 主要機能

- **パラメータ化クエリ**: SQLインジェクション防止の完全実装
- **トランザクション管理**: 4つの分離レベル (Serializable, RepeatableRead, ReadCommitted, ReadUncommitted)
- **セーブポイント**: ネストされたトランザクション対応
- **JSON操作**: PostgreSQLネイティブJSON/JSONB型のフルサポート
- **接続プール**: 健全性チェック・タイムアウト設定・統計情報取得

#### 🧪 テスト・品質保証

- **243テスト全て合格** (100% pass rate)
  - ライブラリテスト: 126/126 ✅
  - 統合テスト: 117/117 ✅
- **コンパイラ警告ゼロ**: clippy・rustc全て合格
- **IDE診断ゼロ**: VS Code warnings完全解決
- **ベンチマーク準備**: 15カテゴリ・484行

#### 🔧 開発環境配置

- **Docker Compose**: PostgreSQL 15 Alpine環境
- **VS Code設定**: cSpell・markdownlint・rust-analyzer最適化
- **CI/CD対応**: Pre-commit check完備

### 🎯 成功指標 (全て達成)

| 指標 | 目標 | 達成 |
|------|------|------|
| **テスト合格率** | 100% | ✅ 243/243 |
| **コンパイラエラー** | 0 | ✅ 0 |
| **警告** | 0 | ✅ 0 |
| **コード行数** | 1,254 | ✅ 実装完了 |

---

## 🚀 v0.15.0 - ユーザーフレンドリーな設定管理システム

**Release Date:** 2025-11-08
**Focus:** 初心者から上級者まで使いやすい包括的な設定管理とセットアップ体験

### 🎯 Major Features

#### 🔧 対話的設定セットアップ

- **`--setup-config`**: WordPress接続テスト付きの対話的設定作成ウィザード
- **`--demo-setup`**: 安全なデモンストレーションモードでの機能体験
- **リアルタイム接続検証**: WordPress API接続の即座テストと詳細エラー診断
- **クロスプラットフォーム対応**: crossterm使用のターミナルUI

#### 🔄 動的設定管理システム

- **`--switch-config`**: 実行時の設定ファイル動的切り替え
- **`--config <file>`**: カスタム設定ファイルの指定起動
- **`--reload-config`**: 設定の動的再読み込み（実行中サーバー向け）
- **設定ファイル自動検出**: `mcp-config.toml` → `config.toml` → `config/mcp.toml`

#### 🤖 自動化とユーザビリティ

- **設定ファイル不存在時の自動セットアップ起動**
- **包括的なヘルプシステム** (`--help`)
- **WordPress接続テスト機能** - 認証情報の即座検証
- **分かりやすい日本語エラーメッセージ**

### 🔧 Technical Improvements

#### 🏗️ 新しいモジュール構造

- **`src/setup/`**: 対話的セットアップシステム
  - `ui.rs`: crossterm使用のターミナルUI実装
  - `validator.rs`: WordPress接続検証とテスト
  - `demo.rs`: 安全なデモンストレーション環境
- **`src/config/dynamic.rs`**: 動的設定管理とリアルタイム切り替え

#### 🛡️ ロバストな入力処理

- **EOF検出と再試行制限**: パイプ入力時の無限ループ防止
- **入力ストリーム終了の適切な処理**
- **非対話環境での自動フォールバック**

#### 🎨 ユーザーエクスペリエンス向上

- **スピナーアニメーション**: 接続テスト中の視覚的フィードバック
- **カラフルなターミナル出力**: 成功/エラー/警告の色分け表示
- **プログレス表示**: セットアップ進行状況の明確な表示

### 🌐 ドキュメントとGitHub Pages

- **美しいランディングページ**: レスポンシブデザインのindex.html
- **GitHub Pages 404エラー修正**: 適切なJekyll設定とpermalink構造
- **包括的なREADME更新**: 動的設定管理機能の詳細説明

### 🧪 Testing & Validation

- **フォーマットチェック完全対応**: `cargo fmt --all -- --check` 通過
- **Clippy警告完全修正**: 冗長なクロージャの最適化
- **全コマンドオプションの動作確認**: `--setup-config`, `--switch-config`, etc.
- **WordPress接続テスト**: 実際のAPI接続による検証

### 📦 Dependencies Added

- `ratatui = "0.27"` - Terminal UI framework
- `crossterm = "0.27"` - Cross-platform terminal manipulation
- `tui-input = "0.8"` - Input handling utilities

---

## ✅ v0.14.0 - Policy Hot-Reload System (Epic #15)

**Release Date:** 2025-11-04
**Focus:** Live Configuration Management

### 📋 Policy Hot-Reload Features

- **Real-time Policy Monitoring**
  - File system watcher with debouncing (200ms)
  - Automatic detection of `.toml` policy changes
  - Non-blocking reload operations

- **4-Level Validation Pipeline**
  1. **Syntax Validation**: TOML parsing and structure verification
  2. **Semantic Validation**: Business logic and constraint checking
  3. **Security Validation**: Security rule verification and threat detection
  4. **Integration Validation**: Cross-component compatibility testing

- **Policy Application Engine**
  - Diff-based policy updates for minimal disruption
  - Rollback capabilities on validation failures
  - Comprehensive audit logging for all changes

### 📊 Performance Metrics

- **Reload Time**: 15-35ms end-to-end
- **Validation Speed**: 2-8ms per validation level
- **Memory Usage**: <5MB additional overhead
- **File Watch Latency**: <100ms detection time

### 🧪 Testing Coverage

- 6 comprehensive integration tests
- Hot-reload stress testing with rapid file changes
- Error handling validation for malformed policies
- Performance benchmarking suite

---

## 🗓️ Upcoming Releases

## v0.16.0 - Advanced Dashboard Features (Planned: 2025-11-06)

- Real-time charts and graphs visualization
- Historical metrics with trend analysis
- Alert system integration
- Export capabilities for monitoring data

## v0.17.0 - Auto-scaling & Health Checks (Planned: 2025-11-08)

- Automatic traffic adjustment based on SLA metrics
- Health check integration for validation
- Circuit breaker pattern implementation
- Promotion criteria automation

## v0.18.0 - Multi-Environment Deployment (Planned: 2025-11-10)

- Staging → Production pipeline automation
- Environment-specific policy management
- Cross-environment metrics comparison
- Advanced rollback strategies

---

## 📋 Version Numbering Strategy

We use a **0.01 increment versioning** approach for granular development tracking:

## Version Format: `0.XX.Y`

- **Major (0)**: Pre-1.0 development phase
- **Minor (XX)**: Feature releases with significant functionality (0.01 increments)
- **Patch (Y)**: Bug fixes and minor improvements within a feature release

## Development Phases

- **v0.01.0 - v0.10.0**: Foundation and Core Protocol Implementation
- **v0.11.0 - v0.20.0**: Advanced Features and Enterprise Capabilities
- **v0.21.0 - v0.30.0**: Cloud Integration and Scalability
- **v0.31.0 - v0.99.0**: Production Hardening and Ecosystem
- **v1.00.0+**: Production Release and Long-term Support

## Release Criteria for 0.01 Increments

Each 0.01 version must include:

1. **Functional Completeness**: All advertised features fully implemented
2. **Test Coverage**: Comprehensive integration and unit tests
3. **Documentation**: Updated README, architecture docs, and examples
4. **Performance Validation**: Benchmarking and optimization verification
5. **Code Quality**: Zero compiler warnings and clean code standards

---

## 🚀 Migration Guides

## Upgrading from v0.14.0 to v0.15.0

### New Dependencies

Add to your `Cargo.toml`:

```toml
ratatui = "0.27"
crossterm = "0.27"
tui-input = "0.8"

```

### API Changes

- New `CanaryDeploymentManager` for traffic management
- Dashboard integration through `run_dashboard()` function
- Enhanced event system with `CanaryEvent` types

### Configuration Updates

No breaking changes to existing policy configuration files.

---

## 📞 Support & Feedback

For questions about specific versions or upgrade assistance:

- **GitHub Issues**: [mcp-rs/issues](https://github.com/n-takatsu/mcp-rs/issues)
- **Discussions**: [mcp-rs/discussions](https://github.com/n-takatsu/mcp-rs/discussions)
- **Documentation**: [project-docs/](project-docs/)

---

*Last Updated: 2025-11-05*
*Current Version: v0.15.0*
