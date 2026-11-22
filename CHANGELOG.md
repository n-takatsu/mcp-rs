# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- `mcp-rs init`で、WordPressを使わずにデータベースだけを設定できるように
  した（#645）。設定ファイルを新しく書くときに、最初に何を使うか
  （WordPress・データベース・両方）を聞く。データベースだけなら、
  `[handlers.wordpress]`の無い設定ファイル（空の`[handlers]`と`[database]`）を
  書き、確かめてからクライアントに登録する。データベースは、設定ファイルを
  書く前に聞き、1度に書く（途中で止めても、未完成の設定ファイルで前のものを
  置き換えない）。データベースのパスワードに、WordPressの資格情報の名前は
  使えない。WordPressを使わない設定（`enabled = false`も含む）では、
  データベースにつながらなければ登録しない（使えるツールが無いため。
  WordPressもあるときは、今までどおり登録する）。`init`の確認は、
  `MCP_RS_CONFIG`ではなく、書いた設定ファイルを確かめる（`check_file`）。
  一時ファイルは、作るときから本人だけが読めるようにした。
  環境変数`MCP_RS_CONFIG`が別の設定ファイルを指すときは、質問の前に止める
  （それを受け継ぐクライアントは、そちらで起動するため）。パスワードの無い
  データベースだけなら、資格情報ストアが使えなくても設定できる。控えは、
  同じ秒に作っても前のものを上書きしない。前の設定を写す（#602）
  ときは、`[handlers.wordpress]`を写さない。既にある設定ファイルを残す
  ときは、今までどおり`[database]`が無ければ足すかを聞く

- `mcp-rs init`で、2つ目以降のWordPressのサイトを足せるようにした（#657）。
  WordPressを使う設定ファイルを書いた後と、既にある設定ファイルを残す
  ときに、サイトを足すかを聞く（データベースだけの設定ファイルには、最初の
  サイトを足せる）。サイトのURL・資格情報（資格情報ストアへ）・名前を聞き、
  `[handlers.wordpress.sites.<名前>]`を書き足す（控えを残し、ほかの設定・
  コメントはそのまま）。1つのサイトの形なら、今のサイトにも名前を付けて
  `sites`の下に移し、`default_site`にする（`site`を省いた呼び出しは、
  今までと同じサイトへ行く）。設定ファイルにある同じURL・ほかのサイトの
  資格情報の名前は使わない。サインインに失敗したら、そのサイトの資格情報を
  入れ直せる。「2つ目のサイト用」に別の設定ファイルで登録したmcp-rsは、
  今までどおり既定で残すが、そのサイトを足したときは、重複として既定で
  消す（#602の作りの見直し。その設定ファイルに、足していないサイト・
  データベース・プラグインがあるときと、相対パスのときは、既定では残す）。
  質問の間に設定ファイルが変わったときと、書けなかったときは、書かずに、
  この実行で作った資格情報の項目を消す（サイト名を聞く途中で止めたときも）。
  データベースのパスワードの名前は、WordPressのサイトに使わない。別の設定
  ファイルのURLが`${VAR}`なら、一致とはみなさない。各サイトの平文の
  パスワードも、控えを消すかを聞く対象にした。別の登録が`env`で`MCP_RS_CONFIG`を
  渡していれば、起動と同じくそちらの設定ファイルで照合し、既定では残す。
  Claude Desktopの設定の控えも、本人だけが読めるようにした。`mcp-rs uninstall`は、1つの設定ファイルの
  全サイトの資格情報を消せる

- Claude Desktopの拡張機能（MCP Bundle、`mcp-rs-<版>.mcpb`）をリリースに
  付ける（#632）。Windows・macOS（Apple Silicon）・Linuxのmcp-rsを1つに
  まとめ、サイトのURL・ユーザー名・アプリケーションパスワード（Claude
  DesktopがOSの保管場所に暗号化して保管）をClaude Desktopの画面で入力する。
  データベース・複数のサイト・プラグインには、画面で設定ファイルを選ぶ。
  署名はしない（SHA256と出所の証明は付ける）。中身は`packaging/mcpb/`、
  `release-binaries.yml`で公式のCLI（`@anthropic-ai/mcpb`）で確かめて作る
- 環境変数`MCP_RS_CONFIG`（#632）。`--config`の代わりに使う設定ファイル。
  拡張機能から起動したとき（`MCP_RS_MCPB=1`）は、サイトが入力されて
  いなければ、止まらずにWordPressを使わずに起動し、何を入力するかを
  `mcp_rs_status`に出す

- MCPのリソースのテンプレート（`resources/templates/list`）で、1件ずつ読める
  ようにした（#644）。`wordpress://post/{id}`・`wordpress://page/{id}`
  （編集用の原文も、資格情報で読めれば）・`wordpress://media/{id}`。
  読める範囲は今のリソースと同じ（対応する読み取りのツールを使える
  呼び出し元だけ、#538）で、監査ログに残す。無いIDは`-32002`。
  2026-07-28では、一覧に`ttlMs`・`cacheScope`を付ける。APIの文書に
  書いてあった、無いリソース（`wordpress://pages`等）を直した

- MCPのプロンプト（`prompts/list`・`prompts/get`）を出す（#643）。
  クライアントのメニュー（Claude Desktopの「+」等）から選ぶ、定型の頼み方:
  `draft_post`（記事の下書き。下書きで作り、公開しない）・
  `review_post_seo`（投稿のSEOの見直し。書き換えは承認の後）・
  `moderate_comments`（承認待ちのコメント。変えるのは決めた後）・
  `summarize_table`（`[database]`のとき。表の要約、読むだけ）。文は英語で、
  利用者の言葉で答えるよう頼む。能力に`prompts`を足した。HTTPモードでは、
  プロンプトが使うツールをすべて使えるAPIキー・トークンにだけ出し、断った
  `prompts/get`は監査ログに残す。複数のWordPressのサイトでは`site`の引数を
  持つ。2026-07-28では`ttlMs`・`cacheScope`を付ける。プラグインの
  プロンプトは、まだ中継しない。APIの文書に書いてあった、無いプロンプトの
  例を直した

- カスタム投稿タイプと独自のタクソノミーを扱えるようにした（#642）。使う
  カスタム投稿タイプは、設定の`post_types`（例: `["product"]`）に書いた
  ものだけ（既定は今までどおり`post`・`page`だけ。AIが商品等を誤って
  書き換えないように）。`get_posts`・`get_post`・`create_advanced_post`・
  `update_post`・`delete_post`・リビジョンのツールの`post_type`に、許した
  型を渡せる（ツールの`post_type`には、許した型だけを出す）。サイトの
  `/wp/v2/types`の`rest_base`を使い、`wp/v2`に無い型は理由を示して断る。
  新しいツール: `get_post_types`（型と、使えるか）・`get_taxonomies`・
  `get_terms`。`create_advanced_post`・`update_post`の`terms`
  （`{"product_cat": [12, 15]}`）で、タクソノミーの項目を付け替える。
  `get_post`は、カスタム投稿タイプの項目も出す。独自のタクソノミーの項目の
  作成・変更は、まだ

- 投稿・固定ページのリビジョンを見て、書き戻すツール（#641）。
  `get_revisions`（新しい順に、日時・書いた人・題・長さ・本文の先頭）・
  `get_revision`（1つの題・抜粋・本文。`format`は`get_post`と同じ）・
  `restore_revision`（題・本文・抜粋をリビジョンのものにする。状態・
  公開日時・スラッグ・カテゴリ・タグは変えない）。WordPressは書き戻す前の
  内容を新しいリビジョンとして残すので、書き戻しも元に戻せる。別の投稿の
  リビジョンは断る。原文が読めない（編集の権限が無い）ときは、表示用の
  HTMLで上書きしない
- コメントのツールの監査ログに、コメントのID（`comment_id`）を残す（#639の
  漏れ）

- 1つのmcp-rsで、複数のWordPressのサイトを使えるようにした（#640）。
  各サイトを対等に`[handlers.wordpress.sites.<名前>]`に書き（`url`・
  資格情報等）、`[handlers.wordpress]`には、全体の`enabled`・既定のサイト
  （`default_site`。サイトが2つ以上なら要る）・全サイト共通の設定
  （`timeout_seconds`・`rate_limit`・`upload_allowed_dirs`。サイトごとに
  上書きできる）だけを書く。ツールは増やさず、各ツールに`site`の引数を
  足す（省けば既定のサイト）。資格情報・レート制限・監査ログ（サイトの
  URL）は、サイトごと。`mcp-rs check`はサイトごとに確かめ、`mcp_rs_status`は
  サイトを並べ、`mcp-rs uninstall`は各サイトの資格情報ストアの項目を消すかを
  聞く。リソースは既定のサイトだけ。`WORDPRESS_URL`等の環境変数は、
  `sites`を使う設定を上書きしない。今までの形（`[handlers.wordpress]`に
  `url`を直接書く、サイトが1つ）は、そのまま使える（`site`の引数も
  付かない）。`mcp-rs init`は今までの形で1つのサイトを設定し、`sites`を
  使う設定ファイルは、置き換えずにそのまま使う

- コメントを管理するツール（#639）。`create_comment`（返信は`parent`。
  `post_id`を省けば、返信先のコメントの投稿にする）・`update_comment`
  （承認・承認待ちに戻す・スパム・ゴミ箱、本文の変更）・`delete_comment`
  （既定はゴミ箱、`force`で完全に削除）。WordPressのコメントには下書きが
  無いので、`create_comment`は、既定で承認待ち（`hold`）で作り、サイトには
  出さない（`status: "approve"`ですぐに公開）。`get_comments`に`status`
  （`approve`・`hold`・`spam`・`trash`・`all`）を足した。結果に、返信先・
  状態・日時・リンクも出す

### Changed

- Dockerイメージを、muslで静的にリンクした1つの実行ファイルにし、土台を
  `gcr.io/distroless/static-debian12:nonroot`にした（#631）。以前の土台
  （`distroless/cc-debian12`）に入っていたDebianの`libc6`・`libssl3`・
  `libstdc++6`等が無くなり、イメージの検査（Trivy）の警告のほとんどが消える。
  mcp-rsはOpenSSLを使わない（通信はrustls）。CAの証明書・タイムゾーンの
  データ・`nonroot`のユーザーは、今までどおりイメージに入っている。
  C言語の標準ライブラリがglibcからmuslに変わるので、名前の解決（DNS）と
  メモリーの割り当ての動きが少し変わる。リリースのLinuxの実行ファイル
  （`x86_64-unknown-linux-gnu`）は変わらない
- WordPressのツールの説明を書き直した（#676）。AIのクライアントが、どの
  ツールをいつ使うかを選びやすくするため。どのツールの説明にも、何をするか、
  似たツールとの違い・いつ使うか、何を返すか、書き込み・削除の影響（下書きか
  公開か、ゴミ箱か完全に削除か等）を書いた。どのツールにも、短い名前
  （`title`）を付けた。引数の説明も直した。`delete_category`・`delete_tag`の
  `force`は「ゴミ箱を通さずに消す」と書いていたが、WordPressはカテゴリー・
  タグをゴミ箱に入れられないので、`true`が要る。`create_advanced_post`の
  引数の説明に混ざっていた日本語を英語にした。`update_post`等の
  `categories`・`tags`は、今のものを置き換えることを書いた。ツールの動きは
  変わらない
- 設定ファイルが無くても、STDIOモードでは起動する（#667）。`--config`が無く、
  `~/.config/mcp-rs/config.toml`も、環境変数による設定（`WORDPRESS_URL`等）も
  無いとき、エラーで終わらず、設定なし
  （ハンドラー無し、`mcp_rs_status`だけ）で起動する。ログに警告を出し、
  `mcp_rs_status`の`Config file`に、設定ファイルが無いことと直し方
  （`mcp-rs init`か`--config`）を出す。MCPの一覧サイト（Glama等）・
  インスペクターが設定なしで`tools/list`を読めるように、またクライアントに
  「起動しない」としか出ないのを避けるため。変えないこと: 今のフォルダの
  設定ファイルは読まない（#541、以前の名前のファイルの案内も出す）。
  `--config`で渡したファイルが無い・読めないとき、HTTPモード、
  `mcp-rs check`は、今までどおりエラー。案内の文に`mcp-rs init`を足した。
  ライブラリとして使う場合: `McpConfig`・`StatusInfo`に
  `missing_config_file`、`McpConfig::load_for_startup`・`uses_stdio`を足した
- `post_type`に`post`・`page`以外の値を渡したとき、断る（#642）。以前は、
  知らない値を投稿（`post`）として扱っていた。ライブラリとして使う場合:
  `PostCreateParams`・`PostUpdateParams`に`terms`、`WordPressPost`に
  `extra`、`WordPressConfig`に`post_types`を足した
- `get_comments`は、コメントした人のメールアドレスを出さない（#639）。
  コメントの管理には要らず、AIとの会話やログに残さないため
- `create_post_with_embeds`で、YouTubeのShorts（`youtube.com/shorts/`）・
  ライブ（`youtube.com/live/`）・`youtube-nocookie.com/embed/`のURLも、
  動画プレーヤーにする（#680）。以前は、エラーにならずに本文に足さなかった。
  `watch`のURLは、`v`が問い合わせの先頭でなくても
  （`watch?feature=shared&v=…`）受け付ける。
  本文に足さなかったURL（受け付けない形のYouTubeのURL、対応しないサービスの
  URL）は、結果に「Not embedded (unsupported URL)」として出す（以前は黙って
  捨てていた）。動画のIDを取り出せないYouTubeのURLは、URLのまま足していたが、
  足さずに結果に出す。YouTube・SNS（`social_urls`）のURLは、URLを分解して、
  ホストが決まったもの（`youtube.com`・`www.x.com`等）と完全に一致するかで
  判定する。以前は文字列を含むかだけを見ていたので、ほかのサイトのURL
  （`https://example.org/x.com/…`、`netflix.com/`等）も本文に足していた。
  YouTubeのURLは、IDの後ろに余分なパス（`/shorts/<ID>/extra`等）があれば
  断る。ツールの説明・文書に、WordPressは5.5.2からInstagram・Facebookを
  埋め込まない（プラグインが要る）ことと、結果はURLを本文に足したことだけを
  示すことを書いた


### Fixed

- `mcp-rs init`で、Claude Code・Copilot CLIの前の登録（`mcp-rs`）を消すのを、
  別の名前のmcp-rsを消すかを聞き終えた後にする（#710）。新しい登録に失敗
  したら、消した前の登録をクライアントの設定ファイルに戻す。以前は、先に
  消してから聞き、途中で終わる・登録に失敗すると、前の登録（引数・環境
  変数）が失われた
- `mcp-rs init`で、Claude Codeの前の`mcp-rs`がユーザーの設定に無く、別の
  スコープ（あるフォルダだけのlocal、プロジェクトの`.mcp.json`）にあるとき、
  そちらが使われ続けることを知らせる（#710）。以前は、消せないまま
  ユーザーの設定に足し、何も知らせなかった
- `mcp-rs init`で、`[transport] type = "http"`の設定ファイルを、クライアントに
  登録しない（#710）。クライアントはSTDIOでつなぐが、その設定ではHTTP
  サーバーが動くので、つながらなかった。前の設定ファイルをそのまま使うと
  選んだときに起きた
- `mcp-rs init`で、資格情報ストアに既にある項目を入れ直し、確認（サイン
  イン）が通らずに終わったら（エラー・入力の終わりを含む）、前の中身に戻す
  （#710）。以前は、確かめる前に上書きしたままで、同じ項目を使うほかの設定
  ファイルも、通らない資格情報を読んだ。前の中身を読めなければ、上書き
  しない
- `mcp-rs init`で、ユーザー名・パスワードの入ったサイトのURL
  （`https://user:pass@example.com`）を受け付けない（#710）。以前は、
  設定ファイルの`url`に平文で書いた
- `mcp-rs init`が表示するコマンドの引数を、貼り付ける
  シェル（WindowsはPowerShell、ほかはsh・bash）で展開されない形（単一引用符）
  にする（#710）。以前は空白と引用符だけを見て`"…"`で囲んだので、`$`・
  `` ` ``等を含むパスは、貼ると別のパスになった
- `mcp-rs init --config --help`のように、`--config`の次がオプションなら、
  パスとして受け取らずに断る（#710）。`-`で始まるパスは`--config=<path>`で
  渡せる
- ホームフォルダの環境変数`HOME`が空の文字列なら、`USERPROFILE`を見る
  （#710）。以前は見ず、Windowsで`--config`無しの起動・`mcp-rs init`、`~`の
  展開が使えなかった
- `mcp-rs init`で、登録が使う設定ファイルが同じかを比べるとき、Unixでは
  `\`を区切り文字にそろえない（#710）。Unixでは`a\b.toml`と`a/b.toml`は
  別のファイルなので、別の設定ファイルの登録を、同じとして消す既定にし
  得た
- HTTPで、2026-07-28のリクエスト（`_meta`で版を送るもの）の版を、
  ハンドラーに渡す前に確かめる（#697）。対応していない版には、400と
  `-32022`を返す。組み込みのハンドラーは以前から自分で確かめていたが、
  ライブラリとして独自のハンドラー（`HttpTransport::with_request_handler`）を
  使う場合は、対応していない版がそのまま届いていた
- `mcp-rs check`で、KubernetesのプラグインのPodの準備中に、起動の確かめ
  （10秒）が打ち切られても、作ったJob・Secretを消す（#697）。以前は、準備の
  中でエラーになったときだけ消し、外から打ち切られたときは残っていた。
  打ち切られても後片付けを実行するガードにした（作る要求の途中で
  打ち切られ、後から作り終えたものも消すよう、3秒後にもう1度消す）。また、`check`は、止めた
  プラグインの後片付け（`docker rm -f`・`kubectl delete`）の終了を、最長
  10秒待ってから終わる。以前は待たずに終わり、コンテナ・Job・Secretが残る
  ことがあった。失敗した・終わらなかったものは警告する
- Kubernetesのプラグインの`env`の値を入れるSecretを、作る時点でJobを
  所有者にする（#700）。以前は、Secret → Job の順に作り、後からSecretの
  所有者をJobにしていた。Secretを作る要求の途中で打ち切られ、作成の確定が
  後片付けより遅れると、所有者の無いSecret（`env`の値を含む）が残ることが
  あった。今は、Jobを一時停止（`spec.suspend`）で作り、Jobを所有者にした
  Secretを作ってから、一時停止を解く。Secretがいつ作られても、Jobが
  無ければKubernetesが消す。**`kubectl`の権限が変わる**：`jobs`に`get`・
  `patch`が要り（`kubectl patch`は変える前にJobを読む）、`secrets`の`patch`は
  要らなくなった。`env`を使うプラグインは、Kubernetes 1.22以降が要る
  （`spec.suspend`が既定で有効な版）。`env`を使わないプラグインは変わらない
- `tools/list`で、ツールを制限したキー・トークンの呼び出し元に、ハンドラーの
  一覧を1回だけ取って返す（#705）。以前は、返す一覧と、制限の判定用に、
  2回取っていた。2回目だけ一覧を返せなかったハンドラー（プラグイン等）の
  ツールは、1回目に取れていても一覧から消えた
- `mcp-rs init`で、平文のパスワードを含む控えを消すかを、設定の確認が
  通ってクライアントに登録した後に聞く（#705）。以前は、確認とサインインより
  前に聞き（既定は「はい」）、後でサインインに失敗して終わると、前の設定
  （引き継がなかった設定を含む）が残らなかった。質問に、消すと引き継が
  なかった設定も消えることを書いた。どのクライアントにも登録しなかった
  ときは、聞かずに残し、場所を知らせる。消せなかったときは、知らせて続ける
  （以前は`init`全体が止まった）
- `mcp-rs init`で、クライアントの登録が使う前の設定ファイルが読めない
  （UTF-8でない・読み取りの権限が無い）、またはTOMLでないときも、置き換える
  設定かを聞く（#705）。以前は、TOMLでなければ聞かずに置き換えるものとし
  （それを使う登録は、削除の既定が「はい」）、読めなければ`init`全体が
  止まった
- `mcp-rs init`・`mcp-rs database-keyring`で、設定ファイルがシンボリック
  リンクなら、リンク先に書く（#705）。以前は、一時ファイルを名前の付け替えで
  置き換えるので、リンク（dotfilesの管理等）が普通のファイルに置き換わった
- `mcp-rs check`で、ほかのユーザーが読める設定ファイルへの警告（v0.27.0）を、
  `--config`無しでも出す（#705）。以前は`--config`を付けたときだけで、
  `mcp-rs init`が既定で書く`~/.config/mcp-rs/config.toml`を`--config`無しで
  確かめると、出なかった
- MySQL・MariaDBのエンジンの`validate_config`が、`ssl_root_cert`を書いた
  `ssl_mode = "verify-ca"`も断っていたのを直した（#705）。接続と同じCAで
  確かめる。mcp-rs本体は使っていない（ライブラリとして使う場合のみ）
- `mcp-rs check`で、コンテナのプラグインの`env`の名前も、起動するときと
  同じく確かめる（#697）。以前は、`DOCKER_HOST`のように起動時に断られる
  名前でも、`--offline`では通っていた
- `mcp-rs check`の説明（`--help`・文書）で、表示しないのはWordPressの
  ユーザー名だと書いた（#697）。`[database]`のユーザー名は、サーバーと
  合わせて表示する（`reader@127.0.0.1:5432/app`等）
- `mcp-rs init`・`database-keyring set`で、資格情報ストアに既にある項目を、
  確かめずにデータベースの資格情報で上書きしない（#692）。ほかの種類の項目
  （別の設定ファイルのWordPressのサイト等）なら、`init`は別の名前を聞き、
  `database-keyring set`は断る。データベースの資格情報なら、`init`は置き
  換えるかを聞く（既定は置き換えない）。`database-keyring set`は、今までどおり
  入れ直せる
- `mcp-rs init`で、MySQL・MariaDBの`ssl_mode = "verify-ca"`では、CAの
  ファイルを必須として聞き直す（#692）。以前は空で答えられ、パスワードを
  資格情報ストアに入れた後で、最後の確認に断られて止まっていた
- `mcp-rs database-keyring set`で、`[database]`の`${VAR}`を展開できない
  とき、展開前の文字列（`${DB_HOST}`等）をホスト・ユーザー名の既定にしない
  （#692）。以前は、そのまま資格情報のホストとして入り、起動時に見つから
  なかった。値そのものを聞く
- データベースの資格情報を使うとき、Unixソケットのパス（`/`で始まる
  ホスト）は大文字小文字を区別して比べる（#692）。ホスト名は今までどおり
  区別しない
- `mcp-rs uninstall`で、資格情報ストアを読めなかった項目（Secret Serviceが
  止まっている等）を、無い項目と同じように黙って飛ばさない（#692）。読め
  なかったことと、消していないことを、消すコマンド（`wordpress-keyring
  delete`・`database-keyring delete`）と合わせて知らせる。既定以外の設定
  ファイルの項目は、登録を外した後ではもう1度実行しても見つからないので、
  その設定ファイルも示す。その名前を使う設定ファイルは、消すかを聞かずに
  残す（消すと、残った資格情報を探す手がかりが無くなる）
- `mcp-rs init`・`uninstall`で、Claude Desktopの設定ファイルを、本人だけが
  読めるように書く（#692）。ほかのサーバーの`env`（トークン等）が入って
  いることがある。以前は、控えだけを本人だけが読めるようにし、設定ファイル
  そのものは既定の権限（Unixでは多くの場合644）で書き直していた
- 一覧の全件取得（`get_posts`・`get_comments`・`get_media`・`get_categories`・
  `get_tags`・`get_terms`・`get_revisions`・`get_all_content`等）が上限の
  100ページ（10,000件）を超えるとき、エラーにする（#692）。以前は、最初の
  10,000件を全件のように返し、結果は「Found 10000 posts」としか出なかった。
  投稿・固定ページは、`per_page`・`page`で1ページずつ読める
- 更新の確認（`mcp_rs_status`・`mcp-rs check`）で、GitHubの応答を
  読みながら1MiBの上限で止める（#692）。以前は、全部を読み込んでから
  大きさを確かめていた
- 文書とツールの説明の、予約投稿の日付の例を先の日付にした（#692）。
  過去の日付では、予約ではなく公開になる
- Scoopのmanifestの自動更新（`checkver`）が動いていなかった（#692）。
  `"checkver": "github"`は`homepage`をGitHubのリポジトリとして読むが、
  `homepage`はGitHub Pagesのため、エラーで飛ばしていた。`checkver.github`に
  リポジトリのURLを書いた。CIの確かめも、エラーで何も作り直さなくても
  通っていたので、一時コピーのURL・ハッシュを変えておき、作り直したことで
  確かめる。版はmanifestの版に固定する（新しいリリースが出ただけで失敗しない）
- 設定ファイルのエラーで、足りない項目・知らない項目の名前を伏せない
  （#617）。以前は``missing field `…` ``のように出て、どこを直せばよいか
  分からなかった。値は今までどおり伏せる（#543）。知らない項目の名前は、
  書いたものなので、64文字で切り、制御文字をエスケープする
- STDIOモードで、Ctrl+Cを受けて終わる（#617）。標準入力の終わりと同じく、
  プラグインを止めてから、終了コード0で終わる。以前は、Windowsで
  `STATUS_CONTROL_C_EXIT`（0以外）で止まり、`cargo run`がエラーを出していた。
  STDIOモードの終了（Ctrl+C・標準入力の終わり）で、コンテナ・Kubernetesの
  プラグインの後片付け（`docker rm -f`・`kubectl delete`）の終了を、最長10秒
  待ってから終わる（PR #672のレビュー）。以前は待たずに終わり、コンテナ・
  Job・Secretが残ることがあった
- HTTPモードの終了（Ctrl+C）で、プラグインを止め、後片付けの終了を最長10秒
  待つ（#673）。以前は、HTTPの処理がプラグインを持ったまま終わり、コンテナ・
  Job・Secretが残ることがあった。処理中のプラグインの呼び出しは、応答を
  待たずにエラーで終わる。後片付けが10秒で終わらなければ、ログに警告を出す。
  ライブラリとして使う場合: `McpHandler`に`shutdown`（既定は何もしない）、
  `McpServer`に`shutdown`を足した
- 複数のWordPressのサイト（`[handlers.wordpress.sites.<名前>]`）で、
  サイトに`upload_allowed_dirs = []`・`post_types = []`と書くと、
  `[handlers.wordpress]`のものを受け継がない（#684）。以前は、空のときも
  受け継ぎ、そのサイトだけ`file_path`を止める・`post`・`page`だけにする
  ことができなかった。省いたときは、今までどおり受け継ぐ。
  ライブラリとして使う場合: `WordPressConfig`の`upload_allowed_dirs`・
  `post_types`を`Option<Vec<String>>`にした（省いたときは`None`）
- 複数のWordPressのサイトで、`enabled = false`のサイトの資格情報を読まない
  （#684）。以前は、そのサイトの`encrypted_credentials`を復号し、
  `WORDPRESS_MASTER_PASSWORD`が無い・違うと、ほかのサイトも起動できなかった
- 複数のWordPressのサイトで、ツールの引数`post_type`に、どれかのサイトで
  許したカスタム投稿タイプを並べる（#684）。以前は、既定のサイトのものだけで、
  ほかのサイトだけで許した型を選べなかった。使えるかは、今までどおり
  サイトごとに確かめる
- STDIOモードが、標準入出力の失敗（クライアントが去った等）で終わるときも、
  プラグインを止め、後片付けの終了を待つ（#684）。以前は、エラーで先に
  終わり、コンテナ・Job・Secretが残ることがあった。HTTPモードでも、
  HTTPサーバーを始められなかったときに、プラグインを止める
- プラグインの後片付け（`docker rm -f`・`kubectl delete`）が失敗した・
  起動できなかったときに、ログに警告を出す（#684）。以前は、時間切れの
  ときだけ警告し、失敗（権限・認証・接続）は見逃していた。警告には、
  コマンド（環境変数は含めない）と理由を出す。手で消すか、
  `docker ps -a --filter label=mcp-rs.plugin`・
  `kubectl get jobs,secrets -l mcp-rs.plugin`で確かめる。既に消えていた
  場合は、失敗にしない。ライブラリとして使う場合:
  `plugin_proxy::wait_for_cleanups`は、`bool`ではなく`CleanupReport`
  （時間切れと、失敗したコマンド）を返す
- 本文（`editor`）・タイトル（`title`）に対応していないカスタム投稿タイプの
  投稿を読めるようにした（#684）。以前は、WordPressの応答に`content`・
  `title`が無く、読むのに失敗していた。本文の無い投稿は、`get_post`で
  空の本文としては見せず、本文の項目が無いことを添える。タイトルの無い
  投稿は、空のタイトルとして読む。ライブラリとして使う場合:
  `WordPressPost`の`content`を`Option<WordPressContent>`にし（無ければ
  `None`）、`WordPressContent`に`Default`を足した
- `restore_revision`の説明と結果で、書き戻しを必ず元に戻せるとは言わない
  （#684）。書き戻す前の内容がリビジョンとして残るのは、サイトでリビジョンが
  有効なとき（`WP_POST_REVISIONS`）だけのため。残したいときは先に
  `get_post`で読むよう案内する
- `create_advanced_post`・`update_post`の`terms`で、投稿タイプに登録されて
  いないタクソノミーを断る（#684）。以前は、WordPressが黙って無視し、成功と
  出していた。`terms`があるときだけ、送る前にサイトの
  `/wp/v2/taxonomies?type=<投稿タイプ>`で確かめ、使える`rest_base`を
  エラーに出す
- `get_revisions`・`get_revision`で、書いた人の名前を同時に聞き、全体で
  10秒までしか待たない（#684）。以前は1人ずつ、再試行を含めて待ち、
  上限が無かった。読めなかった人は今までどおり`user <ID>`と出す
- `mcp-rs init`・`mcp-rs uninstall`のレビューの指摘を直した（#684）
  - サイトを足したとき、同じサイトだけを使う別の設定ファイルの登録でも、
    サイトごとの設定（`post_types`・`upload_allowed_dirs`等）や
    `[server]`・`[transport.http]`等があれば、既定では消さない。消すと
    失われる設定を、質問に示す。以前は、`[database]`・`[[plugins]]`だけを
    見ていて、設定の多い登録を既定で消していた
  - `--config`を渡したとき、複数のサイトを使う前の設定ファイルに、黙って
    切り替えない。どちらを使うかを聞く（既定は`--config`の場所）
  - 同じ名前（`mcp-rs`）の登録を置き換えるかを聞くときは、その登録が
    実際に読む設定ファイル（クライアントごとに見る）が、新しい設定ファイル
    でも、置き換えると選んだ前の設定ファイルでもなければ、そこにあって
    新しい設定ファイルに無いサイトと設定を挙げ、既定では置き換えない。
    `--config`の場所を選んだ後や、前の設定ファイルを「置き換える前の設定
    ではない」と答えた後は、その設定ファイルを確かめられない（相対パス・
    読めない）ときも、既定では置き換えない
  - 別の名前の登録を消すかを聞くとき、新しい設定ファイルの最初のサイトも、
    足したサイトとして扱う（同じサイトの重複として、既定で消す）。
    `enabled = false`のサイトは、足したサイトとして扱わない
  - 登録を置き換える・消すかを聞くとき、同じURLのサイトの名前
    （`site`の値）が新しい設定ファイルで変わるときや、`site`を省いた
    呼び出しが別のサイトに送られるようになるときも、失われるものとして
    挙げ、既定では置き換えない・消さない（`default_site`を省いた設定は、
    起動と同じく、使う1つのサイトを既定とする）。1つのサイトの形の
    設定を使う登録は、同じ名前の登録を置き換えるときだけ、`site`を省いた
    呼び出しの送り先を比べる（別の名前の登録を同じmcp-rsへまとめる流れは
    変えない）
  - 前の設定を写して、WordPressだけと選んだときは、写した`[database]`を
    外す（そう表示する）。以前は、`allow_write`も含めて残り、データベースの
    ツールも登録されていた
  - `uninstall`は、資格情報ストアの項目を、設定ファイルの場所ではなく、
    入っている種類（WordPress・データベース・mcp-rsのものではない）で
    示す。場所と種類が合わなければ、実際の種類を示し、既定では消さない
  - Unixで、本人だけが読めるファイル（設定ファイル・控え）を作った後、
    実際のモードを確かめる。vfat等で、ほかの人も読めるモードになったら、
    中身を書く前にファイルを消して、エラーで止める（WindowsのACLの確認と
    同じ）

### Security

- HTTPで、認証の失敗が続くアドレスを止める仕組み（#596、1分に20回）が、
  覚えるアドレスの数の上限（1万件）に達すると、新しいアドレスを数えずに
  通していた（#705）。たくさんのアドレス（IPv6等）で表を埋めると、後の
  アドレスからは上限なく試せた。今は、覚えきれないアドレスを、まとめて
  1つとして数える（メモリーの上限はそのまま）。また、前の1分の間に始めた
  認証が、次の1分になってから通ったときに、次の1分の失敗の数を減らして
  いたのを直した
- Windowsで、`mcp-rs init`・`mcp-rs database-keyring`が書く設定ファイル・
  控え・一時ファイルを、本人・SYSTEM・Administratorsだけが使えるACL
  （親フォルダから受け継がない）で作るようにした（#674）。Unixの`600`と
  同じく、内容を書く前の、作るときから制限する。以前は親フォルダのACLを
  受け継いだので、`--config`でほかの人も読めるフォルダに書くと、設定
  ファイルに残った平文のパスワードや、前の設定ファイルの控えを、ほかの
  ローカルユーザーが読めた。前の版で作った控え等のACLは変えない。FAT32・
  exFAT等のACLの無いファイルシステムでは、本人だけにできないので、
  何も書かずにエラーにする
- 依存クレートを、セキュリティ勧告の修正版に上げた。`Cargo.toml`の下限も
  `url = "2.5.9"`・`mysql_async = "0.37"`に上げ、`lru = "0.18.2"`を
  `mysql-backend`の機能の依存に足した（`mysql_async` 0.37は`lru` 0.18.0
  以降を許すため）ので、mcp-rsをライブラリとして使う側も、勧告の対象の版を
  選ばない
  - `lru` 0.16.4 → 0.18.5（RUSTSEC-2026-0253: `LruCache::pop()`がパニック
    に対して安全でなく、解放後の使用になり得る。健全性（unsound）の警告）。
    `mysql_async` 0.36.2 → 0.37.1（`mysql_common` 0.35.5 → 0.37.3）に
    上げた。0.36は`lru` 0.16を要求していた（MySQLを使うビルドのみ）
  - `url` 2.5.7 → 2.5.9（RUSTSEC-2026-0592: 入れ子の`blob:`のURLで
    `Url::origin()`がスタックオーバーフローする）

## [0.29.0] - 2026-11-19

### Removed

- 本体（`[database]`・`DatabaseHandler`）から使われていなかった、データベースの
  モジュールを消した（#638）。`handlers::database`の`loadbalancer`・`retry`・
  `advanced_security`（`AnomalyDetector`・`MultiFactorAuth`・
  `RoleBasedAccessControl`）・`integrated_security`・`security_config`・
  `dynamic_engine`・`dynamic_tools`・`data_masking`・`masking_formatters`・
  `masking_rules`と、古いPostgreSQLの実装`engines::postgres`
  （`PostgresEngine`。本体は`engines::postgresql`を使う）。それだけを使う
  テスト・例・ベンチマーク・設計の文書（`docs/`）と、使われなくなった依存
  （`num_cpus`）も消した。約17,000行。動かない「TODO」（VPNの検証・cron式・
  ClickHouse等）が残り、点検の範囲を増やしていた。**互換性**: mcp-rsを
  ライブラリとして使い、これらの型を使っていた場合は、ビルドできなくなる
  （mcp-rs本体の動きは変わらない）

### Security

- WordPressの応答を読む大きさに上限を設けた（#598）。1つの応答は32MB、
  一覧の全件取得（`get_posts`等）は合計64MB・100ページ（10,000件）まで。
  以前は、応答を丸ごと読み、`X-WP-TotalPages`をそのまま信じていたので、
  乗っ取られたサイト等が、大きな応答や大量のページで、メモリーを使い
  切らせられた。上限を超えた応答はエラーにする。100ページを超える一覧は、
  先頭の100ページを返し、ログに警告を出す。エラーの応答の本文は、
  先頭の64KBだけを読む
- プラグインのツールの名前を確かめる（#598）。MCPの名前の形（英数字・`_`・
  `-`・`.`、プラグインの名前を付けて128文字まで）でないツールは、警告を
  出して使わない。空白・制御文字・`*`の入った名前が、ログ・監査ログ・
  ツールの制限の名前の指定に、そのまま入っていた。ツールの説明に
  AIへの指示を入れられること（tool poisoning）を、文書に書いた

### Changed

- プラグインの相対パスの`command`（区切り文字を含むもの）と`working_dir`を、
  設定ファイルのフォルダを基準にする（#598）。以前は、mcp-rsを起動した
  フォルダ（Claude Codeでは開いたフォルダ）を基準にしていたので、開いた
  フォルダの同じ名前のプログラムを動かすおそれがあった。`node`のような名前だけの
  コマンドは、今までどおり`PATH`から探す。コンテナ・Kubernetesのプラグインは
  変わらない

### Added

- `mcp-rs uninstall`（#636）。`mcp-rs init`が設定したものを、1つずつ
  確かめてから外す: Claude Desktop（設定ファイルの控えを残す）・Claude Code・
  Copilotの、mcp-rsの登録、設定ファイルの`keyring`が指す、資格情報ストアの
  WordPress・データベースの項目、設定ファイル（既定は残す）。mcp-rs自身は
  消さず、入れ方（Scoop・Homebrew・インストールスクリプト・それ以外）に
  合わせた消し方を示す
- `mcp-rs check`と`mcp_rs_status`で、新しい版があれば知らせる（#635）。
  GitHubのリリースの最新の版と比べ、入れ方（Scoop・Homebrew・インストール
  スクリプト・それ以外）に合わせた更新のしかたを示す。GitHub
  （`api.github.com`）には、版を含む`User-Agent`だけを送る。
  `mcp_rs_status`は、STDIOモードでだけ、mcp-rsが動いている間に1回だけ
  問い合わせる（HTTPモードでは問い合わせない）。`mcp-rs check --offline`は
  問い合わせない。`[server] check_updates = false`で止められる
- `get_posts`・`get_pages`で、検索・絞り込み・ページを指定できるように
  した（#637）。`search`・`categories`・`tags`（固定ページには無い）・
  `author`・`after`・`before`・`orderby`・`order`・`per_page`・`page`。
  どれかを渡すと、全件ではなくそのページだけ（既定は20件）を読み、
  1行目にページと総件数を出す。何も渡さなければ、今までどおり全件。
  値は、形を確かめ、URLの形に符号化して送る。`get_pages`も`status`を
  受けるようにした
- Scoop（Windows）とHomebrew（macOS Apple Silicon・Linux x64）で入れられる
  ようにした（#578）。`scoop bucket add n-takatsu https://github.com/n-takatsu/scoop-bucket`・
  `scoop install n-takatsu/mcp-rs`、`brew install n-takatsu/tap/mcp-rs`。
  リリースのビルド済みのmcp-rsを入れる。manifest・formulaの元は
  `packaging/`にあり、CI（`packaging.yml`）で実際に入れて確かめる。bucket・
  tapは、新しいリリースに自動で追いつく。wingetは、まだ
- `mcp-rs init`は、Homebrewで入れたmcp-rsを、版によらないパス
  （`<prefix>/opt/mcp-rs/bin/mcp-rs`）で登録する（#578）。版ごとのフォルダ
  （`Cellar/mcp-rs/<版>`）は、更新で消えるため
- `mcp-rs database-keyring set|show|delete <名前>`（#628）。`[database]`の
  パスワードを、`mcp-rs init`を使わずに資格情報ストアに入れる・見る・
  消す。`set`は、設定ファイル（`--config`。無ければ既定の場所）に
  `[database]`があれば、そのホスト・ポート・ユーザー名を既定にして
  パスワードを入れ直し、`[database]`を`keyring`を使う形に書き換えるかを
  聞く（`password`を消す。コメント等は残し、控えを残す）。無ければ、
  `mcp-rs init`と同じ質問で項目を作り、`[database]`を書き足すかを聞く。
  `show`はパスワードを出さない。WordPressの資格情報は、見せず、消さない
- `mcp-rs init`で、データベース（PostgreSQL・MySQL・MariaDB）も設定できる
  ようにした（#590）。WordPressに続けて、使うかを聞き、エンジン・ホスト・
  ポート・データベース名・ユーザー名・パスワード・TLS（`ssl_mode`。ループ
  バック以外では`verify-full`が既定）・CAのファイルを聞いて、設定ファイルに
  `[database]`を書き足す（ファイルは置き換えず、控えを残す。既に`[database]`
  があれば、そのまま）。書き込み（`allow_write`）は既定で無効で、許すときは
  2回確かめる。データベースだけが使えないときは、登録を止めない
- `[database]`の`keyring`（#590）。パスワードを、WordPressと同じくOSの資格
  情報ストアから読む。Claude Desktopはユーザーの環境変数を渡さないので、
  `${VAR}`では、クライアントの設定に平文で書く必要があった。資格情報ストアの
  項目は、`host`・`port`・`username`と結び付いていて、`[database]`の値が
  違えば使わない（ほかの人が書いた設定ファイルで、パスワードを別の
  サーバーに送らせないため）。`password`とは一緒に書けない
- `update_post`で、既存の投稿を予約投稿にできるようにした（#625）。`date`
  （公開日時、ISO8601、サイトのタイムゾーン）を足した。下書きで確かめてから
  予約する、という手順が、mcp-rsのツールだけで行える。`get_post`に公開日時
  （`Date:`、予約済みなら`scheduled`）を出し、`get_posts`に`status`（`future`・
  `draft`・`any`等、カンマ区切り）を足して、予約済み・下書きの投稿も一覧
  できるようにした（既定は今までどおり公開済みだけ）

### Fixed

- `update_post`で、日付の無い`status: "future"`を断る（#625）。WordPressは、
  日付の無い予約を、すぐに公開してしまう。以前は確かめずに送っていたので、
  既存の下書きが意図せず公開される恐れがあった。既に予約済みの投稿なら、
  日付を送らずに`future`のまま更新できる

## [0.28.0] - 2026-11-18

### Added

- MCPの2026-07-28版（`initialize`を使わない版）のリクエストを、STDIOで
  受けるようにした（#570の段階1）。今までの`initialize`も、そのまま使える
  （新旧の両方を受ける）。`server/discover`に、対応する版・能力・サーバーの
  情報を返す。`params._meta`に版（`io.modelcontextprotocol/protocolVersion`）
  があるリクエストは、`initialize`無しで、そのリクエストだけその版で答える。
  版・クライアントの能力が無ければ`-32602`、対応していない版なら`-32022`
  （`data`に対応する版）。結果には`resultType`・`_meta`のサーバーの情報、
  一覧・リソースの読み取りには`ttlMs`・`cacheScope`を付ける。
  `mcp_rs_status`に、使っている版がどちらの形かと、対応する版を出す。
- HTTP（`/mcp`）でも、2026-07-28版のリクエストを受けるようにした（#570の
  段階2）。`MCP-Protocol-Version: 2026-07-28`を受け、ヘッダーと本文
  （`MCP-Protocol-Version`と`_meta`の版、`Mcp-Method`と`method`、
  `tools/call`・`resources/read`の`Mcp-Name`と`params.name`・`params.uri`）が
  合うかを確かめる。合わない・ヘッダーが無ければ400と`-32020`。`Mcp-Name`は
  `=?base64?…?=`の形でも受ける。2026-07-28のリクエストへのエラーは、知らない
  メソッドなら404と`-32601`、版・`_meta`の誤りなら400で返す。セッションは
  持たず、APIキー・OAuthによるツールの制限・監査ログはリクエストごとに効く
- `initialize`（2025-11-25以降の版）と`server/discover`の結果のサーバーの
  情報に、mcp-rsのアイコン（`icons`）を付けた（#570）。図柄はサイトの
  ファビコン（PNGの32×32・180×180とSVG）。クライアントが外のサイトに取りに
  行かないよう、`data:`のURIで埋め込む。約4KBあるので、2026-07-28の、ほかの
  結果の`_meta`のサーバーの情報には付けない。表示するかはクライアントによる
- `mcp-rs check`で、`[database]`と`[[plugins]]`も確かめる（#589）。
  起動に失敗しても、mcp-rsはそのツールが無いまま動き、クライアントは
  「ツールが見つからない」としか言わないため。`[database]`は、設定
  （`engine`・`ssl_mode`・`ssl_root_cert`・`${VAR}`）、TLSの弱い設定の警告、
  つながってログインできるか（書き込みはしない。パスワードは表示しない）。
  `[[plugins]]`は、1つずつ、起動に使うコマンド（コンテナなら`docker`等、
  Kubernetesなら`kubectl`等）が見つかるかと、起動してツールの一覧を返すか
  （確かめたら止める）。Windowsで`npx`等の`.cmd`を拡張子無しで書いていれば、
  `npx.cmd`と書くよう知らせる。接続・起動は、1つにつき10秒まで待つ。
  `--offline`では、データベースにつながず、プラグインを起動しない

### Fixed

- legacyの版（`initialize`を使う）で、`ping`に空の結果で答える（仕様の
  MUST）。以前は`-32601`だった。2026-07-28の版には`ping`が無いので、
  `-32601`のまま（#570）
- リソースが無いときのエラーを、legacyの版では`-32002`、2026-07-28の版では
  `-32602`（HTTPでは400）にした。以前は、どちらも`-32603`だった。`data`に
  そのURIを入れる（#570）

### Changed

- HTTPの`/mcp`で、対応していない`MCP-Protocol-Version`には、`-32600`では
  なく`-32022`（`data`に対応する版と、要求された版）を返す（#570）
- HTTPで、ある呼び出し元の`initialize`が、ほかの呼び出し元の版を変えない
  ようにした（#570・#596）。以前は、`/mcp`・`/ws`の全クライアントで版を
  1つ共有していた。`/mcp`の版は、リクエストの`MCP-Protocol-Version`
  ヘッダーで決まり、無ければ対応する一番古い版（2024-11-05）で答える
  （2025-06-18以降のクライアントは、ヘッダーを付ける）。`/ws`の版は、接続
  ごとに`initialize`で決まる（`/ws`は今までどおり、2026-07-28の形を使わない）

## [0.27.0] - 2026-11-17

### Changed

- `mcp-rs init`で、クライアントに今ある登録が、別の設定ファイルを使って
  いて、新しい設定ファイルを使う登録がまだ無いときは、それが置き換える前の
  設定かを聞き（2つ目のサイト用等はnで残す）、そうなら、その設定ファイルにあって新しい設定に無いもの（`[[plugins]]`・
  `[database]`・`upload_allowed_dirs`・`[audit]`等）を一覧にし、新しい設定に
  写す（既定）・その設定ファイルをそのまま使う・写さない、から選べるように
  した（#602）。v0.26.0への移行で、手で登録していた設定のプラグイン等が
  抜け落ちたため。写すときは、WordPressのユーザー名・パスワードは写さず、
  資格情報ストアに入れる。サイトのURLは、前の設定のものを既定にする。
  前の設定ファイルを使う登録は、重複として既定で消す。実行ファイルの場所が
  変わるときは、ログの新しい場所を表示する
- リリースのビルド済みのファイル（`.zip`・`.tar.gz`）に、`README.ja.md`も
  入れる（`README.md`だけだった）

### Security

- `execute_query`で、行の鍵（`FOR UPDATE`・`FOR SHARE`）を断るようにした
  （#597）。読み取りの道具なのに、ほかの書き込みを待たせられたため
  （PostgreSQLは読み取り専用のトランザクションで拒否するが、MySQLは通す
  ことがある）。副問い合わせの中も見る
- `execute_query`・`execute_command`で、データベースのサーバーのファイル・
  接続に触れる関数を断るようにした（#597）。MySQL・MariaDBの`LOAD_FILE()`・
  `sys_exec()`・`sys_eval()`、PostgreSQLの`pg_read_file()`・
  `pg_read_binary_file()`・`pg_ls_*`・`pg_stat_file()`・`lo_import()`・
  `lo_export()`・`dblink*`の関数（`FROM`の中も）、ほかの接続を切る
  `pg_terminate_backend()`・`pg_cancel_backend()`。PostgreSQLの
  `U&"…"`（Unicodeのエスケープで書いた名前。関数の名前を言い換えられる）と、
  MySQLのヒント`SET_VAR(…)`（その文の間だけ`foreign_key_checks`等を
  変える）も断る。読み取り専用の
  トランザクションでは止まらないため。権限を与えないのが本来の守りで、
  これは2つ目の守り
- MySQL・MariaDBのSQLの検査を点検し直した（#597）。`#`・空白の無い`--`・
  版付きの`/*!…*/`・入れ子のコメント・`` ` ``の中の`\`・文字列の前の
  `_utf8`等で、検査とサーバーの読み方が食い違うものは無かった（単体
  テストと、MySQL 8.4・MariaDB 11.4の結合テストで確かめる）
- HTTPモードの小さな強化（#596）
  - 認証の失敗の警告のログを、監査ログと同じく1分あたりの件数の上限まで
    にした。JWTの`kid`（署名を確かめる前のヘッダーの値）は、改行等を
    エスケープし、長さを切ってから書く（ログに偽の行を作らせない）
  - `/ws`のメッセージも、IDSで確かめる（攻撃なら閉じる）。以前は
    アップグレードのときにしか確かめなかった
  - 1分に20回認証に失敗したアドレスは、その1分の残りを429で断る
    （正しいキーでも）。16文字より短いAPIキーは、起動時に警告する
  - 使えるツールを制限した呼び出し元には、`mcp_rs_status`で、設定
    ファイル・ログの場所・ハンドラーとプラグインの状態を見せない
  - ツールの許可の確認で、一覧に無いツールは許さない（以前は許していた）。
    制限のある呼び出し元が無いツールを呼ぶと、「許可されていない」になる
  - `max_connections`のコメント・文書を直した（`/ws`の接続は、
    アップグレードの後は`websocket_max_connections`で数える）
- リリースのビルド済みのファイル（`.zip`・`.tar.gz`）に、GitHubの出所の
  証明（build provenance attestation）を付ける（#599）。`.sha256`は同じ
  リリースから取るので、壊れたダウンロードは見つかるが、リリースのファイルを
  差し替えられる人には効かない。`gh attestation verify <ファイル> -R
  n-takatsu/mcp-rs`で、このリポジトリのリリースのワークフローが作ったことを
  確かめられる
- `mcp-rs init`の小さな強化（#599）: 置き換えた設定ファイルの控えに
  平文のパスワードがあれば、消すかを聞く。前の設定ファイルに平文の
  パスワードが残っていれば、最後に知らせる。Unixでは、書く設定ファイルと
  控えを本人だけが読める（600）、フォルダを本人だけ（700）にし、
  `mcp-rs check`は、ほかのユーザーが読める設定ファイルに警告を出す。
  設定ファイルとClaude Desktopの設定ファイルは、一時ファイルに書いてから
  置き換える（途中で止まっても壊れない）。`claude`・`copilot`のコマンドは、
  `PATH`の相対パスの項目（今のフォルダ等）から探さない
- HTTPモードで、処理中のリクエストが無いまま`request_timeout_seconds`
  （既定30秒）が過ぎた接続を閉じるようにした（#592）。HTTP/2には、
  HTTP/1.1のヘッダーを待つ時間の上限にあたるものが無く、pingに答えるだけで
  リクエストを送らない接続や、何も送らない接続が、いつまでも残った。その
  ため、認証無しで`max_connections`（既定1024）を埋め、ほかのクライアントを
  つなげなくできた。時間のかかるツールの呼び出しや、大きな応答の送信中は
  閉じない
- HTTPモードで、APIキー・トークンの`@read_only`は、プラグインのツールを
  既定で含めないようにした（#595）。印（`readOnlyHint`）はプラグイン自身が
  付けるので、書き込むツールに付いていても、mcp-rsには分からないため。
  確かめたプラグインは、`[[plugins]]`に`trust_annotations = true`を書くか、
  ツールを名前で許す。AIのクライアントに見せる印は変えない（STDIOモードは
  影響しない）。互換性: `@read_only`のキーで使えていたプラグインのツールが、
  使えなくなる
- HTTPモードで、`trusted_proxies`を書いた（リバースプロキシの後ろの）ときは、
  ループバックで待ち受けていても、認証（APIキー・OAuth・mTLS）が無ければ
  起動しないようにした（#593）。同じマシンのプロキシ（nginx等）から来る
  通信は、すべてループバックからに見えるので、`127.0.0.1`で待ち受けても、
  プロキシの先（インターネット等）のクライアントがすべてのツールを使えた。
  プロキシの後ろでは、APIキーとOAuthだけを認証に数える（mTLSは、
  プロキシを確かめるだけで、その先のクライアントを確かめないため）。
  プロキシが認証するときは、今までどおり`allow_unauthenticated_external = true`
  で明示して許す。互換性: この設定で認証が無いと、起動しなくなる
- データベースの`ssl_mode = "verify-ca"`で、なりすましを防げなかった（#594）。
  `verify-ca`はホスト名を確かめないので、公開のCAを信頼すると、公開のCAが
  発行したどのホストの証明書でも受け入れてしまう
  - **PostgreSQL**: `verify-ca`は使えなくした（`PGSSLMODE=verify-ca`も）。
    sqlxは、`ssl_root_cert`があっても公開のCAを信頼するため。`verify-full`を
    使う
  - **MySQL・MariaDB**: `verify-ca`は、`ssl_root_cert`が要る（そのCAだけを
    信頼する）。無ければ、データベースのツールは起動しない
  - 互換性: これらの設定は、データベースのツールが起動しなくなる
    （`mcp_rs_status`に理由を出す）

## [0.26.0] - 2026-11-17

### Added

- インストールスクリプト（#576）。Windowsは
  `irm https://raw.githubusercontent.com/n-takatsu/mcp-rs/main/install/install.ps1 | iex`、
  macOS（Apple Silicon）・Linux（x64）は
  `curl -fsSL https://raw.githubusercontent.com/n-takatsu/mcp-rs/main/install/install.sh | sh`。
  最新（または`MCP_RS_VERSION`の版）のリリースをダウンロードし、SHA256を
  確かめて、`~/.mcp-rs/bin`（`%USERPROFILE%\.mcp-rs\bin`）に置き、最後に
  `mcp-rs init`を動かすかを聞く。管理者の権限は要らず、PATH等は変えない。
  Windowsでは、動いているmcp-rsがあっても更新できる（古いファイルを
  `mcp-rs.exe.old`に移す）。CIで、3つのOS（Windows PowerShell 5.1・
  PowerShell 7を含む）で実際のリリースを入れて確かめる
- `mcp-rs init`: 質問に答えるだけで、使い始める準備をするコマンド（#575）。
  WordPressのサイトのURL・ユーザー名・アプリケーションパスワードを聞いて
  OSの資格情報ストアに入れ、設定ファイル（既定は`~/.config/mcp-rs/config.toml`）
  を書き、`mcp-rs check`と同じ確認をし、Claude Desktop（設定ファイルに足す）・
  Claude Code（`claude mcp add --scope user`）・GitHub Copilot
  （`copilot mcp add`）に登録する。登録とファイルの置き換えは、1つずつ
  確かめてから行い、置き換えるファイルは控えを残す。断ったものは、登録の
  コマンドや設定を表示する。別の名前でmcp-rsを起動する登録（手で登録した
  `mcp-rs-wordpress`等）があれば、消すかを聞く（同じツールが2組並ばない
  ように）。同じ設定ファイルのものは既定で消し、別の設定ファイルのもの
  （2つ目のサイト用等）は、そのパスを見せて、既定では残す。最初の版は、WordPressだけを聞く

### Changed

- READMEの導入手順を短くした（#577）。インストールスクリプト → `mcp-rs init`
  → 確かめ方の3段だけにし、手で入れる手順・ソースからのビルド・クライアント
  ごとの手での設定・設定ファイル・ログファイルは、サイトの
  [Getting Started](https://n-takatsu.github.io/mcp-rs/docs/guides/)のガイドに
  移した。HTTPモードのTLS・mTLS・WebSocketの設定は、Securityの文書に移した
- `serde_json`の`preserve_order`を有効にした（#575）。Claude Desktopの設定
  ファイルに足すとき、ほかの項目の順番を変えないため。mcp-rsが返すJSONの
  キーも、並べ替えずに作った順になる（ライブラリとして使うときの
  `serde_json::Map`の順番も変わる）

### Fixed

- MySQL・MariaDBで、サーバーの`sql_mode`に`NO_BACKSLASH_ESCAPES`か
  `ANSI_QUOTES`（`sql_mode = ANSI`にも含まれる）があると、SQLの検査（`\'`を
  エスケープと、`"…"`を文字列と読む）とサーバーで文字列の区切りが食い違い、
  `SELECT … INTO OUTFILE`や`/*M! … */`の拒否をすり抜けられた（#574）。
  接続を作ったとき・プールに戻してリセットしたときに、セッションの
  `sql_mode`から、この2つと、`ANSI_QUOTES`を含むまとめたモード（`ANSI`、
  MariaDBの`DB2`・`MSSQL`・`ORACLE`・`POSTGRESQL`・`MAXDB`）を外し、サーバーの
  読み方を検査とそろえる（ほかのモードは残す）。mcp-rsの接続では、`"…"`は
  文字列になるので、名前は`` `…` ``で囲む（ツールの説明にも書いた）。既定の
  設定では起きない
- `mcp-rs check`と`wordpress_health_check`で、サインインが失敗したのに、
  権限とメディアが「OK」と出ていた（#575の確認で分かった）。どちらも認証の
  要らない読み取りでも通るため。サインインが失敗したら、権限とメディアは
  確かめない（`mcp-rs check`は「not checked」と出す）。`mcp-rs check`の
  エラーは、最後にまとめて「WordPress site」の名前で出していたのを、
  失敗した項目の名前で、確かめた順に出すようにした

## [0.25.0] - 2026-11-17

### Added

- `[database]`で、MySQL（8.0以降）を使えるようにした（`engine = "mysql"`、
  #527）。PostgreSQLと同じツール・上限・監査ログで、既定は読み取りだけ。
  パラメーターは`?`。`execute_query`は`START TRANSACTION READ ONLY`の中で
  実行してロールバックし、SELECTの時間は`MAX_EXECUTION_TIME`で、それ以外は
  別の接続からの`KILL QUERY`で止める。行数・バイト数の上限に達したら、
  残りを受け取らずに接続を閉じる。プールに戻す接続はリセットする
  （`COM_RESET_CONNECTION`）。TLSは`ssl_mode`（`allow`以外）で、
  `ssl_root_cert`を書いたときはそのCAだけを信頼する。`get_schema`は
  外部キー・インデックス・ビューも返し、`execute_command`は
  `last_insert_id`も返す。既定のビルド・配るバイナリ・Dockerイメージに
  含める（`mysql-backend`を既定の機能にした）
- `[database]`で、MariaDB（10.6以降）を使えるようにした（`engine = "mariadb"`、
  #527）。MySQLと同じ実装・上限で、SELECTの時間は`max_statement_time`で止める
  （MariaDBには`MAX_EXECUTION_TIME`が無い）。MySQLかMariaDBかは、つないだ
  サーバーの版で決めるので、`engine = "mysql"`のままMariaDBにつないでも動く。
  MariaDBの`JSON`は文字列の型なので、文字列で返す。MariaDBだけが実行する
  コメント（`/*M! … */`）は、中身を検査できないので拒否する

- リリースに、ビルド済みのmcp-rsを付ける（#564）。Windows（x64）・macOS
  （Apple Silicon）・Linux（x64、glibc 2.35以降）。タグで動く
  `release-binaries.yml`が作り、それぞれの`.sha256`と、まとめた`SHA256SUMS`
  も付ける。署名はしない。READMEに、ダウンロード・SHA256での確かめ方・
  止められたときの開き方を書き、置き場所の例を、管理者の権限の要らない
  `~/.mcp-rs/bin`（Windowsは`%USERPROFILE%\.mcp-rs\bin`）にした

- `mcp-rs check [--config <path>] [--offline]`: MCPのクライアントを使わずに、
  設定を確かめるコマンド（#565）。起動と同じ読み方で、設定ファイルが読めるか
  （誤りの行と理由）、WordPressの資格情報を読めるか、サイトにつながり認証が
  通るか（読み取りのリクエストだけ）、ログと監査ログを書けるかを表示する。
  誤りがあれば終了コード1。パスワード・トークン・ユーザー名は表示しない。
  `[database]`・`[[plugins]]`・HTTPモードの設定は、まだ確かめない。起動と
  同じく、ログ・監査ログのフォルダとファイルが無ければ作り、ログファイルに
  `checked`の見出しの行を書く

### Changed

- VS Code（GitHub Copilot）のチャットからmcp-rsを使う手順を、READMEと
  websiteのガイドに書いた（#521）。Copilot CLIに`copilot mcp add`で登録し、
  資格情報は資格情報ストアから読む。mcp-rsと関係の無い
  `.vscode/mcp.json.example`（GitHubのMCPサーバーの例）は削除した
- WordPressの資格情報が無いときのエラーで、`keyring`も選べることを示す
  ようにした

### Fixed

- `wordpress_health_check`（と`mcp-rs check`）で、サイトに届くかを、認証の
  要らない`/wp-json/`で確かめるようにした（PR #568のレビュー）。以前は
  管理者だけが読める`/wp-json/wp/v2/settings`を見ていたので、投稿者・寄稿者の
  アカウントでは、届いているのに「届かない」となり、残りの確かめを飛ばして
  いた。設定（管理者のメールアドレス等）は、読めたときだけ表示する
  `/wp-json/`は、まず認証情報を付けずに読み、401・403なら付けて読み直す
  （RESTをログインした人だけに開くサイトのため）

## [0.24.0] - 2026-11-15

### Added

- WordPressのユーザー名とアプリケーションパスワードを、OSの資格情報ストア
  （Windowsの資格情報マネージャー、macOSのキーチェーン、LinuxのSecret
  Service）から読めるようにした（#533）。`[handlers.wordpress]`に
  `keyring = "<名前>"`と書き、`mcp-rs wordpress-keyring set <名前>`で登録する
  （サイトのURLも一緒に保存し、設定の`url`と同じサイトのときだけ使う、#541）
  （`show`・`delete`もある。値は引数で受け取らず、エコー無しで読む）。
  Claude Desktopは、OSの環境変数をMCPサーバーに渡さず、設定のJSONの`env`
  だけを渡すため、Claude DesktopとClaude Code・ターミナルの両方で使うと、
  同じ値を2か所に書く必要があった。資格情報ストアは、どれから起動しても同じに
  読める。`keyring`を書いた設定では、`WORDPRESS_URL`・`WORDPRESS_USERNAME`・
  `WORDPRESS_PASSWORD`の環境変数で上書きしない（残っていた`WORDPRESS_URL`
  で、パスワードを別のサイトへ送らないため）。`username`・`password`・
  `encrypted_credentials`と一緒に書くと起動しない。平文・`${VAR}`・
  `encrypted_credentials`は今までどおり使える。既定で有効な機能
  `os-keyring`に入れた。Dockerのイメージは、資格情報ストアが無いので、
  この機能を外してビルドする
- HTTPモードのAPIキー（`[[transport.http.api_keys]]`の`key`）で、`${VAR}`
  （環境変数）を展開するようにした（#534）。キーを設定ファイルに平文で
  書かずに済む（Dockerでは`-e`等で渡す）。変数が無いか空なら、どの
  エントリーのどの変数かを出して起動しない（値は出さない）。キーを使う設定（HTTPモードで
  `auth_enabled = true`）のときだけ展開する。以前は`${...}`をそのままキーと
  して使っていたので、`${`を含むキーは環境変数から渡す必要がある

### Changed

- **互換性のない変更**: `--config`を付けずに起動したとき、今のフォルダの
  設定ファイル（`mcp-config.toml`・`config.toml`・`config/mcp.toml`）を
  読まないようにした（#541）。`~/.config/mcp-rs/config.toml`だけを読む。
  Claude Code等は、開いたフォルダでMCPサーバーを起動するので、
  `--config`無しで登録していると、開いたリポジトリの`config.toml`を読み、
  任意のコマンドを実行させたり（`[[plugins]]`）、資格情報を別のサイトへ
  送らせたりできた。今のフォルダの設定ファイルを使っていた場合は、
  `--config <絶対パス>`を付ける。設定が見つからず、今のフォルダにこれらの
  ファイルがあるときは、読まずに、絶対パスでの`--config`の付け方を案内して
  止まる。相対パスの`--config`も、起動したフォルダから読むので、起動の
  ときに標準エラーに警告を出す。`--config ~/…`は、mcp-rsがホームフォルダに
  展開する（以前は、シェルを通さないクライアントの設定では`~`がそのまま
  届き、起動したフォルダの中の`~`という名前のフォルダを探していた）
- **互換性のない変更**: HTTPモードで、ループバック以外（`0.0.0.0`等）で
  待ち受けるのに、認証（APIキー・OAuth・mTLS）が無い設定は、起動しない
  ようにした（#540）。以前は警告を出すだけで起動し、ネットワークの誰でも
  全部のツールを使えた。`auth_enabled = true`とAPIキー等を書く。mcp-rsの
  前に置いたもの（ログインを求めるリバースプロキシ等）が認証するときだけ、
  `[transport.http]`に`allow_unauthenticated_external = true`を書けば、
  警告を出して起動する
- **互換性のない変更（ライブラリ）**: `HttpConfig`に`max_connections`・
  `websocket_idle_timeout_secs`・`allow_unauthenticated_external`、
  `HttpTransportConfig`に`max_request_size`・`request_timeout_seconds`・
  `max_connections`・`websocket_idle_timeout_seconds`・
  `allow_unauthenticated_external`、`oauth::TokenInfo`に`expires_at`を
  足した（#540）。構造体を直接作っているコードは、これらを足すか、
  `..Default::default()`を使う必要がある
- **互換性のない変更（ライブラリ）**: `WordPressConfig`に`keyring`
  （`Option<String>`）を足した（#533）。構造体を直接作っているコードは、
  `keyring: None`を足す必要がある。設定の読み込みでは、
  `resolve_encrypted_credentials`の代わりに、資格情報ストアも読む
  `resolve_credentials`を呼ぶ
- **互換性のない変更（ライブラリ）**: `EnvExpansionError`に`Empty`（変数は
  あるが値が空）を足した（#534）。`match`で全部の場合を書いているコードは、
  この場合を足す必要がある
- **互換性のない変更（ライブラリ）**: `DatabaseToolOptions`に
  `max_result_bytes`、`DatabaseSettings`に`max_result_bytes`・
  `ssl_root_cert`を足した（#539）。構造体を直接作っているコードは、
  これらを足すか、`..Default::default()`を使う必要がある
- READMEとClaude Desktopのガイドに、Claude Desktopには環境変数が渡らない
  こと、`env`と設定ファイルの`${VAR}`の関係、`--config`の有無での
  `WORDPRESS_*`の扱いの違い、`enabled = true`が要ること、Windowsでの設定の
  例、設定を変えた後にClaude Desktopを終了し直すこと、起動しないときに見る
  ログを書いた（#533）。Claude Desktopの例は、ひな形の
  `configs/production/claude-desktop.toml`が読む3つの変数のうち1つしか
  `env`に書いておらず、そのとおりにすると起動しなかった。資格情報ストアを
  使う例に変え、`${VAR}`のときは3つとも要ることを書いた。
  生成する設定のひな形のコメントにあった、mcp-rsが読まない`.env`ファイルの
  案内を直した
- 最小対応のRustのバージョンを1.98にし、`Cargo.toml`に`rust-version`として
  宣言した（#516）。ソースからビルドするには、Rust 1.98以降が要る（以前は
  1.88）。Dockerのイメージも1.98でビルドする。CIで、Ubuntu・Windowsの
  1.98でのコンパイルと、`Dockerfile`のバージョンとの一致を確かめる。
  `resolver = "3"`にしたので、`cargo update`は1.98で使えない版の依存
  クレートを選ばない
- 使われていない`use`を削除し、`src/lib.rs`のクレート全体の
  `#![allow(unused_imports)]`を外した（#502）。未使用の`use`は、機能の
  組み合わせによらずコンパイラーの警告になる

### Fixed

- `configs/`の設定ファイルの例を、今のmcp-rsが読む形に直した（#557）。
  mcp-rsは知らないキーを拒否しないので、古いキーは黙って無視されていた
  - `production/claude-desktop.toml`・`main.toml`・`web-ui.toml`と
    `development/http-transport.toml`から、mcp-rsが読まないキーを消した。
    存在しない「ファイルシステムハンドラー」（`[filesystem]`・
    `[handlers] enabled`）・`[security]`・`[logging]`・`server.name`等。
    HTTPの`cors_enabled`・`timeout_ms`は、読まれる`enable_cors`・
    `request_timeout_seconds`にした。`main.toml`は、ファイルシステム
    ハンドラーだけの設定で、実際にはツールが何も無かった
  - `examples/multi-handler.toml`を、WordPress・PostgreSQL・プラグインを
    一緒に使う例に書き直した。以前は、mcp-rsが一度も読まなかった形式
    （`[handlers.<名前>]`に`type = "..."`）で、起動もしなかった
  - `examples/log-policy-demo.toml`・`module-separated.toml`の
    `[handlers.wordpress]`に、要る`enabled = true`を足し、読まれない
    `timeout`等を`timeout_seconds`にした
  - `development/testing.toml`を読めるようにした（`[transport]`が無く、
    `requests_per_second`が`10.0`で、読めなかった）
  - 無いTCPのトランスポートの`development/tcp.toml`を削除した
  - 例が今後も古くならないよう、読めて、読まれないキーが無いことを確かめる
    テスト（`tests/config_examples_test.rs`）を足した
  - 効いていなかった`[transport.stdio]`（`buffer_size`・`framing_method`
    等）を、設定と例から削除した。サーバー（`McpServer::run_stdio`）は、
    1行ずつ読み書きし、この設定を使っていなかった。書いてあっても、
    エラーにならずに無視する。**互換性のない変更（ライブラリ）**:
    `TransportConfig`（`config`と`transport`の両方）の`stdio`、
    `config::StdioTransportConfig`、`transport::stdio`（`StdioConfig`）を
    削除した
- ライブラリの`DatabaseSecurity`（`SqlInjectionDetector`）が、`1' OR '1'='1`
  （末尾の引用符を元のSQLに閉じさせる形）と`OR true`を通していたのを、
  止めるようにした（#501）。正当なサブクエリは止めない。正規表現での検知は
  見逃しも誤検知も無くせない補助であることを、ドキュメントに書いた。
  mcp-rsの`[database]`のツールはこの検知を使わないので、影響しない

### Removed

- **互換性のない変更（ライブラリ）**: 使われていないコードを削除した（#543の
  5つ目）。mcp-rs（バイナリ）の動きは変わらない
  - `McpServer::run`（と、それだけが使っていた`ServerCapabilities`・
    `ToolsCapability`・`ResourcesCapability`）。認証もOriginの検査も無い、
    生のTCPのJSON-RPCサーバーだった。STDIOは`run_stdio`、HTTPは
    `HttpTransport`と`dispatcher`を使う
  - 単独のWebSocketサーバー`transport::websocket::server`
    （`WebSocketServer`・`ServerConfig`・`EchoHandler`・`MessageHandler`等）と、
    `WebSocketTransport`のサーバーモード（`WebSocketConfig`の
    `server_mode`・`max_connections`・`anti_replay_enabled`、
    `WebSocketConfigBuilder`の`server_mode`・`max_connections`、
    `is_server_mode`）。認証もOriginの検査も無かった。MCPをWebSocketで
    受け付けるには、HTTPトランスポートの`/ws`
    （`enable_websocket_upgrade`）を使う。`WebSocketTransport`は
    クライアントとして残した
  - `StdioTransport`。サーバーは`McpServer::run_stdio`でSTDIOを読み書き
    するので、使っていなかった。設定の`StdioConfig`は残した
  - `handlers::multi`（`MultiHandlerManager`・`DefaultHandlerFactory`・
    `HandlerFactory`）と`handlers::generic`（`HandlerConfig`・
    `SecurityConfig`・`ExtendedMcpHandler`等）。`SecurityConfig`は、
    何もしていなかった
- 使われておらず、ビルドもできなかった`Dockerfile.test`と、`docker-compose.test.yml`
  の`test-runner`を削除した（#516）
- KubernetesのマニフェストとHelmチャート（`k8s/`・`charts/mcp-server/`）を
  削除した（#504）。今のmcp-rsと合わず、起動しないかプローブの失敗で再起動を
  繰り返すものだった（環境変数での設定・存在しない`/health`等のエンドポイント・
  ポート3000等）。同じ前提の`DEPLOYMENT.md`・`docs/deployment-guide.md`・
  `scripts/deploy/`・ルートの`nginx.conf`も削除した。コンテナで動かす
  方法は`docs/DOCKER_GUIDE.md`を参照

### Security

- 低い指摘のまとめ（#543）の6つ目: その他
  - **互換性のない変更**: `docker-compose.yml`のPostgreSQLを、
    `docker-compose.postgres.yml`に分けた。`POSTGRES_PASSWORD`の既定値
    （`changeme`）をやめ、設定しないとComposeが止まるようにした（`:?`）。
    以前は、設定せずに起動すると、誰でも知っているパスワードの
    データベースになった。同じファイルのままだと、PostgreSQLを使わない
    ときも`POSTGRES_PASSWORD`が要るので、分けた。PostgreSQLを使うときは、
    `-f docker-compose.yml -f docker-compose.postgres.yml`を付ける。
    `.env.example`の`POSTGRES_PASSWORD`も空にした（Composeは`.env`を
    読むので、コピーした`postgres`で起動できてしまう）。テスト環境
    （`configs/docker/docker-compose.postgres.yml`）は、空なら今までどおり
    `postgres`を使う。`docs/DOCKER_POSTGRES_GUIDE.md`は、テスト環境の
    ファイルを指すように直した。テスト環境のPostgreSQL（2つ）とpgAdminも、
    `127.0.0.1`だけに公開するようにした（既定のパスワードは`postgres`と
    `admin`）
  - `docker-compose.dev.yml`のPostgreSQLとRedisを、`127.0.0.1`だけに
    公開するようにした。以前は、同じネットワークの誰からも、ファイルに
    書いてあるパスワード（Redisはパスワード無し）で接続できた
  - `docs/DOCKER_GUIDE.md`の例で、パスワードとAPIキーを、コマンド行の
    `-e WORDPRESS_PASSWORD="…"`やクライアントの設定の`env`ではなく、
    `--env-file`のファイル（本人だけが読める）から渡すようにした。
    コマンド行の値は、シェルの履歴とプロセスの一覧に残る
  - プラグインの名前で、`_`で始まるもの・終わるものを拒否するようにした。
    例えば`fs_`のツール`fs___read`は、ほかのプラグイン`fs`のツールの指定
    （APIキーの`tools = ["fs__*"]`）にも当たった
- データベースの`float8`・`float4`のNaN・Infinity・-Infinityを、0ではなく、
  文字列`"NaN"`・`"Infinity"`・`"-Infinity"`（PostgreSQLと同じ書き方）で
  返すようにした（#543）。JSONの数では表せない。計算できなかった値が0に
  見えていた。返した文字列は、そのまま`execute_query`の引数に使える
- `get_schema`（と`db://schema`）が、文書のとおり、外部キーとインデックスを
  返すようにした（#543）。以前は、いつも空だった。複数の列の外部キーは、
  列ごとに1項目（同じ`name`）にする。インデックスの`columns`は、キーの列
  だけ（`INCLUDE`の列と、式は含めない）
- 低い指摘のまとめ（#543）の4つ目: HTTP
  - `/ws`で応答を送る時間に、上限を付けた（10秒と、64KB/秒で送る時間の
    和。10MBなら約170秒）。時間内に送れなければ、接続を閉じる。以前は、
    受信を止めたクライアントに大きな応答を送ると、送信で止まり続け、接続の
    枠を持ち続けた。その間、無通信の上限・トークンの期限も働かなかった。
    64KB/秒（約512kbps）より遅い回線では、大きな応答で接続が切れることが
    ある。Windowsでは、OSが大きな送信もすぐ受け取るので、送信では止まらず、
    読まない相手は無通信の上限で閉じられる
  - プロキシの付けるヘッダー（`X-Forwarded-Proto`・`Forwarded`・TLSの版・
    証明書のフィンガープリント・`X-Forwarded-For`・`X-Real-IP`）を、
    新しい設定`trusted_proxies`（IPアドレスかCIDR）の接続元からだけ使う
    ようにした。以前は、どの接続元のものも信じたので、平文のHTTPで
    `X-Forwarded-Proto: https`を付ければ`enforce_https`を通れ、証明書の
    ピン留めのヘッダーも書けた
  - **互換性のない変更**: `tls_enabled`無しの`enforce_https`・証明書の
    ピン留めと、`ids_trust_forwarded_for`は、`trusted_proxies`が無いと
    起動しない。プロキシのアドレスを`trusted_proxies`に書く
  - IDSで、`X-Forwarded-For`の、信頼するプロキシでないいちばん右の
    アドレスを使うようにした。以前はいちばん左（クライアントが書ける部分）
    を使ったので、ほかのアドレスをブロックさせられた
  - IDSで、呼び出し元を`X-User-ID`ヘッダーではなく、認証した呼び出し元
    （APIキー・トークン）か、接続元のIPで見分けるようにした。ヘッダーは
    クライアントが何でも書けるので、他人の名前で記録させたり、毎回違う値で
    記録を際限なく増やしたりできた
  - IDSが覚える呼び出し元・アドレス・集約したアラートの数に、上限
    （それぞれ1万）を付けた。超えると、古いものから消す。古い記録の掃除も、
    5分ごとに呼ぶようにした（以前は、掃除の関数があっても呼ばれていなかった）
  - リプレイ攻撃対策で、タイムスタンプをnonceより先に確かめるようにした。
    以前は、古いタイムスタンプのリクエストもnonceのキャッシュに入った
  - リプレイ攻撃対策のnonceのキャッシュを、呼び出し元（APIキー・トークン、
    無ければ接続元のIP）ごとに分けた。1つの呼び出し元は1万まで。以前は全員で
    1つ（10万）を使い、1つのクライアントが埋めると、全員のリクエストが
    失敗した。`trusted_proxies`のプロキシの後ろでは、`X-Forwarded-For`の
    クライアントのアドレスで分ける。nonceを覚える時間は、タイムスタンプを
    受け付ける間（既定で70秒）に縮めた（以前は5分）。nonceは要求の内容に
    結び付かない（署名しない）ので、補助的なものだと文書に書いた
  - JSONとして読めないメッセージに、JSON-RPCの`-32700`（Parse error）を、
    JSONだがリクエストでないもの（バッチ等）に`-32600`（Invalid Request）を
    返すようにした（STDIO・HTTP・`/ws`、HTTPでは400）。以前は何も返さず、
    クライアントは自分のタイムアウトまで待った。クライアントからの応答
    （`method`が無く、`result`か`error`がある）には、これまでどおり答えない
  - **互換性のない変更（ライブラリ）**: `HttpConfig`・`HttpTransportConfig`に
    `trusted_proxies`、IDSの`RequestData`に`caller_id`、
    `AntiReplayConfig`に`max_nonce_cache_per_caller`、
    `NetworkMonitorConfig`に`max_tracked_ips`、`BehavioralConfig`に
    `max_baselines`を足した。構造体を直接作っているコードは、これらを
    足すか、`..Default::default()`を使う必要がある。`NonceManager`は、
    同じnonceでも呼び出し元（`user_id`）が違えば受け付ける
- 低い指摘のまとめ（#543）の3つ目: WordPress
  - `wordpress_health_check`は、HTTPモードで認証していない呼び出し元には、
    ログのファイルのパスを出さないようにした（`mcp_rs_status`と同じ）
  - AIに使わせるWordPressのアカウントは、寄稿者（Contributor）か投稿者
    （Author）を勧めるよう、文書を直した。以前のトラブルシューティングは
    編集者（Editor）か管理者（Administrator）を勧めていた。どちらも
    `unfiltered_html`を持つので、プロンプトインジェクションで`<script>`
    入りの記事をすぐに公開できる
  - `upload_media`の`file_path`で上げたメディアは、記事に使う前から
    公開のURLで見られることを、文書に書いた
- 低い指摘のまとめ（#543）の2つ目: 暗号化した資格情報の鍵の導出
  - `encrypted_credentials`の鍵の導出（PBKDF2-HMAC-SHA256）の回数を、
    10万回から60万回（OWASPの推奨値）にした。`encrypt-wordpress-credentials`
    は、方式と回数を`kdf`・`kdf_iterations`として書き出す。記録の無いもの
    （v0.23.0までに作ったもの）は、作ったときと同じ10万回で読むので、
    そのまま使える。60万回にするには、`encrypt-wordpress-credentials`で
    作り直す
  - 導出した鍵のバイト列を、使い終わったら0で上書きする（`Zeroizing`）。
    `aes`の`zeroize`機能も有効にし、展開したAESのラウンド鍵も、捨てる
    ときに0で上書きする
  - **互換性のない変更（ライブラリ）**: `EncryptedCredentials`に`kdf`・
    `kdf_iterations`（どちらも`Option`）を足した。構造体を直接作っている
    コードは、これらを足す必要がある
- 低い指摘のまとめ（#543）の1つ目: ログと秘密情報の表示
  - 通常のログを、監査ログと同じく、Unix系では本人だけが使える権限で
    作るようにした。新しく作るフォルダは700、ファイルは600にし、ログの
    ファイルの代わりに置かれたシンボリックリンクはたどらない
    （`O_NOFOLLOW`）。以前は、umaskのとおり（多くは755・644）で、ほかの
    利用者がログを読めた。監査ログにも`O_NOFOLLOW`を付けた
  - 最後の手段の一時フォルダ（`mcp-rs/logs`）は、Windowsでだけ使う。
    Unix系の`/tmp`は全員で共有するので、ほかの利用者が先に`/tmp/mcp-rs`を
    作ると、ログを読んだり、リンクで別のファイルへ追記させたりできた
  - 設定ファイルの書き方の誤りのエラーで、誤りのある行をそのまま出さない
    ようにした。行と列の番号と、値を`…`に伏せた理由だけを出す。以前は、
    `password = "…"x`のような誤りで、パスワードの行が標準エラー（Claude
    Desktopではそのログ）に出た。値の型の誤り（`port = "…"`等）のエラーも、
    値を伏せる
  - データベースのツールの監査ログで、エラーの文の値を伏せるとき、値の中の
    引用符で伏せるのを終えないようにした。PostgreSQLは値の中の`"`をそのまま
    書くので（`"ab"cd"`）、以前は値の後半が監査ログに残った（#539で入れた
    処理）
  - `ApiKeyEntry`・`ApiKeyEntryConfig`・`ConnectionConfig`の`Debug`で、
    APIキー・パスワードを伏せた（どこもログに出してはいなかった）
- HTTPモードの受け口に、大きさ・時間・接続数の上限を付け、許していない
  クライアントに本文を読ませないようにした（#540）
  - 本文の大きさを`max_request_size`（既定1MB）までにした。超えたら413。
    以前は、この設定はWebSocketにしか使われず、HTTPはaxumの既定の2MB
    だった
  - TLSのハンドシェイクとヘッダーを受け取る時間を、
    `request_timeout_seconds`（既定30秒）までにした。本文は、データが
    届く間隔を、この秒数までにした（合計ではない）。以前は時間の制限が
    無く（`timeout_ms`はどこでも使われておらず、`axum::serve`にタイマーも
    渡していなかった）、少しずつ送る接続（slowloris）で枠を占められた。
    ツールの呼び出しにかかる時間は制限しない
  - 同時の接続を`max_connections`（既定1024）までにした。超えた接続は、
    前の接続が閉じるまで待つ
  - ネットワークの制限（`network_policy_*`）を、接続を受け付けた時点で
    確かめ、合わない接続はTLSのハンドシェイクもせずに閉じる。`Origin`と
    認証も、本文を読む前に確かめる。以前は、本文を読んでJSONとして
    解釈してから確かめていた
  - 認証の失敗を監査ログに書くのは、1分あたり100件までにした。残りは
    数えて、その1分が終わったときに1件にまとめて書く（IPアドレスは
    残らない）。以前は毎回書いたので、
    大量の失敗で監査ログがローテーションされ、本物の記録が消えた
  - `/ws`（`enable_websocket_upgrade`）: メッセージとフレームの大きさを
    `max_request_size`までにした（以前はtungsteniteの既定の64MiB・
    16MiB）。サーバーから30秒ごとにpingを送り、
    `websocket_idle_timeout_seconds`（既定300秒）の間、pongを含めて何も
    受け取らなければ閉じる。以前は、応答の無い接続も閉じなかった。
    クライアントは自動でpongを返すので、何も送らないだけのクライアントは
    今までどおり切れない。OAuthのトークンで開いた接続は、トークンの期限
    （`exp`）で閉じる（以前は接続のときにしか見なかった）
  - 同時の接続の上限は、全部のクライアントで共有し、IPアドレスごとの上限は
    無い。インターネットに公開する場合は、IPアドレスごとに接続を制限する
    リバースプロキシを前に置くことを、ドキュメントに書いた
- GitHub Actionsのワークフローで使う外部のアクションを、すべてコミットの
  SHAで固定した（#542）。版はコメントに書き、Dependabot
  （`.github/dependabot.yml`）で更新する。以前は、リリースのイメージを
  ghcr.ioへpushするジョブ（`packages: write`）で`anchore/sbom-action@v0`
  を、スキャンで`aquasecurity/trivy-action@master`を使っていて、上流の
  タグ・ブランチが乗っ取られると、イメージやリリースを上書きできた。
  `trivy-action`は、2026年3月に付け直されたタグと勧告
  （GHSA-9p44-j4g5-cfx5）の対象を避け、v0.36.0にした。あわせて次を直した
  - `deploy.yml`: SBOMの生成を別のジョブに移した。リリースへの添付に要る
    `contents: write`だけを持ち、イメージをpushする権限は持たない。
    イメージを作るジョブは`contents: read`にした
  - `docker-security.yml`: 使っていない`packages: write`を外した
    （イメージはpushしない）
  - `cargo install cargo-audit`に`--locked`を付けた
  - `docker-security-scan.yml`: 通知の`run:`の中に書いていたブランチ名
    （`${{ github.ref_name }}`）を環境変数で渡し、JSONは`jq`で作る

- データベースの`execute_query`の結果に、バイト数の上限を付けた（#539）。
  以前は行数の上限（`max_rows`）だけで、`SELECT repeat('x', 200000000)
  FROM generate_series(1, 1000)`のような、読み取り専用の正しいSELECT
  1つで、mcp-rsがメモリを使い切って落ちた（同じプロセスのWordPressの
  ツール・プラグインも止まる）。`[database]`の`max_result_bytes`（既定は
  10MB）を超えた行は返さず、`truncated`を`true`にする。1行目だけで
  超えるときは、大きな値を切り詰める（文字列は先頭を残し、`bytea`・`json`
  は大きさを示す文に置き換える）。行はカーソル（`DECLARE … CURSOR`・
  `FETCH`）で少しずつ受け取り、上限で閉じる。`FETCH`の行数は、1行から倍々に増やすので（500行まで）、上限を
  超えて作らせる行は、それまでに受け取った行数ほどで済む。上限は目安で、
  JSONのエスケープの分は数えない。結果には`max_result_bytes`も返す
- データベースの接続をプールに戻すとき、セッションの設定とロールを元に
  戻すようにした（#539）。以前は、`allow_write = true`のとき、
  `execute_command`の中の`set_config('role', …, false)`等で変えた設定が
  コミットされて接続に残り、その接続の後の`execute_query`・`get_schema`が、
  別のロールや`search_path`で動いた。`get_schema`も、読み取り専用の
  トランザクションと`statement_timeout`の中で実行する（以前は時間の上限が
  無かった）
- データベースのツールの監査ログに、パラメーターの値を書かないようにした
  （#539）。以前は、値を含むエラー（mcp-rsの型の検査の
  `parameter $1: '<値>' is not a valid uuid`と、PostgreSQLのキャストの
  エラー）を、そのまま`error`に書いていた。mcp-rsのエラーの文から値を除き、
  監査ログに書くエラーの文では、引用符の中を`…`に伏せる（クライアントには
  全文を返す）
- データベースのTLSについて、`[database]`に`ssl_root_cert`（CAのファイル）
  を足した（#539）。非公開のCA（RDS等）でも、`verify-ca`・`verify-full`を
  使える（CAのファイルは`verify-ca`・`verify-full`のときだけ使われるので、
  それより弱い`ssl_mode`と一緒に書くと、データベースのツールを起動しない）。
  ループバック・Unixソケット以外のホストで、`ssl_mode`が
  `verify-full`より弱いときは、起動時に警告する（既定の`prefer`は、
  証明書を確かめず、黙って平文に落ちる）。設定に書かないとき使われる
  環境変数（`PGSSLMODE`・`PGSSLROOTCERT`・`PGPASSWORD`・`PGOPTIONS`等）と
  `.pgpass`を、ドキュメントに書いた
- HTTPモードで、APIキー・アクセストークンごとのツールの制限（`tools`・
  `scope_tools`）を、リソース（`resources/list`・`resources/read`）にも
  効かせた（#538）。以前は、`tools = []`のキーでも、`wordpress://posts`・
  `comments`・`categories`・`tags`と`db://schema`を一覧で見て読めた。
  各リソースは、同じデータを読める読み取り専用のツールを使えるときだけ
  読める。`wordpress://posts`は投稿の本文まで返すので`get_post`、ほかは
  `get_comments`・`get_categories`・`get_tags`・`get_schema`。
  リソースの読み取りも、呼び出し元付きで監査ログに残し（以前は記録
  しなかった）、WordPressのツールと同じレート制限を通す。ライブラリでは、`McpHandler`に
  `resource_tool`（既定は`None`）を足した。自分のハンドラーのリソースを、
  制限のある呼び出し元に出すには、これを実装する
- 依存クレートを、セキュリティ勧告の修正版に上げた。`Cargo.toml`の下限も
  `tokio = "1.54"`・`ammonia = "4.1.9"`に上げたので、mcp-rsをライブラリと
  して使う側も、勧告の対象の版を選ばない
  - `ammonia` 4.1.8 → 4.1.9（RUSTSEC-2026-0533: 埋め込みのSVG・MathML
    の扱いの誤りによる、変異型XSS）
  - `quinn-udp` 0.5.14 → 0.5.16（RUSTSEC-2026-0538: 切り詰められた
    データグラムの後、受信ループが空回りする）
  - `tokio` 1.48.0 → 1.54.1（RUSTSEC-2026-0386）、`rustix` 1.1.2 → 1.1.6
    （RUSTSEC-2026-0486）。どちらも健全性（unsound）の警告
  - `lru` 0.16.4（RUSTSEC-2026-0253）は、`mysql_async` 0.36が0.16を
    要求するため、まだ上げられない

## [0.23.0] - 2026-11-12

### Added

- データベースのハンドラーを本体につないだ（#453、PostgreSQL）。
  設定ファイルの`[database]`に接続先を書くと、`execute_query`（SELECT）・
  `get_schema`のツールが使える。`allow_write = true`のときだけ
  `execute_command`（INSERT・UPDATE・DELETE）も出す。既定のビルド
  （`cargo build --release`）とDockerのイメージに含む
  （`--no-default-features`で除く）
  - `password`等は`${VAR}`（環境変数）で渡せる。ログ・`mcp_rs_status`・
    エラーの文には出さない。環境変数が無い場合も、データベースだけを飛ばし、
    mcp-rs（WordPress等）は起動する
  - 文字列とNULLのパラメーターは、PostgreSQLが期待する型（uuid・日付・
    時刻・numeric・JSON・base64のbytea・数値等）に合わせて送る。結果で
    返した値を、そのままパラメーターに戻せる。text・列挙型にはそのまま
    送る。それ以外の変換しない型（interval・money・inet・配列等）には送らず、
    `$1::text::interval`のようにtextを経由するキャストを案内するエラーに
    する（形式の違うバイト列を送ると、別の値として保存されうるため）
  - つながらなければ、プラグインと同じく警告を出して飛ばし、理由を
    `mcp_rs_status`の「Handlers and plugins that failed to start」に出す
    （以前の「Plugins that failed to start」）。接続先は`Handlers`の行に出す
  - ツールの呼び出しは、SQL（長さを切る）と結果を監査ログに記録する。
    パラメーターの値は記録しない
  - `execute_query`・`get_schema`は`readOnlyHint`、`execute_command`は
    `destructiveHint`を持つ（HTTPモードの`@read_only`等で絞れる）

### Changed

- `database`機能を既定で有効にし、MySQLのエンジン（`mysql-backend`）を
  含めないようにした（#453）。ライブラリとしてMySQLのエンジンを使う場合は、
  `--features mysql-backend`を指定する（`database`も有効になる）
- データベースのドキュメント（websiteの`docs/database.md`・
  `docs/guides/postgres-integration.md`・`docs/api/database.md`、README）を、
  実際の`[database]`の書き方・ツール・安全のための仕組みに合わせて書き直した。
  読み取り専用のロールで接続することを勧める（読み取り専用のトランザクション
  では、`dblink`等の拡張が別の接続で書き込むのは防げないため）。
  `configs/database/postgres.toml`も、そのまま使える例にした（以前の例は
  `engine`が無く、起動しなかった）

- README（英・日）とwebsiteを、実際の機能に合わせて書き直した（#459）。
  「Production-Ready」「7-Layer（100%）」「260+ tests」等の実態と合わない
  表現、本体が使っていないRBACの節、存在しない機能（リアルタイム共同編集・
  複数エンジンのデータベース等）の説明を除いた。websiteの
  Documentation・Getting Started・Architecture・Securityの各ページと、
  Claude Desktop統合ガイドを書き直し、READMEのドキュメントの一覧の
  切れたリンク（`project-docs/quick-start.md`等）を直した
- 実態と合わないドキュメントを削除した（#459）: websiteの
  `landing-page.md`・`demo-page.md`・`marketing-content.md`、
  `project-docs/`のClaude Desktop・Claude Code統合ガイド以外、
  `docs/PRODUCTION_DEPLOYMENT_GUIDE.md`・`docs/CONTAINER_INTEGRATION_TESTS.md`・
  `docs/PHASE3_COMPLETION.md`
- ビルドされない古いコードを削除した（#459）: `examples.disabled/`・
  `tests.disabled/`・`web-ui/`・`static/demo.html`・ルートの`index.html`。
  WordPressにつながずに成功を表示するだけだったexample
  `wordpress_blog_service_demo`も削除した。
  dev containerの作成時に`web-ui`で`npm install`を実行しないようにし、
  存在しないWeb UI（8080番）の案内を削除した
- クレート全体の`#![allow(dead_code)]`を外し、使われていない関数とフィールドを
  削除した（#459）。CIのclippyも`-A dead_code`を外し、使われないコードを
  警告にする。受け取った設定・引数を使っていないフィールドは、実装するか
  削除するかを#499で決めるため、`#[allow(dead_code)]`を付けて残した
- `PostgresEngine::begin_transaction`のドキュメントの例が、コンパイル
  できなかったのを直した（#459）
- 受け取った設定・引数を使っていなかった箇所を削除した（#499）。効いて
  いるように見えて実際には効いていなかったもので、動作は変わらない。
  ライブラリのAPIが変わる
  - `PasswordHasher::new(salt_rounds)`を`new()`にした。`salt_rounds`は
    bcryptの考え方で、Argon2には使われていなかった。ハッシュは従来どおり
    Argon2id（メモリー19 MiB・反復2回・並列度1）
  - `DatabaseHandler::new(threat_intelligence)`を`new()`にした。作って
    いた`DatabaseSecurity`（正規表現でのSQLインジェクション検知等）と
    `SafetyManager`は、クエリの実行で使っていなかった。`[database]`の
    安全性は、従来どおり読み取り専用のロール・文の種類の検査・
    `allow_write`による
  - `MySqlEngine::new(config, security)`・`MariaDbEngine::new(config,
    security)`を`new(config)`にした。受け取った`DatabaseSecurity`は
    使っていなかった。同じ動きの`new_without_security`を、PostgreSQL・
    SQLiteのものも含めて削除した
  - `IntegratedSecurityManager`は、`encryption.encrypted_columns`から
    カラム暗号化を作るが使っていなかったので、作らないようにした。
    カラム暗号化は`DatabaseHandler::with_column_encryption`で使う
  - `MongoEngine::new_with_config(config, mongo_config)`を
    `new_with_config(mongo_config)`に、`SwitchOrchestrator::new`・
    `EngineMonitor::new`から使われない引数を除いた
  - 何もせずに成功を返すだけだった`handlers::database::availability`
    （ヘルスチェック・復旧・フェイルオーバー）を削除した。どこからも
    使われていなかった
  - ほかに、書くだけで読まれないキャッシュ・フィールドを削除した
    （RBACの権限のキャッシュ、IDSのリクエスト履歴のパス・サイズ・IP等）

### Security

- データベースのハンドラー（`database`機能）の`execute_query`で、データを
  変えられないようにした（#453）。以前は、`WITH d AS (DELETE … RETURNING *)
  SELECT …`をSELECTとして通していた。`WITH`の中の書き込みと`SELECT … INTO`
  を拒否し、さらにPostgreSQLでは毎回、読み取り専用のトランザクション
  （`SET TRANSACTION READ ONLY`）の中で実行してロールバックする（書き込む
  関数を呼ぶSELECTも、データベースが拒否する）
- PostgreSQLのパラメーターを、SQLの文字列に埋め込まずバインドするように
  した（#453）。以前は文字列の置き換えで、`$1`が`$10`の中にも当たった。
  パスワードもURLに埋め込まないので、記号を含んでいてもよい
- `execute_command`は、1つのINSERT・UPDATE・DELETEだけを受け付ける
  （DDL・`TRUNCATE`・権限の変更等は拒否する）
- `execute_query`で取ったセッション単位の勧告ロック（`pg_advisory_lock`）
  が、ロールバックの後も接続に残らないよう、接続をプールに戻すときに外す
  （#453）。読み取り専用のトランザクションでは防げない副作用のため

### Fixed

- Dockerのイメージが既定の引数で起動しなかったのを直した（#497）。既定の
  引数が存在しないオプション`--transport stdio`だったため、すぐに終了して
  いた。既定の引数を`--config /app/config/mcp-config.toml`にし、設定
  ファイルをマウントして使う。ビルドはnightlyをやめてRust 1.88にし、
  読まれない`MCP_SERVER_PORT`・古い版のラベル・使わないポート（3000・
  3001）を除いた。`docker-compose.yml`は設定ファイルのマウント・ポート
  8081にし、ログ・監査ログを名前付きボリューム（`/home/nonroot`）に
  残すようにし、動かないヘルスチェック（distrolessにシェルが無い）と
  使わないRedisを除いた。`docs/DOCKER_GUIDE.md`を実際の
  使い方（STDIO・HTTP）に書き直した。CIでイメージの起動とMCPの応答を
  確かめる
- データベースのハンドラー（#453）: PostgreSQLの接続のたびに新しい接続の
  プールを作っていたのを、1つのプールを共有するようにした。NUMERIC・UUID・
  日付等の値を黙ってNULLにしていたのを、型に合わせて読む（NUMERICは桁を
  落とさず文字列）ようにし、読めない型はそう分かる文字列にした。行が無い
  結果でも列の名前と型を返す。1つのSQLの時間の上限（既定30秒）と、返す
  行数の上限（既定1000行、超えたら`truncated`）を設けた。エンジンが無い
  ときにパニックしていたのを、エラーにした
- データベースのハンドラーの、動かなかったツール（`begin_transaction`は
  コミット・ロールバックの手段が無かった）と、1つのデータベースでは意味の
  無いツール（`list_engines`・`switch_engine`）を出さないようにした。
  ツールの結果と`db://schema`のリソースを、MCPの形（`content`・`contents`）
  で返すようにした

## [0.22.0] - 2026-11-11

### Added

- MCPのプロトコルの版`2025-11-25`に対応した（#475）。クライアントが
  `2025-11-25`を要求すれば（Claude Desktop等）その版で答え、対応していない
  版には`2025-11-25`で答える。`2025-06-18`・`2024-11-05`を要求する
  クライアントには、これまでどおりその版で答える
  - `2025-11-25`のときだけ、`initialize`の`serverInfo`に`description`と
    `websiteUrl`を付ける
  - プラグインにも`2025-11-25`を要求する。古い版で答えるプラグインも、
    これまでどおり使える
  - `2025-11-25`で加わった任意の機能（tasks・URLのelicitation・sampling
    でのツール・アイコン）は使わない。ツールの引数の誤りを
    `isError: true`の結果で返すこと（SEP-1303）、OAuthの`insufficient_scope`
    と`scope`（SEP-835）、Protected Resource Metadata（RFC 9728、SEP-985）は、
    既に仕様どおり
- HTTPモードで、`initialize`の後のリクエストの`MCP-Protocol-Version`
  ヘッダー（Streamable HTTP）を読むようにした（#484）。そのリクエストは
  ヘッダーの版で答える（以前は、直近の`initialize`の版をすべての
  クライアントに使っていた）。対応していない版ならHTTP 400と
  JSON-RPCのエラー（`-32600`）で答える。ヘッダーが無いリクエストと`/ws`は、
  これまでどおり直近の`initialize`の版

### Security

- HTTPモードで、ブラウザーが付ける`Origin`ヘッダーを検証するようにした
  （#484）。許可していないオリジンからの`/mcp`・`/`・`/ws`のリクエストには
  HTTP 403で答える。訪れたウェブページが、ブラウザー経由で（DNS
  rebindingを含む）mcp-rsのツールを呼べないようにする（MCPの仕様の
  MUST）。受け付けるのは、`Origin`が無いリクエスト（Claude Desktop等の
  AIクライアント）、ループバックのオリジン（`localhost`・`127.0.0.1`・
  `[::1]`）、`[transport.http] allowed_origins`に書いたオリジン（`"*"`で
  すべて）。Protected Resource Metadataは、これまでどおりどのオリジンにも
  返す
  - `Origin`の検証は、本文を読む前のミドルウェアで行う（#488）。許可して
    いないオリジンには、本文が壊れていても403で答える
  - `/`・`/mcp`・`/ws`のCORSも、同じ規則で許可するオリジンを決める
    （#488）。以前の`cors_enabled`（既定はtrue）はすべてのオリジンを許可して
    いた。許可していないオリジンは、ブラウザーのpreflightで止まる
  - ループバックのオリジンは常に受け付けるので、認証を設定していなければ、
    同じPCのウェブページ（別のポートも）からは呼べる。そのことと、防ぐには
    APIキーかOAuthを設定することを、websiteのセキュリティの節に書いた
  - **互換性のない変更（ライブラリ）**: `HttpConfig`と
    `HttpTransportConfig`に`allowed_origins`を加えた。構造体を
    `..Default::default()`無しで作っている場合は、項目を加える。
    `JsonRpcRequestHandler`には、既定の実装がある
    `handle_jsonrpc_with_protocol_version`を加えた（実装している側の変更は
    要らない）
- Unix系（Linux・macOS）で、監査ログのフォルダを本人だけが使える権限
  （`700`）、ファイルを本人だけが読み書きできる権限（`600`）で作るように
  した（#489）。以前はumaskに従い、多くの環境で他のユーザーも読めた
  （監査ログには利用者名・IPアドレスが入る）。ローテーションの後の
  ファイルも同じ。既にあるフォルダ・ファイルの権限は変えない（以前の
  バージョンが作ったものは`chmod`で絞る）。Windowsと通常のログは変えない

### Changed

- **既定の保存先の変更**: 監査ログの既定の保存先を、設定ファイルと同じ
  フォルダの`audit.log`から、通常のログと同じユーザー領域の
  `audit\audit.log`にした（#474）。Windowsは
  `%USERPROFILE%\.mcp-rs\audit\audit.log`、それ以外は
  `$XDG_STATE_HOME/mcp-rs/audit/audit.log`か
  `~/.local/state/mcp-rs/audit/audit.log`。Claude Desktopの案内どおりの
  設定では、以前は`%APPDATA%\Claude\mcp-rs\audit.log`（Claudeのパッケージの
  フォルダの中）になり、Claude Desktop上のClaudeから読めなかった。
  通常のログ（`.mcp-rs\logs`）と並ぶので、`instance`で突き合わせやすい
  - 設定ファイルを使わない場合も、この場所に書く（以前はメモリーだけ）
  - `[audit] file_path`を書いていれば、これまでどおりその場所。相対パスの
    基準も、これまでどおり設定ファイルのフォルダ
  - 以前の場所の`audit.log`は移さない。残っていれば、起動時のログにその
    場所を出す。以前の場所を使い続けるなら`file_path = "audit.log"`
- Production Deployment（`deploy.yml`）の名前を「Release Image」にし、中身が
  `echo`だけだったデプロイのジョブ（Deploy to Staging・Deploy to
  Production・Rollback Deployment）を削除した（#451）。デプロイ先は無く、
  タグのpushでは`deploy-production`が動かない設定になっていた。検証・
  イメージのビルドとpush・SBOM・Trivyのスキャンは変わらない。手動の実行
  （`workflow_dispatch`）の環境の選択も無くした。説明
  （`DEPLOYMENT.md`・`docs/deployment-guide.md`）を実態に合わせ、
  使っていないSecrets（`DEPLOY_SSH_KEY`等）の記述を消した
- Release Imageのイメージの`latest`のタグを、`vX.Y.Z`タグのときだけ付ける
  ようにした（#482）。以前は、既定のブランチ（`develop`）を選んで手動で
  実行すると、リリース前のイメージで`latest`を上書きした。タグのpushで
  付く`latest`は変わらない
- GitHubのcode scanningに結果を送る`github/codeql-action/upload-sarif`を
  v3からv4にした（#479）。v3は2026年12月に非推奨になる

## [0.21.0] - 2026-11-11

### Added

- 監査ログの各記録に、記録したプロセスを示す`pid`と`instance`を加えた
  （`metadata`、#468）。`instance`はプロセスの番号と起動時刻
  （例: `70580-20261110T130333`）で、通常のログの起動時の見出しの行
  （`=== mcp-rs … / pid … / instance … / log: … ===`）と`mcp_rs_status`
  （`Instance`の行）にも同じ値を出す。Claude Desktopがmcp-rsを2つ起動する
  場合や、Windowsがpidを使い回した場合でも、どのプロセスがツールの呼び出しを
  処理したかを、2つのログで突き合わせられる
  - GDPR等の削除の依頼で記録を消す場合も、`pid`と`instance`は残す
    （利用者の情報ではないため）
  - 起動時の見出しの行の`started at`と`instance`は、同じ1つの起動時刻から
    作る
- 動き続けているmcp-rsが日付でログのファイルを切り替えたとき、新しい
  ファイルの先頭に見出しの行（`=== mcp-rs … continued at … / pid … /
  instance … / log: … ===`）を書くようにした（#472）。起動時の見出しの行は
  起動したファイルにしか無いので、以前は前の日のファイルまでさかのぼらないと
  `instance`が分からなかった。この行と切り替えのきっかけの1件のログは、
  1回でまとめて書く（別のプロセスの行と混ざらない）
- 監査ログの`timestamp`はUTC（通常のログはローカル時刻）であることを、
  websiteに書いた

### Documentation

- Claude Desktopが、1つの登録に対してmcp-rsを2つのプロセスとして起動する
  ことがあり、ログが二重に見えることを、README・websiteに書いた（#465）。
  `[pid …]`でプロセスを区別できること、起動時の記録が2回ずつ出るのは
  正常であることを説明した。mcp-rsの動作は変わらない

### Changed

- **互換性のない変更（ライブラリ）**: `AuditLogEntry::metadata`の型を
  `HashMap<String, String>`から`BTreeMap<String, String>`にした（#468）。
  監査ログの`metadata`のキーが、アルファベット順に並ぶ（以前は行ごとに
  順序が変わった）。`mcp-rs`バイナリの使い方と、監査ログのJSONの形
  （キーと値）は変わらない。ライブラリとして`metadata`を`HashMap`として
  受け取っている場合は、`BTreeMap`に変える
- ログのファイル名の日付と、新しいファイルに切り替える時刻を、UTCから
  ローカル時刻にした（#464）。ログの中の時刻（ローカル時刻）と、ファイル名の
  日付がずれなくなる。日本では、以前は毎朝9:00に切り替わり、
  `mcp-rs.log.2026-11-09`に11/9 9:00〜11/10 8:59の記録が入っていた。
  ファイル名の形式（`mcp-rs.log.YYYY-MM-DD`）と、ログの出力先は変わらない
  - tracing-appenderのローテーション（UTCだけ）をやめ、ローカル時刻で
    切り替える小さなwriterに置き換えた。切り替えの判定は書き込みのたびに
    行い、時計が戻っても前の日のファイルには戻らない。1件のログは1回で
    書くので、同じファイルに書く2つのプロセス（Claude Desktop）の行は
    混ざらない
  - `mcp_rs_status`のログの行は、writerが実際に書いているファイルを表示する
  - **更新した日の注意**: 更新する前に書かれた記録は、以前のとおりUTCの
    日付の名前のファイルにある。日本では、更新した日の0:00〜8:59の記録が、
    前の日の日付の名前のファイルに入っていることがある

### Security

- jsonwebtoken を10.4.0から11.1.0に上げ、暗号の実装を`rust_crypto`から
  `aws_lc_rs`に変えた（#450）。HTTPモードのOAuthのアクセストークンの検証に
  使う。11.xの`rust_crypto`は、まだp256・p384 0.13（crypto-bigint 0.5.5）と
  rsa 0.9を使うため、上げるだけでは脆弱性の警告が消えなかった。`aws_lc_rs`は
  rustls（HTTPS）が既に使っている実装で、新しい暗号のライブラリは増えない
  - crypto-bigint 0.5.5・p256・p384・rsaがビルドから無くなり、
    RUSTSEC-2026-0387（crypto-bigint）の無視を`.cargo/audit.toml`から外した
  - RUSTSEC-2023-0071（rsa）の無視は残す。rsaは、ビルドしていない
    sqlx-mysqlの依存として`Cargo.lock`に載っているだけになった
  - 検証できる署名の方式（RS256・PS256・ES256・EdDSA等）は変わらない

## [0.20.0] - 2026-11-10

### Removed

- 本体（`mcp-rs`バイナリ）と、データベースのハンドラー（`database`機能）の
  どちらからも使われていないモジュールを削除した（#448、約6.1万行）。
  動作は変わらない
  - 最上位: `ai`・`analytics`・`canary_deployment`・`cli`・`llm`・
    `mcp_server`・`monitoring`・`operator`（`mcp-operator`バイナリと
    `Dockerfile.operator`を含む）・`optimization`・`policy`・
    `policy_application`・`policy_config`・`policy_validation`・
    `policy_watcher`・`protocol`・`rollback`・`server`・`session`・
    `threat_intelligence`・`dashboard`
  - `security`: `audit`・`image_scanner`・`mtls`・`secure_server`・
    `sql_injection_protection`・`tls_enforcement`・`validation`・`waf`・
    `xss_protection`・`ids::ml`。HTTPモードのmTLSは`transport::http`に
    あり、変わらない
  - `transport`: `dynamic`・`factory`、`websocket`の`balancer`・
    `compression`・`failover`・`llm_bridge`・`metrics`・`rate_limit`・
    `transfer`
  - `zero_trust`: `continuous_auth`・`device_verifier`・`network_analyzer`
    （`micro_segmentation`はデータベースのハンドラーが使うので残した）
  - どこからも読み込まれていなかった`src/bin/performance_test_execution_analysis.rs`
    （mcp-rsを使わない単独のプログラム）と
    `handlers/database/integrated_availability.rs`
  - これらを使っていたテスト・examples・ベンチ・スクリプト
  - **ライブラリの変更**: 上のモジュールと、`mcp_rs::McpServer`・
    `mcp_rs::McpProtocol`・`mcp_rs::Session`等（`session`の再公開）を
    削除した。MCPのサーバーは`mcp_rs::mcp::McpServer`を使う
- 使わなくなった機能（features）と依存関係を削除した（#448）。依存する
  クレートは725個から591個に減った
  - 機能: `session-management`・`security`・`websocket`・`mcp-protocol`・
    `metrics`・`testing`・`simd`・`kubernetes-operator`・
    `ml-anomaly-detection`・`llm-integration`・`full`。既定の機能（`default`）
    は空にした（どれもコードから参照されていなかった）。`--features`で
    これらを指定している場合は、外す
  - 依存関係: `async-openai`・`kube`系・`smartcore`等の機械学習・
    `prometheus`・`metrics`・圧縮（`flate2`・`brotli`・`zstd`）・
    `tokio-postgres`・`clap`・`governor`等、コードから使われていない
    もの
  - WebSocketのクライアント（`wss://`）のTLSを、native-tls（Linuxでは
    OpenSSL）からrustlsに変えた。サーバー側と同じTLSになり、OpenSSLに
    依存しなくなった
  - `.cargo/audit.toml`から、該当する依存関係が無くなった無視
    （RUSTSEC-2025-0012・RUSTSEC-2024-0384・RUSTSEC-2024-0436・
    RUSTSEC-2026-0173）を外した
- 削除した機能のドキュメント（脅威インテリジェンス・LLM・機械学習・監視・
  セッション・Zero Trust・WAF・ポリシー・MFA・Kubernetes operator等）と、
  operatorのHelmチャート（`charts/mcp-operator`）・CRD・マニフェスト
  （`k8s/crds`・`k8s/manifests`）、ポリシーのサンプル（`demo-policies`）を
  削除した。README・websiteの、削除した機能の記述を直した

### Fixed

- 既定の機能でベンチをビルドすると、`benches/postgres_jsonb_benchmarks.rs`が
  コンパイルできなかった（#448）

- 設定ファイルの`${VAR}`（環境変数）の展開の問題を直した（#454）
  - 展開した値の先頭20バイトをdebugのログに出していた。プラグインの`env`
    （APIキー等）や設定の再読み込みでは、ログのレベルがdebugだとログ
    ファイルに残った。値は出さず、変数名だけにした。値に日本語等が含まれる
    とpanicすることがあったのも、あわせて直った
  - WordPressを有効にした設定で、参照した環境変数が無い場合と、`${`に
    対応する`}`が無い場合は、起動しないようにした。どの項目のどの変数かを
    出す（値は出さない）。以前は`[ERROR:VAR]`等の文字列をパスワードにして
    起動し、後でWordPressの認証が分かりにくい形で失敗していた。WordPressを
    有効にしていない設定では、これまでどおり起動を止めない
  - 置き換えた値の中の`${...}`は、もう一度は展開しないようにした
    （パスワードに`${`が含まれていても、そのまま使う）
  - プラグインの`env`で`${`の書き方がおかしい値は、渡さないようにした
    （以前は`[INVALID_ENV_VAR]`を含む文字列を渡していた）
  - **ライブラリの変更**: `McpConfig::expand_env_vars`は
    `Result<String, EnvExpansionError>`を、`McpConfig::expand_wordpress_config`は
    `Result<(), ConfigEnvError>`を返す

### Changed

- `C:\Program Files\MCP-rs`等、実行ファイルのフォルダに書き込めない場所に
  インストールした場合、ログの出力先がユーザーのフォルダ
  （`%USERPROFILE%\.mcp-rs\logs`等）になるのを、フォールバックとして
  扱わないようにした（#441）。以前は起動のたびに、`mcp_rs_status`と
  起動時の表示にフォールバックの理由が出ていた。ログの出力先は変わらない。
  ログの出力先の決まり方と、ファイル名の日付がUTCであることを、README・
  websiteに書いた

- Production Deployment（`deploy.yml`）を、同時には1つだけ実行するようにした
  （#445）。`main`へのpushとタグのpushが続いても、後の実行は前の実行が
  終わるのを待つ。共有のイメージのタグ（`main`・`latest`・`0.19`等）を
  並行して上書きしない
- `mcp-rs`バイナリ（`src/main.rs`）が、ライブラリと同じモジュール
  （`config`・`handlers`・`security`・`transport`等）を`mod`で読み込み直して
  いたのをやめた（#448）。バイナリはライブラリ（`mcp_rs::`）だけを使う。
  同じコードを2回コンパイルしなくなり、ビルドが速くなる。動作は変わらない

### Security

- openssl 0.10.75の脆弱性（RUSTSEC-2026-0463、`aes::unwrap_key`の範囲外
  書き込み）を、0.10.82への更新で解消（#455）。mcp-rsは`aes::unwrap_key`を
  直接使っていない。その後、WebSocketのクライアントをrustlsに変えたため、
  opensslはもう使っていない（#448）
- 依存関係を互換の範囲で更新した（#448）: mongodb 3.6.0→3.10.0
  （hickory-proto・hickory-resolver 0.25.2→0.26.3）、serde_with
  3.16.1→3.24.0、rsa 0.9.9→0.9.10、rand 0.8.5→0.8.8・0.9.2→0.9.5。
  hickory（DNS）・serde_with等の既知の脆弱性の警告（Trivy）を解消し、
  不要になった`.cargo/audit.toml`の無視（RUSTSEC-2026-0118・
  RUSTSEC-2026-0119・RUSTSEC-2026-0097）を外した
- RUSTSEC-2026-0459（`libsqlite3-sys` 0.30.1に同梱のSQLite、
  CVE-2026-119375）を、`.cargo/audit.toml`で理由を付けて無視（#444）。
  修正版（0.39.0以降）を使えるsqlxが、まだ無い（0.9.0も0.38.0未満）。
  信頼できない人が書いたデータベースとWALのファイルを開いた場合だけの
  問題で、既定のビルドはSQLiteを開かない（SQLiteは任意の`database`
  機能だけ）。sqlxが対応したら外す（#449）

## [0.19.2] - 2026-11-09

### Added

- mcp-rs自身の状態を返す、読み取り専用のツール`mcp_rs_status`を追加した
  （#434）。WordPress・プラグインの有無にかかわらず、常に使える。AI
  （Claude Desktop・Claude Code等）が、ツールを呼ぶだけでバージョン・動作
  モード・プロトコル版・設定ファイル・ログと監査ログの書き込み先・各
  ハンドラーとプラグインの状態（起動できなかった理由を含む）を確かめられる
  - パスワード・キー・トークン・環境変数の値は含めない。HTTPモードで認証して
    いない呼び出し元には、バージョンだけを返す
  - 呼び出しは監査ログに記録する。HTTPモードでは`tools`（#52）で制限できる
  - `wordpress_health_check`の出力にも、バージョンの行を加えた
  - **ライブラリの変更**: `McpHandler`に`status`（既定の実装あり）、
    `McpServer::set_status_info`・`mcp::StatusInfo`・
    `mcp::process_started_at`、
    `plugin_proxy::start_plugins_reporting_failures`を追加。`tools/list`の
    先頭に`mcp_rs_status`が入る

### Changed

- 知らない引数と、パスの無い`--config`があると、起動しないようにした（#437）。
  どの引数がおかしいかと使い方を標準エラー出力に出し、終了コード2で終わる。
  v0.19.1では、ログに警告を出して無視していた。打ち間違い（`--confg`等）に
  気づかず、既定の場所の設定ファイルで起動してしまうのを防ぐ
  - **ライブラリの変更**: `cli_args::RunArgs`から`unknown`・
    `missing_config_value`を外し、`cli_args::Command::Invalid`を追加した

## [0.19.1] - 2026-11-08

### Added

- `mcp-rs --version`（`-V`）でバージョンを、`--help`（`-h`）で使い方を表示する
  ようにした（#431）。サーバーは起動せず、設定ファイル・ログにも触れない。
  以前は`--version`を無視してサーバーとして起動し、入力を待ち続けていた
  - `--config=<path>`の書き方も受け付ける
  - 知らない引数と、パスの無い`--config`は、ログに警告を出して無視する
    （これまでどおり起動する）。将来のバージョンでは、知らない引数があると
    起動しないようにする

## [0.19.0] - 2026-11-08

### Added

- 監査ログに、呼び出し元と結果を記録するようにした（#55）
  - ツール呼び出しの記録に、呼び出し元（`user_id`: `api_key:<name>`・
    `oauth:<client>`・STDIOは`local`）、利用者（`subject`: APIキーの
    `user_id`・トークンの`sub`）、IPアドレス、所要時間（`duration_ms`）を
    加えた
  - WordPressの読み取りのツールも記録する（以前は変更するツールだけ）
  - HTTPモードの認証の失敗（キー・トークンが無い・無効・スコープ不足）を
    記録する。キー・トークン・ツールの引数は記録しない
  - **ライブラリの変更**: `mcp::Caller`に`subject`・`ip_address`を追加。
    `HttpTransport::with_audit_logger`、`McpServer::audit_logger`、
    `mcp::with_caller`・`mcp::current_caller`・`mcp::audit_tool_call_context`
    を追加

- HTTPモードで、APIキー・アクセストークンごとに使えるツールを制限できるように
  した（#52）
  - APIキーの`tools`、OAuthの`scope_tools`（スコープ → ツール）で指定する。
    ツール名、`*`を使うパターン（`fs__*`等）、`@read_only`（annotationsで
    読み取り専用のツール）が使える。省略時は、これまでどおりすべて使える
  - 使えないツールは`tools/list`に出さず、`tools/call`はツールのエラーにする。
    拒否した呼び出しは監査ログに記録する
  - **ライブラリの変更**: `transport::http::JsonRpcRequestHandler::handle_jsonrpc`
    に、呼び出し元（`mcp::Caller`）の引数を追加した。`ApiKeyEntry`に`tools`、
    `OAuthConfig`に`scope_tools`を追加。ツールを制限する設定で、
    `with_request_handler`を使わずに`start_server`するとエラーになる。
    `mcp::Caller`・`mcp::ToolAccess`、`RequestDispatcher::handle_message_as`、
    `McpServer::set_audit_logger`を追加

- HTTPモードで、OAuth 2.1のアクセストークン（JWT）を受け付けるようにした
  （#420、`[transport.http.oauth]`）。MCPの仕様（2025-06-18の
  「Authorization」）のリソースサーバーとして動き、ログインは外部の認可
  サーバー（Keycloak・Okta・Microsoft Entra ID等）が受け持つ
  - 署名（認可サーバーのJWKS）・発行者・有効期限・対象（`aud`）を確かめる。
    共有鍵（HS256等）・署名なしのトークンは受け付けない
  - Protected Resource Metadata（RFC 9728）を公開し、401・403に
    `WWW-Authenticate`を付ける
  - `required_scopes`で必要なスコープを指定できる
  - APIキー（`auth_enabled`）と併用できる

- MCPのプロトコル版`2025-06-18`に対応した（#417）
  - 版の交渉: クライアントが要求した版（`2025-06-18`・`2024-11-05`）で答える。
    それ以外は`2025-06-18`で答える。以前は常に`2024-11-05`で答えていた
  - ツール定義の`title`・`outputSchema`・`annotations`と、ツールの結果の
    `structuredContent`を中継する（`2025-06-18`で話すクライアントにだけ。
    `2024-11-05`では除き、リソースリンク・音声のコンテンツはテキストに
    置き換える）。プラグインには`2025-06-18`を要求する
  - WordPressのツールにannotations（読み取り専用・破壊的・冪等）を付けた。
    クライアントが、実行前に確認するか等を決めるのに使う
  - **ライブラリの変更**: `mcp::Tool`に`title`・`output_schema`・
    `annotations`を追加し、`Default`を実装した（構造体リテラルには
    `..Default::default()`が要る）。`mcp::version`（版の交渉・項目の
    取捨）と`mcp::Session`を追加

- 監査ログの書き込み先を、設定の`[audit]`で指定できるようにした（#416）。
  省略時は、設定ファイルと同じフォルダの`audit.log`に書く
  - `file_path`（相対パスは設定ファイルのフォルダが基準）・`enabled`・
    `max_file_size_mb`（既定100MB、0でローテーションしない）・`max_files`
    （既定5）
  - サイズでのローテーション（`audit.log.1`〜）を実装した。以前は設定項目
    だけがあり、ファイルは増え続けていた
  - 起動時のログに書き込み先を出す。書き込めない場合は警告を出し、メモリに
    だけ残す
  - WordPressとプラグインで、同じ監査ログを使う（以前はハンドラーごとに
    別々の記録器が同じファイルに追記していた）

- プラグインをDockerコンテナの中で起動できるようにした（#411、#392の段階2）。
  `[[plugins]]`に`[plugins.container]`（`image`等）を書くと、`docker run -i`で
  起動し、標準入出力で中継する。既定で、ネットワークなし（`--network=none`）・
  全ケーパビリティ削除・ルートファイルシステム読み取り専用（`/tmp`のみ
  書き込み可）・メモリ512MB・プロセス数256の上限付き。ホストのフォルダは
  `mounts`で指定したものだけを、既定で読み取り専用でマウントする
  - イメージは起動時に取得しない（`--pull=never`）。事前に`docker pull`する
  - `env`の値はコマンドラインに載せず、dockerコマンドの環境変数経由で渡す
  - 停止時・終了時にコンテナを`docker rm -f`で消す
  - コンテナで起動する場合、`command`は省略できる（イメージのCMDを使う）
- プラグインをKubernetesのJobとして起動できるようにした（#413、#392の段階3）。
  `[[plugins]]`に`[plugins.kubernetes]`（`image`等）を書くと、`kubectl`でJobを
  作り、`kubectl attach -i`で中継する。Podは既定で、root以外（UID 65532）・
  全ケーパビリティ削除・seccomp RuntimeDefault・ルートファイルシステム読み取り
  専用（`/tmp`のみ書き込み可）・メモリ512Mi・CPU 500m・ServiceAccountの
  トークンなしで、通信をすべて拒否するNetworkPolicyの対象にする
  - `env`の値は、Jobを所有者とするSecretに入れる（コマンドラインに載せない）
  - Jobは再起動せず（`backoffLimit: 0`）、終了の60秒後に消える。停止時・
    終了時は`kubectl delete`でJobとSecretを消す
  - 必要な権限（Roleの例）は、website/docs/plugins.mdに記載

### Fixed

- 監査ログのファイルへの書き込みを、終わるまで待つようにした（#55で発見）。
  #416で入れた書き込みは、書き込みが終わる前にファイルを閉じることがあり、
  次の書き込みやローテーションと順番が入れ替わるおそれがあった

- 監査ログが、mcp-rsの作業フォルダの`audit.log`（相対パス）に書かれていた
  問題を修正（#416）。Claude Desktop等から起動した場合、作業フォルダは
  クライアントによって決まり、書き込めない場所ではエラーを標準エラー出力に
  出すだけで記録されなかった
  - **ライブラリの変更**: `AuditConfig::default()`（`AuditLogger::with_defaults()`）は
    ファイルに書かなくなった（メモリのみ）。`AuditConfig`に`max_files`を追加。
    `plugin_proxy::start_plugins`は監査ログ（`Arc<AuditLogger>`）を受け取る。
    `PluginHandler::start_with_audit_logger`・
    `WordPressHandler::with_audit_logger`・`McpConfig::audit_config`を追加

- `HttpTransport::transport_info()`が、ロックの競合時に実際に待ち受けた
  アドレスの代わりに設定のアドレス（ポート0のことがある）を返しえた問題を
  修正（#265）。待ち受けたアドレスを`std::sync::RwLock`で持ち、同期の
  メソッドからも待たずに読めるようにした

### Removed

- 本体から使われていなかった認証コードを削除（#421、#419の決定）。
  HTTPモードのAPIキー認証（`security::auth::api_key`、#342）は残す
  - **ライブラリの変更**: 次の公開モジュール・型を削除した
    - `security::auth`の`jwt`・`oauth2`・`session_auth`・`provider`
      （`MultiAuthProvider`）・`middleware`（`AuthMiddleware`）・`api`
      （`create_auth_router`）・`repository`（memory・PostgreSQL・Redis）と
      `AuthConfig`
    - `security::mfa`（TOTP・SMS・バックアップコード・デバイス信頼）と、
      機能（features）の`mfa`・`mfa-full`
    - `compliance`（GDPR/CCPAのエンジン。上記の認証の上に作られていた）
    - `session::jwt_integration`（`SessionJwtManager`等）
  - 依存から`jsonwebtoken`・`totp-rs`・`qrcode`を外した
  - 関連するexample・テストと、READMEのMFA・JWTの記述、MFAの設計書
    （`docs/design/mfa-implementation.md`）も削除した
  - HTTPモードのOAuth 2.1による認可は#420で扱う

- 本体から使われなくなった`core`モジュール（`Runtime`・`HandlerRegistry`・
  `PluginInfo`・`ExecutionContext`）を削除（#409）。#400の修正（#406）以降、
  リクエストの処理は`McpServer`（`RequestDispatcher`）が受け持っている。
  `Runtime`は受信ループが起動されず、`tools/call`の処理も無かった
  - **ライブラリの変更**: 公開モジュール`mcp_rs::core`を削除した
  - `core`だけを試していたテスト（`tests/unit/runtime_tests.rs`・
    `handler_tests.rs`）と、「Core Runtime Module」を完成した機能として
    案内していたドキュメントの記述も削除・修正した

## [0.18.4] - 2026-11-08

### Fixed

- HTTPモードで、MCPのリクエストが処理されず、すべて30秒後に`408`に
  なっていた問題を修正（#400）。受け付けたリクエストを流す先の受信
  ループ（`core::Runtime::run`）が起動しておらず、`tools/call`の処理も
  無かった
  - HTTPモードも、STDIOと同じ処理（`McpServer`）でリクエストに応答する。
    `/mcp`（と`/`）へのPOSTと、`enable_websocket_upgrade`が有効なら`/ws`の
    WebSocketで使える。APIキー認証等の受け付け時のチェックは、これまで
    どおりかかる
  - 同じ`id`を使う複数のクライアント（多くのクライアントは`id`を1から
    振る）の応答が入れ替わらないよう、リクエストごとに直接処理する
  - プラグイン（#394）もHTTPモードで使えるようになった。HTTPの待ち受けに
    届く人は誰でもプラグインのツールを呼べるため、自分のマシン以外から
    届く場合はAPIキー認証を有効にする
  - **ライブラリの変更**: `HttpTransport::with_request_handler`と
    `JsonRpcRequestHandler`トレイト、`McpServer::dispatcher`
    （`RequestDispatcher`）を追加した。`mcp-rs`本体は`core::Runtime`を
    使わなくなった
  - `[transport] type`が`stdio`・`http`以外（未指定を含む）の場合は、
    STDIOモードで動く（以前は処理しないまま待ち続けていた）

## [0.18.3] - 2026-11-08

### Added

- プラグイン（#394、段階1）。設定の`[[plugins]]`で指定したコマンドを、
  別プロセスのMCPサーバーとして起動し、そのツールを
  `<プラグイン名>__<ツール名>`として中継する。STDIOモードのみ
  （HTTPモードは#400の修正後）
  - プラグインには本体の環境変数を渡さず、`env`で指定したものだけを渡す
    （WordPressの認証情報等は渡らない）
  - 本体側で、レート制限・監査ログ（すべての呼び出し）・タイムアウト
    （既定30秒）・応答サイズの上限（既定1MB）をかける
  - プラグインの標準エラー出力は本体のログに記録する。起動できない
    プラグインは警告を出して飛ばし、異常終了・無応答のプラグインの
    ツールはエラーを返す（自動再起動はしない）
  - ファイルシステム・ネットワークは隔離しない。信頼できるプラグインにのみ
    使う（コンテナでの隔離は今後、#392）
  - v0.18.1以前の`[plugins]`（テーブル）は、これまでどおり無視する。
    ただし`[[plugins]]`と同じファイルに書くとTOMLとして読めないため、
    プラグインを追加するときは古い`[plugins]`を削除する。両方あって
    読み込みに失敗した場合は、エラーにその旨を添える
  - ツールの結果は`content`・`isError`・`_meta`だけを返す。新しい仕様の
    `structuredContent`は除く（mcp-rsは`outputSchema`を中継しないため、
    クライアントが`content`の代わりにJSONを表示していた）
  - ドキュメント: `website/docs/plugins.md`

- 投稿・固定ページを作成・更新するツールに`slug`引数を追加（#398）。
  日本語のタイトルの記事に、英語のスラッグを付けられる。これまでは
  WordPressがタイトルから日本語のスラッグを作り、管理画面で1件ずつ
  変更する必要があった
  - 対象: `create_post`・`create_advanced_post`・
    `create_post_with_featured_image`・`create_post_with_categories_tags`・
    `create_post_with_embeds`・`update_post`
  - 省略時はこれまでどおりWordPressがタイトルから作る。空文字列は
    受け付けない
  - 結果には、WordPressが実際に設定したスラッグを表示する（重複時の
    `-2`等）。`get_post`の表示にもスラッグを追加した
  - **ライブラリの変更**: `PostCreateParams`・`PostUpdateParams`に
    `slug`フィールドを追加した。構造体を`..Default::default()`なしで
    組み立てている場合は、`slug: None`の追加が必要

### Changed

- STDIOモードで、登録したすべてのハンドラーのツール・リソースを扱うように
  した（#394）。これまでは最初の1つのハンドラーにしか渡しておらず、
  プラグイン（#394）を追加するとどちらかのツールしか見えなくなるため
  - `tools/list`・`resources/list`は全ハンドラーの分を登録順にまとめる。
    同じ名前のツールは先に登録したものを使う。`tools/call`はツール名、
    `resources/read`はURIで呼び出し先を決める
  - `initialize`の応答の`serverInfo`を、WordPressハンドラーの
    `mcp-rs-wordpress` 0.1.0から、`mcp-rs`と本体のバージョンに変更した。
    ハンドラーが1つもない場合も、エラーにせず応答する
  - 存在しないツール名は、WordPressハンドラーに渡さず`Tool not found`の
    エラー（`isError: true`）を返す

## [0.18.2] - 2026-11-07

### Removed

- 本体に接続されていなかったプラグイン実装をすべて削除（#393）。いずれも
  プラグインのコードを実際には実行していなかった（調査結果は#392）。
  プラグイン機構は、各プラグインを独立したMCPサーバーとして別プロセスで
  動かし、mcp-rsが中継する方式で作り直す（#392・#394）
  - **ライブラリの変更**: 公開モジュール`mcp_rs::plugin`・`mcp_rs::plugins`・
    `mcp_rs::plugin_isolation`・`mcp_rs::docker_runtime`（`docker-runtime`
    機能）と、オペレーターのPlugin（`reconcile_plugin`・`Plugin`等のCRD型）
    を削除した。`docker-runtime`機能と、依存関係の`libloading`・`bollard`も
    削除した
  - **設定の変更**: 設定ファイルの`[plugins]`（`PluginsConfig`）を削除した。
    本体はこれまでもこの設定を読んでいなかった。設定ファイルに残っていても
    無視される
  - Kubernetesの`plugins.mcp.n-takatsu.dev`のCRD（`k8s/crds/plugin-crd.yaml`）、
    RBACのPluginの権限、Pluginのexampleを削除した
  - 関連するexample・テスト・ベンチマーク・ドキュメント（`docs/plugin-*.md`・
    `docs/docker-runtime-guide.md`）を削除し、プラグインの隔離を既存の
    機能として案内していた箇所を「計画中」に直した
- 何にも接続されていなかったKubernetes連携モジュール`src/k8s/`を削除
  （#305）。`PluginDeployment`・`PluginPolicy`のCRDと、2つの独立した
  reconciler（`PluginOperator`・`PluginReconciler`）はどちらもTODOだけの
  スタブで、オペレーター（`mcp-operator`）からも呼ばれていなかった。
  オペレーターが扱うのは`src/operator/`の`MCPServer`等で、変更はない
  - **ライブラリの変更**: `kube`機能有効時の公開モジュール`mcp_rs::k8s`を
    削除した
  - 対応するCRD（`k8s/crds/plugin-deployment.yaml`・
    `plugin-policy.yaml`）とexample、これらを使える機能として説明していた
    `docs/kubernetes-operation-guide.md`も削除した（存在しないHelm
    リポジトリ・ディレクトリを案内しており、どこからもリンクされて
    いなかった。Kubernetesの案内は`docs/KUBERNETES_GUIDE.md`）
- 使われていなかった対話式のセットアップ機能（`src/setup/`）と、実行時の
  対話式コントローラー（`src/runtime_control.rs`、`config::ConfigSwitcher`）
  を削除（#386）。v0.16.0以降、`--setup-config`・`--generate-config`・
  `--demo-setup`・`--switch-config`は処理されておらず、指定しても無視されて
  いた。いずれもApplication Passwordを入力中に表示する作りで、
  `src/setup/`は平文のまま設定ファイルに保存していた
  - 設定ファイルは`configs/production/`のものを使う。設定ファイルに
    パスワードを置く場合は`mcp-rs encrypt-wordpress-credentials`で暗号化
    できる
  - **ライブラリの変更**: 公開モジュール`mcp_rs::runtime_control`と、公開型
    `mcp_rs::config::ConfigSwitcher`を削除した
  - 関連するexample（`interactive_setup_test`・
    `runtime_transport_switching_demo`）と、機能を説明していた
    `docs/RUNTIME_TRANSPORT_SWITCHING.md`も削除した

### Fixed

- example`wordpress_blog_service_demo`が、Application Passwordの先頭4文字を
  画面に表示していた問題（#386）

- WordPressへのリクエストがタイムアウトや5xxで失敗したとき、作成（POST）・
  削除（DELETE）のリクエストを再送しないよう修正（#388）。1回目が
  WordPressで処理済みだった場合に、`upload_media`でメディアが重複したり、
  `delete_*`で削除できているのに404のエラーになったりしていた
  - GET・PUTはこれまでどおり再送する。接続できなかった場合と429は、
    リクエストが処理されていないため、メソッドを問わず再送する
  - 再送しなかった場合は、処理済みの可能性があり、結果を確認してから
    再実行するよう、エラーメッセージで示す

## [0.18.1] - 2026-11-07

### Added

- `get_post`に`format`引数を追加（#356）。`raw`（既定。Gutenbergのブロック
  区切り等を含む編集用の原文で、そのまま`update_post`に渡せる）・
  `rendered`（表示用のHTML）・`text`（タグを除いたテキスト）を選べる。
  `raw`は編集権限が必要で、権限が無い場合は表示用のHTMLを返し、その旨を
  出力に示す
- `mcp-rs encrypt-wordpress-credentials`サブコマンド（#355）。設定ファイルに
  貼り付ける`[handlers.wordpress.encrypted_credentials]`のセクションを生成
  する。これまでは生成にRustのコードを書く必要があった
  - ユーザー名・Application Password・マスターパスワード（確認のため2回、
    12文字以上）を端末から入力する。パスワードは入力中も表示しない
  - 端末でない場合（標準入力のリダイレクト等）は、標準入力の3行から読む
  - パスワードをコマンドライン引数では受け付けない（シェル履歴や
    プロセス一覧に残るため）
  - 出力前に、生成した値を復号して入力と一致することを確認する
- `upload_media`に`file_path`引数を追加（#380）。base64の文字列の代わりに、
  mcp-rsが動いているマシン上のファイルを指定してアップロードできる。AIが
  base64を書き出す必要がなく、トランスポートのメッセージサイズ上限にも
  当たらない
  - 既定では無効。`[handlers.wordpress]`の`upload_allowed_dirs`で許可した
    フォルダの中のファイルだけを読む（存在しないフォルダを指定すると起動
    エラー）
  - パスは絶対パスのみ。`..`やシンボリックリンクを解決した後のパスで判定
    するため、許可フォルダの外には出られない
  - 中身は`file_data`と同じ検証（サイズ・MIMEタイプ・拡張子・マジック
    ナンバー）を通す。`filename`・`mime_type`は省略するとファイル名と
    拡張子から決める
  - HTTP/WebSocketで使う場合は、クライアントではなくサーバー上のファイルを
    読むことになるため、起動時に警告を出す
- `get_post`の出力に、アイキャッチ画像の行（`Featured Media:`）を追加（#378）。
  メディアIDと画像のURLを表示し、未設定なら`none`と表示する。これまでは
  アイキャッチ画像を設定したかをmcp-rsのツールだけでは確認できなかった。
  画像のURLが取得できなくても`get_post`は失敗させず、IDだけを表示する

### Changed

- **挙動の変更**: `create_advanced_post`・`create_post_with_embeds`は、
  `status`を指定しない場合に公開ではなく下書きになるよう変更（#367）。
  意図しない公開を避けるため、v0.17.3で`create_post`系を`draft`既定にした
  方針にそろえた。公開するには`status: "publish"`を指定する
- **挙動の変更（ライブラリ）**: 公開型`PostCreateParams`の`Default`の
  `status`を`publish`から`draft`に変更。`..Default::default()`で組み立てて
  `create_advanced_post`を呼ぶコードは、下書きを作成するようになる
- `create_post_with_featured_image`に`status`引数を追加（既定は`draft`）
- 投稿作成で`status: "future"`（予約投稿）を日付なしで指定すると、
  WordPressが即座に公開してしまうため、全ツールが経由する
  `create_advanced_post`で拒否するよう変更（`date`を指定できるのは
  `create_advanced_post`のみ）。それ以外の値は、プラグインが追加する
  独自のステータスもあるため絞らずWordPressに任せる
- 投稿作成系ツールで、`status`に文字列以外の値を渡した場合に、既定値で
  黙って続けずに入力エラーにするよう変更
- 投稿作成系ツールの成功メッセージに、要求した値ではなくWordPressが実際に
  設定したステータスを表示するよう変更
- `create_advanced_post`の失敗時に、WordPressのエラーコード・メッセージを
  含めるよう変更（以前はHTTPステータスのみ）
- `create_post_with_featured_image`は、5xx・429で再送しなくなった。投稿の
  作成を再送すると、1回目が実は成功していた場合に投稿が重複しうるため、
  ほかの投稿作成系ツールにそろえた
- 開発用ビルド（`dev`プロファイル）のデバッグ情報を行番号のみ
  （`line-tables-only`）に変更（#371）。exampleの`.pdb`が1個あたり
  約400MBになり、`target`が約70GBまで肥大化していたため。デバッガで
  変数を見る場合は`CARGO_PROFILE_DEV_DEBUG=true`を設定する

### Fixed

- `get_post`が本文をプレーンテキスト化したうえで100文字に切り詰めており、
  本文全体を確認できなかった問題（#356）。全文を返すようにした。閲覧
  パスワード付きの投稿で本文が空になる場合は、空として扱わないよう
  注意書きを添える
- WordPressへのリクエストが成功したときに、応答本文の先頭500文字を警告
  ログに出していたのを、バイト数のみのdebugログに変更（#356）。編集用の
  応答には、投稿の閲覧パスワードや未公開の下書きが含まれるため
- `delete_media`を`force: true`で呼ぶと、削除に成功しているのに
  `404 rest_post_invalid_id`のエラーを返していた不具合を修正（#383）。
  完全削除の応答（`{"deleted": true, "previous": {...}}`）を解析できず、
  同じ削除リクエストを再送していたため
- WordPressへのリクエストで、成功（2xx）の応答を解析できなかった場合に、
  同じリクエストを最大3回まで再送していたのを、再送せずエラーを返すよう
  修正（#383）。リクエストは処理済みのため、再送すると作成の重複や
  誤ったエラーにつながる
- `501 Not Implemented`を再送しないよう修正（#383）。再送しても結果が
  変わらないため（例: メディアのゴミ箱移動に未対応のサイトで`force`なしの
  `delete_media`を呼んだ場合）
- `upload_media`で0バイトのファイルを指定したとき、`File is empty`の
  エラーを返すよう修正（#383）。これまでは中身とMIMEタイプの不一致として
  拒否していた
- `create_post_with_featured_image`が常にHTTP 400（`empty_content`）で
  失敗し、投稿を作成できなかった問題（#367）。タイトル・本文を、
  WordPressが書き込み時に読まない`{"rendered": ...}`の形で送っていた
- STDIOサーバーが、JSON-RPCの通知（`id`なし。`notifications/initialized`等）
  に`-32601`のエラー応答を返していた問題（#360）。JSON-RPC 2.0に従い、
  通知には応答しないようにした。`tools/call`等のリクエスト用のmethodが
  `id`なしで届いた場合も、実行せずに警告ログのみ記録する
- HTTP transportが、JSON-RPCの通知（`id`なし）に
  `{"jsonrpc":"2.0","result":{"status":"accepted",...}}`の本文を返していた
  問題（#374）。MCPのHTTP transportに従い、本文なしの`202 Accepted`を
  返すようにした。また、通知の転送に失敗したときなどに、非同期の
  ハンドラー内で`blocking_write()`を呼んでpanicしうる箇所を修正
- `--config`を省略して起動したときの設定読み込み（`McpConfig::load()`）の
  問題を修正（#375）
  - 設定ファイルを読み込んだことを標準出力に書き出しており、STDIOモードの
    通信（JSON-RPC）を壊すおそれがあった。標準エラー出力に変更
  - `~/.config/mcp-rs/config.toml`の`~`がホームフォルダに展開されず、
    この場所の設定ファイルが読み込まれなかった
  - `[transport] type = "stdio"`と書いた設定ファイル（`configs/`の設定例と
    同じ書き方）が、`duplicate field transport_type`で読み込めなかった
- ビルド時の将来非互換警告（`proc-macro-error2`）を解消（#369）。
  使われていなかった`validator`の依存を削除し、`mysql-common-derive`を
  0.32.2に更新した

## [0.18.0] - 2026-11-06

### Added

- HTTP/WebSocket transportのAPIキー認証（`transport.http.auth_enabled`・
  `api_keys`）。`Authorization: Bearer`または`X-Api-Key`で送られたキーを
  検証し、無効・失効・期限切れのキーは401で拒否する。既定は無効（#342）
- WordPressツール呼び出しへのレート制限（サイト接続単位、既定10件/秒・
  バースト20件）（#343）
- WordPressの変更系ツール呼び出しの監査ログ記録。入力検証やレート制限で
  拒否された呼び出しも記録する（#344）
- WordPress認証情報の暗号化保存（`encrypted_credentials`、復号鍵は
  `WORDPRESS_MASTER_PASSWORD`環境変数）（#338）
- `cargo audit`の日次定期実行。develop/mainを監査し、検知時はIssueを
  自動作成する（#340）

### Changed

- `upload_media`の入力検証を追加: MIME許可リスト（画像・PDF・動画・
  音声）、拡張子とMIMEの一致、マジックナンバーによる内容検証、
  ファイル名の検証、サイズ上限（10 MiB）。SVG・テキスト・Office文書・
  movは受け付けなくなった（#339）
- WordPressを有効にしているのに認証情報（`username`/`password`または
  `encrypted_credentials`）が無い設定は起動時にエラーにするよう変更
- `WordPressConfig`/`WordPressHandler`のDebug出力でパスワードを伏せる
  よう変更

### Fixed

- 監査ログファイルが追記ではなく毎回上書きされ、最新の1件しか残って
  いなかった問題

### Security

- v0.17.3の依存関係に含まれていた既知の脆弱性10件のうち9件を更新で
  解消（ammonia、rustls-webpki、webpki-roots、zstd系ほか）。修正版の
  ないRUSTSEC-2026-0387（crypto-bigint、jsonwebtoken 11.xへの移行が
  必要）は`.cargo/audit.toml`で理由を付けて無視

## [0.17.3] - 2026-10-13

### Fixed

- WordPressハンドラーの実機バグ修正（redring.jpでの継続テストで発見）:
  一覧系APIのページネーション頭打ち、固定ページの読み書き不可、
  日本語ファイル名アップロードの失敗、エラー詳細の欠落、意図しない
  公開（status既定値）、カテゴリ/タグのslug・parent未対応など。
- ログ出力の堅牢性向上: 書き込み不可なログディレクトリでの起動時
  パニック、`non_blocking()`ガードの即時dropによるログ消失、
  ファイルへのANSIエスケープ混入を修正。実際の書き込み確認込みの
  フォールバックチェーン、プロセスID付きログ行、ローカル時刻表記を
  追加。

## [0.17.2] - 2026-10-09

### Fixed

- Claude Desktop（STDIO transport）から起動してもMCPサーバーがリクエストを
  処理しないバグを修正。原因は2つ:
  1. `Runtime::initialize()`がtransportを起動するだけで、実際にそれを
     読み続ける`Runtime::run()`を一度も呼んでいなかった。STDIO設定時は
     独立して動作する`McpServer::run_stdio()`（改行区切りJSON-RPCの
     読み取り・ディスパッチループ）へルーティングするよう変更。
  2. `configs/production/claude-desktop.toml`等が`type = "stdio"`と
     書いているが、`TransportConfig.transport_type`フィールドと一致せず
     黙って`None`にデシリアライズされ、上記の新しい分岐が発火しなかった。
     `#[serde(alias = "type")]`を追加し両方の表記を受け付けるよう修正。
- 回帰テスト`test_transport_type_accepts_type_alias`を追加。

## [0.17.1] - 2026-10-03

### Fixed

- `deploy.yml`の`docker/metadata-action`が、タグpush（`v0.17.0`）時に
  `{{branch}}`が空文字列になることで不正なDockerタグ
  （`ghcr.io/n-takatsu/mcp-rs:-<sha>`）を生成し、Dockerイメージの
  ビルド・公開が失敗していた問題を修正。該当するタグパターンを
  ブランチrefの場合のみ有効化するよう変更。

## [0.17.0] - 2026-10-03

### 🔐 Security Enhancement Phase 3 (Epic #219) — 完了

mTLS・ゼロトラスト・HSM・IDS/IPS・量子耐性暗号・GDPR/CCPA対応までを含む、
多層防御のセキュリティ強化計画が全フェーズ完了しました。

#### Phase 3.1: Critical Security

- **mTLS（相互TLS認証）** (#221, #254): HTTP transportへの完全実装
  - `mtls_enabled`・`mtls_ca_cert_path`の設定マッピング追加
  - クライアント証明書なしのリクエストを拒否、有効なCA署名証明書を受理
- **ネットワークポリシーとローカル実行制限** (#220, #256): bind時点でのローカル限定ポリシー適用
- **なりすまし防止・リプレイ攻撃対策** (#222, #259): HTTP/WebSocket/プラグインブローカーへの統合
- **TLS/SSL強制とHSTS** (#224, #261): HTTPトランスポートおよびWebSocket(WSS)の両方に強制適用

#### Phase 3.2: High Priority Security

- **カラムレベル暗号化 + RBAC統合** (#223, #260, #308): `DatabaseHandler`のクエリパスへの実配線、関連バグ修正
- **プラグイン間E2E暗号化の鍵交換フロー** (#225): X25519 ECDH + HKDF-SHA256、PFS・自動ローテーション・グレースピリオド対応
- その他の堅牢化修正: `CommunicationBroker`のメッセージ配送不具合、Redis `DEL`コマンドの既定ブロック、SQLiteトランザクション分離の修正、KubernetesのNetworkPolicy実CNI適用確認・調整、プラグインネットワーク隔離の修正

#### Phase 3.3: Advanced Security

- **ゼロトラスト・マイクロセグメンテーション** (#226): DBクエリ実行パスへの実配線
- **HSM統合**（PKCS#11） (#227): カラム暗号化への追加
- **IDS/IPS** (#228): 既存IDSをHTTPトランスポート層へ実配線し、IPSブロックを実装

#### Phase 3.4: Future Security（スコープ縮小の上で実装）

- **量子耐性暗号（ML-KEM-768ハイブリッド鍵交換）** (#230): プラグイン間E2E暗号化にX25519との
  ハイブリッドKEMコンバイナーとして追加
- **GDPR/CCPAコンプライアンス自動化** (#229): mcp-rs自身が保持するアカウントデータ
  （APIキー・セッション・監査ログ）に対する削除権・アクセス権として実装
  （外部バックエンドDBのデータは対象外）

### Added — その他

- 脅威インテリジェンス統合 (#211)
- Dockerランタイム統合の完成 (#212)
- WebSocket Transport基盤の実装 (#213)
- PostgreSQL最適化 Phase 2 (#214)
- パフォーマンス最適化提案エンジン (#177)
- Analytics モジュールのリファクタリング（anomaly/prediction分割）

### Fixed

- `cargo clippy --all-targets`の警告解消 (#264)
- 新しいstable clippyが`#[async_trait]`生成コードを誤検知する
  `clippy::double_must_use`への対応
- WordPress関連: 設定読み込み不具合、一時的な404アセットの削除、メニュー/README修正、
  モバイルホームページのスタイル調整
- テストコメント中の古いIssue #115参照の整理

## [0.16.0] - 2025-12-22

### 🚀 WebSocket通信機能の強化とプラグイン分離システムの完成

#### Added - WebSocket Transport Enhancements

- **WebSocket Server Mode** (#197)
  - 双方向WebSocket通信のサーバーモード実装
  - 接続管理とクライアントトラッキング
  - Ping/Pongによるヘルスチェック機能
  - 柔軟な接続設定とタイムアウト管理

- **LLM Streaming Integration** (#196)
  - OpenAI GPT-4とClaude 3.5 Sonnetのストリーミング対応
  - リアルタイム応答生成とチャンク送信
  - トークン使用量の追跡と統計
  - エラーハンドリングと再試行ロジック
  - 3つの実装例：
    - `websocket_echo_server.rs`: 基本的なエコーサーバー
    - `websocket_llm_chat.rs`: LLMチャットインターフェース
    - `websocket_load_balanced.rs`: 負荷分散サーバー

- **Connection Pool and Load Balancing** (#195)
  - ラウンドロビン、最小接続数、ランダムの3つのアルゴリズム
  - ヘルスチェックと自動フェイルオーバー
  - 接続プール管理と統計情報
  - 接続再利用によるパフォーマンス向上

- **Metrics, Rate Limiting, and Compression** (#194)
  - リアルタイムメトリクス収集（メッセージ数、バイト数、レイテンシ）
  - 3種類のレート制限アルゴリズム：
    - TokenBucket: バースト対応
    - LeakyBucket: 均一な処理速度
    - SlidingWindow: 時間窓ベース制限
  - メッセージ圧縮（gzip、deflate）で帯域幅削減

- **WebSocket Tests, Benchmarks, Documentation** (#193)
  - 統合テスト: 224テストケース
  - ベンチマーク: 294行の性能測定
  - 包括的ドキュメント:
    - `websocket-guide.md`: 508行の詳細ガイド
    - `websocket-performance.md`: 614行の性能分析
    - `llm-integration-guide.md`: 631行のLLM統合ガイド

#### Added - Plugin Isolation System Completion (#190)

- **Inter-Plugin Communication**
  - メッセージベースのプラグイン間通信
  - Pub/Subパターンによるイベント配信
  - 型安全なメッセージングインターフェース

- **Advanced Error Handling**
  - 包括的エラー分類と復旧戦略
  - エラー伝播とコンテキスト情報
  - 自動リトライとサーキットブレーカー

- **Enhanced Monitoring**
  - プラグイン稼働状態の詳細監視
  - リソース使用量トラッキング
  - パフォーマンスメトリクスとアラート

- **Docker Runtime Support** (#185)
  - Dockerコンテナでのプラグイン実行
  - コンテナライフサイクル管理
  - セキュリティスキャンとコンプライアンス
  - 7つのドキュメント:
    - `docker-runtime-guide.md`: 454行の実装ガイド
    - `plugin-developer-guide.md`: 352行の開発者ガイド
    - `plugin-security-guide.md`: 403行のセキュリティガイド
    - `plugin-troubleshooting-guide.md`: 504行のトラブルシューティング

### Enhanced

- **Performance Optimization** (#177)
  - クエリキャッシング機能の強化
  - 接続プール最適化
  - メモリ使用量の削減

- **Code Architecture** (#162-166)
  - Analyticsモジュールのリファクタリング
  - Operatorモジュールの責務分離
  - Plugin Isolationモジュールの構造改善
  - Security IDSモジュールの再構築
  - Transportモジュールの整理

### Quality Assurance

- **567テスト全て合格** (100% pass rate)
- **Clippy警告ゼロ**: 全モジュールでクリーン
- **フォーマットチェック通過**: cargo fmt準拠
- **ベンチマーク**: WebSocket、プラグイン分離の性能測定完備

### Documentation

- **10個以上の新規ドキュメント追加**
- **実装例3つ追加**: WebSocket通信の実践的サンプル
- **日本語ドキュメント完備**: 初心者から上級者まで対応

### Dependencies

- 既存の依存関係を維持（互換性重視）
- Axum 0.8への対応完了

### Breaking Changes

なし - 後方互換性を完全に維持

---

## [0.15.1] - Previous Release

### Added - RBAC Implementation (Issue #74)

- **Role-Based Access Control (RBAC)** (`src/handlers/database/advanced_security.rs`)
  - `RoleBasedAccessControl` with role hierarchy and inheritance
  - User role management (assign, revoke, get roles)
  - Permission caching for optimized access checks
  - Comprehensive access decision engine

- **Advanced Access Controls**
  - Condition evaluation engine with 6 condition types:
    - TimeOfDay, DayOfWeek, IpAddress
    - UserAttribute, DataSensitivity, QueryComplexity
  - 9 comparison operators (Equals, NotEquals, Contains, GreaterThan, LessThan, Between, In, NotIn, Regex)
  - Time-based access control:
    - Business hours per weekday (Monday-Sunday)
    - Emergency access configuration
    - Break period restrictions
    - Timezone support (defaults to "UTC")
  - IP restrictions:
    - CIDR notation support via `ipnet` crate
    - Role-based IP ranges
    - VPN requirement enforcement
    - Geo-blocking configuration

- **Resource-Level Security**
  - Column-level permissions:
    - Read/Write role assignments
    - Data masking rules integration
    - Encryption requirements
  - Data masking (4 types):
    - **Full**: Complete redaction to "***"
    - **Partial**: Configurable reveal (first/last N characters)
    - **Hash**: SHA-256 hashing via `sha2` crate
    - **Tokenize**: Random token generation via `rand` crate
  - Row-level security:
    - Policy column enforcement (e.g., owner_id)
    - User attribute matching
    - Admin bypass capability

- **IntegratedSecurityManager Integration** (`src/handlers/database/integrated_security.rs`)
  - Enhanced `check_authentication_and_authorization` with RBAC
  - Query type to action mapping (Select→Read, Insert/Update→Write, Delete→Delete, DDL→Admin)
  - Public RBAC APIs:
    - `assign_user_role`, `revoke_user_role`, `update_rbac_config`
    - `check_column_access`, `check_row_level_security`
    - `apply_data_masking`, `get_user_roles`

- **Comprehensive Test Suite** (15 RBAC tests)
  - Basic RBAC operations (role assignment, hierarchy)
  - Condition evaluation (all 6 types + 9 operators)
  - Time-based access control scenarios
  - IP restriction validation with CIDR
  - Column-level permission enforcement
  - Data masking (all 4 types)
  - Row-level security policies

### Security

- **7-Layer Security Architecture**: Now includes RBAC as the primary access control layer
- **Data Masking**: 4 masking strategies for PII/PHI protection
- **Fine-Grained Access Control**: Column and row-level security policies
- **Time-Based Security**: Business hours and emergency access support
- **Network Security**: IP-based access restrictions with CIDR

### Dependencies

- Added `ipnet = "2.10"` for CIDR notation IP range validation
- Added `sha2 = "0.10"` for SHA-256 hashing in data masking
- Added `rand = "0.8"` for cryptographically secure token generation

### Quality Assurance

- 15/15 RBAC tests passing (100%)
- 133 library tests total (100% passing)
- 44 compatibility tests (100% passing)
- Zero Clippy warnings
- Zero compiler errors

### Documentation

- Updated README.md: 6-Layer → 7-Layer Security Architecture
- Enhanced module documentation with comprehensive feature lists
- Added detailed API documentation for RBAC methods

### Added - MySQL Phase 1 Security Enhancement

- **Parameterized Query Support** (`src/handlers/database/engines/mysql/prepared.rs`)
  - `MySqlPreparedStatement` struct for type-safe parameter binding
  - SQL injection prevention through parameter separation
  - Support for all MySQL data types (NULL, Bool, Int, Float, String, DateTime, Binary)
  - Automatic row conversion to internal QueryResult format

- **Transaction Management** (`src/handlers/database/engines/mysql/transaction.rs`)
  - `MySqlTransactionManager` for transaction lifecycle management
  - `MySqlTransaction` context for ACID-compliant operations
  - Full support for transaction isolation levels:
    - READ UNCOMMITTED
    - READ COMMITTED
    - REPEATABLE READ
    - SERIALIZABLE
  - Savepoint functionality:
    - Named savepoint creation
    - Partial rollback to savepoint
    - Savepoint release and cleanup
  - Automatic rollback on transaction drop with warning

- **Trait Extensions** (`src/handlers/database/engine.rs`)
  - `parameter_count()` method for PreparedStatement trait
  - `get_sql()` method for PreparedStatement trait
  - Default implementations for backward compatibility

- **Comprehensive Test Suite** (45 tests - 2,140 lines)
  - Basic functionality tests (21 tests):
    - Parameter counting and validation
    - SQL injection prevention scenarios
    - Complex query handling
    - Isolation level support
    - Savepoint operations
    - Data type handling
    - Unicode and special character support
    - Performance metrics
  - Integration tests (24 tests):
    - Prepared statement lifecycle
    - Transaction workflows
    - Savepoint scenarios
    - 4 SQL injection attack vectors
    - Data integrity validation
    - Failure recovery
    - Concurrent access patterns

### MySQL Security

- **SQL Injection Prevention**: All 4 major attack vectors tested and blocked
- **Transaction Isolation**: 4-level isolation support validated
- **Data Type Safety**: Type-safe conversion between Rust and MySQL
- **Error Handling**: Comprehensive error propagation and recovery

### MySQL Performance

- Parameter conversion: ~164µs for 1000 SQL statements
- Batch operation handling: Successfully tested with 10,000 operations
- Savepoint management: Successfully tested with 100+ savepoints

### MySQL Quality Assurance

- 45/45 tests passing (100%)
- Zero Clippy warnings
- Zero compiler errors
- Full backward compatibility
- No breaking changes

## [0.15.0] - 2025-11-08

## 🚀 Major Release: ユーザーフレンドリーな設定管理システム

### Added

- **対話的設定セットアップシステム** (`--setup-config`)
  - WordPress接続テスト付きの設定ウィザード
  - リアルタイム接続検証とエラー診断
  - crossterm使用のクロスプラットフォーム対応UI
  - スピナーアニメーションとカラー出力
- **動的設定管理システム**
  - `--switch-config`: 実行時設定ファイル切り替え
  - `--config <file>`: カスタム設定ファイル指定
  - `--reload-config`: 動的設定再読み込み
  - 設定ファイル自動検出機能
- **デモンストレーションモード** (`--demo-setup`)
  - 安全なテスト環境での機能体験
  - デモ設定ファイル自動生成
- **包括的ヘルプシステム** (`--help`)
  - 全オプションの詳細説明と使用例
  - 設定ファイル検索順序の明示
- **GitHub Pages統合**
  - 美しいランディングページ (index.html)
  - Jekyll設定の最適化とpermalink構造
  - 404エラーの完全修正

### Enhanced

- **ユーザビリティの大幅向上**
  - 設定ファイル不存在時の自動セットアップ起動
  - 分かりやすい日本語エラーメッセージ
  - 初心者から上級者まで対応の段階的ガイダンス
- **ロバストな入力処理**
  - EOF検出と再試行制限によるパイプ入力対応
  - 非対話環境での自動フォールバック
  - 入力ストリーム終了の適切な処理

### Changed

- **Breaking Changes Resolved**: Complete migration to latest API versions
  - axum 0.8 WebSocket API with `.into()` conversions
  - secrecy 0.10 Secret types (Secret to SecretString)
  - jsonwebtoken 10.0 new crypto backend support
  - criterion 0.7 black_box API migration
  - serde_yaml → serde_yaml_ng transition
- **Performance Improvements**: Significant performance gains across all subsystems
  - 15-20% faster async runtime (tokio 1.48)
  - 20-30% improved HTTP throughput (axum 0.8 + hyper 1.6)
  - 10-20% faster cryptographic operations
  - Enhanced DNS resolution and network performance

### Security Enhancements

- **Enhanced Cryptography**: Latest security algorithms and implementations
  - jsonwebtoken 10.x with improved crypto backends
  - secrecy 0.10 with strengthened Secret management
  - ring 0.17.8 cryptographic optimizations
- **Vulnerability Management**: RUSTSEC-2023-0071 properly managed
  - No actual security impact (unused dependency chain)
  - Alternative secure MySQL implementation via mysql_async
  - Comprehensive audit trail documentation

### Technical Updates

- **Quality Assurance**: 356+ tests passing with zero warnings
  - Complete test suite modernization
  - Strict clippy compliance (-D warnings)
  - Cargo fmt standardization
  - Release build optimization
- **Documentation**: Comprehensive guides and references
  - Updated API documentation
  - Migration guides for breaking changes
  - Production deployment guides
  - Security configuration examples

## [0.1.0-alpha] - 2025-11-04

### WordPress Features

- **WordPress Integration**: Complete WordPress REST API integration with 27 tools
  - Advanced post/page management with SEO integration
  - Complete media management with base64 upload support
  - Category and tag management with hierarchical support
  - YouTube and social media embed support
  - Comment management and retrieval
- **Enterprise Security**: 6-layer security architecture (100% implemented)
  - AES-GCM-256 encryption with PBKDF2 key derivation
  - SQL injection protection (11 attack patterns)
  - XSS attack prevention (14 attack patterns)
  - Token bucket rate limiting with DDoS protection
  - TLS 1.2+ enforcement
  - Comprehensive audit logging
- **Core Infrastructure**:
  - JSON-RPC 2.0 server implementation using axum
  - Type-safe TOML configuration with environment variable override
  - Comprehensive error handling with thiserror
  - Async/await support with tokio runtime
  - Production-ready logging with tracing
- **Documentation**:
  - Complete README with usage examples
  - Technical documentation in project-docs/
  - GitHub Pages website preparation
  - Contributing guidelines and code of conduct
- **Testing**: 205+ comprehensive tests with 100% pass rate
- **Security Features**:
  - Zero-panic operations with Result-based error handling
  - Safe environment variable expansion with infinite loop prevention
  - Application password lifecycle management
  - Production monitoring and health checks

### Security Implementation

- Implemented military-grade AES-GCM-256 encryption
- Added comprehensive input validation and sanitization
- Enabled zero-trust data validation architecture
- Implemented real-time security monitoring

### Technical

- Built with Rust 2021 edition for memory safety
- Async-first architecture using tokio
- Clean layered architecture with separation of concerns
- Production-optimized build profiles

### API Documentation

- Comprehensive API documentation for all 27 WordPress tools
- Security implementation guide with examples
- Architecture documentation with design decisions
- Complete setup and deployment guides

## [0.0.0] - 2025-10-01

### Initial Setup

- Initial project setup
- Basic project structure
- License files (MIT/Apache-2.0)

[Unreleased]: https://github.com/n-takatsu/mcp-rs/compare/v0.1.0-alpha...HEAD
[0.1.0-alpha]: https://github.com/n-takatsu/mcp-rs/releases/tag/v0.1.0-alpha
[0.0.0]: https://github.com/n-takatsu/mcp-rs/releases/tag/v0.0.0
