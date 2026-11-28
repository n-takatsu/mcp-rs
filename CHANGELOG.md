# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed

- プラグインの起動の途中で終了の合図（SIGTERM・Ctrl+C）を受けたら、起動を
  待たずにやめ、後片付け（子プロセス・コンテナ・KubernetesのJob・Secret）を
  して終わる（#775）。残りのプラグインは起動しない。今までは、起動し終えて
  から止めたので、`start_timeout`（既定は30秒）の間に`docker stop`の猶予
  （既定は10秒）が過ぎ、後片付けの前にSIGKILLで終わることがあった
  - ライブラリとして使う場合: `plugin_proxy::start_plugins_until`を足した
    （`start_plugins_reporting_failures`に、やめる合図のfutureを渡す版）
- STDIOで取り消した（`notifications/cancelled`）プラグインのツールの呼び出しを、
  プラグインにも取り消しとして伝える（#766）。今までは、プラグインは取り消された
  ことを知らずに処理を続け、STDIOの次の要求も、その呼び出しが終わるか
  `call_timeout_seconds`が過ぎるまで待たされた
  - プラグインに送り終えた要求は、mcp-rsが割り当てた`id`で、プラグインに
    `notifications/cancelled`を送り、応答を待たずに終える。後から届いた応答は、
    今までどおり`id`で読み飛ばす
  - まだ送っていない要求（同じプラグインの前の呼び出しを待っている）は、送らない
  - 送る途中では止めない（書き終えてから取り消す）。書きかけのフレームを残さない
    今までの守り（#750）は、そのまま
  - 監査ログには、結果を`CANCELLED`として残す（呼び出し元も今までどおり）
  - プラグインのツール以外の、読み取りだけでないツール（WordPressの変更等）は、
    今までどおり最後まで行い、応答だけを返さない。HTTPモードは変わらない

## [0.31.0] - 2026-11-28

### Added

- 公式のMCP Registry（`registry.modelcontextprotocol.io`）に、リリースの
  `.mcpb`を`io.github.n-takatsu/mcp-rs`の名前で登録する（#668）。リポジトリの
  ルートに`server.json`を置き、新しいワークフロー`mcp-registry.yml`が、
  リリースのタグで`.mcpb`をリリースに付けた後に、その版・URL・SHA256を
  書き込んで登録する。認証はGitHub ActionsのOIDCで行い、鍵やトークンは
  持たない。`server.json`を変えたPRでは、登録せずに中身を確かめるだけ
  v0.30.0は、マージの後に手で実行して登録した。README・導入ガイドに、
  公式のMCP Registryに載っていることを書いた
- `[database]`で、MongoDBを使えるようにした（`engine = "mongodb"`、#530）。MongoDBは
  SQLではないので、操作ごとのツールを出す: `find`・`aggregate`・`count_documents`・
  `get_schema`（コレクションの一覧・おおよその数・数件から読んだフィールドの
  名前と型）。`allow_write = true`のときだけ`insert_documents`・`update_documents`・
  `delete_documents`
  - 書き込みの段（`$out`・`$merge`）と、JavaScriptを動かす演算子（`$where`・
    `$function`・`$accumulator`）は、どのツールでも断る（フィルター等の中も見る）。
    `system.`のコレクションは使えない。コレクション・インデックスを作る・消す
    ツールは無い
  - `update_documents`・`delete_documents`は、空のフィルターを断る。置き換えの更新
    （演算子の無いドキュメント）も断る。`insert_documents`は`max_rows`件まで
  - `insert_documents`は、あるコレクションにだけ挿入する。MongoDBは、無い
    コレクションへの挿入でコレクションを作るので、先に確かめて、無い名前
    （打ち間違い等）は断る。確かめたあとに他の誰かが消した場合は、ユーザーに
    権限があれば作られる。順に挿入し、途中で失敗したら、何件が入ったかを
    エラーに書く（送り直すと二重に入るため）
  - 返す量の上限: `find`は`max_rows`件まで、`aggregate`には`$limit`を足す。
    少しずつ受け取り、`max_result_bytes`に達したら残りを読まずにカーソルを閉じる。
    `get_schema`も、結果が`max_result_bytes`に収まるように、コレクションと
    フィールドを省く。ドキュメントを読み切れなかったコレクションには
    `fields_unread`を付ける
  - 読み取りの時間の上限は`maxTimeMS`で送り、こちらも2秒後に待つのをやめる。
    初めの応答から読み終えるまでの全体で、1つの上限。`get_schema`は全部の
    コレクションで1つの上限で、時間が尽きたらそこまでを返す（名前の一覧も
    読めていなければ、エラー）
  - 書き込みには`maxTimeMS`を送れない（ドライバーに無い）。時間が過ぎたら
    待つのをやめるだけで、サーバーは続けることがある。結果は分からないと
    エラーに書く。あとから当たることもあるので、読んで確かめるだけでは
    足りない。二重に当たっても結果が同じだと言い切れるときだけ、送り直せる
    （削除・`$set`・`_id`のある挿入でも、言い切れるとは限らない）。通信の
    エラー（切断等）で終わったときも、同じく結果は分からないと書く。サーバーが
    断ったエラーは、1つのドキュメントの書き込みなら、そのまま返す（#808）。
    `update_documents`・`delete_documents`の`many`では、サーバーが途中で
    止めても、それまでのドキュメントは変わっているので、確かめてから送り直す
    よう書く。ドライバーは書き込みを送り直さない（`retryWrites`を切った。
    送り直すと、返るエラーから、1回目が当たったかが分からなくなるため。
    1つのホストにだけつなぐので、送り直しても同じホストに行く）。mongosが、
    シャードから結果を受け取れなかったことを書き込みのエラーで返したもの
    （`Write results unavailable`）も、成否が分からない側にする
  - 待つのをやめても、ドライバーの操作は別のタスクで最後まで動かす（途中で
    落とすと、ドライバーの内部の状態が壊れることがある）。そのタスクは接続を
    持ち続けるので、動いているタスクが接続のプールの大きさ（`max_connections`）
    に達したら、新しい操作を始めずに断る（#808）。枠が空くのは、サーバーが
    答えるか接続が切れたとき。TCPのkeepaliveが切るのは相手が消えた接続だけで、
    サーバーが生きていて答えない操作は切らない。枠が埋まったままなら、
    サーバーで操作を止める（`killOp`）か、mcp-rsを起動し直す
  - コレクションの名前の前後の空白は削らない（#808。`users `を指定すると、
    `users`を読み書きしていた）
  - 書いたホストにだけつなぐ（`directConnection`）。レプリカセットの他の
    メンバーは探さない（`ssl_mode`の確認を通っていないホストに、認証の情報を
    送らないため）。書き込むなら、`host`はプライマリー
  - 値はExtended JSON（relaxed）で返し、フィルターでもExtended JSONを受け付ける
  - `database`はデータベースの名前、`username`は省略できる（認証しない。書いたら
    パスワードが要る）、`auth_source`（新しい項目。既定は`admin`）。
    `ssl_mode`は`disable`・`require`・`verify-full`（ループバック以外では必須。
    省略したら、TLSを使わない）。`auth_source`は、MongoDB以外では断る。
    データベースの名前は、どのOSのサーバーにも共通する制限だけを確かめる
    （#808。Windowsのサーバーでだけ使えない`*<>:|?`は、サーバーに任せる）
  - 監査ログには、ツールとコレクションの名前だけを書く
  - 既定のビルドに含めた。`mongodb`クレートは、`mongodb+srv://`のためのDNSを
    読む機能を外した（1つのホストにつなぐ）
  - 設定の例: `configs/database/mongodb.toml`。CIに、MongoDBのサービスと、実際の
    MongoDBでツールを確かめるテスト（`tests/database_mongodb_tools_test.rs`）を足した
- `[database]`で、Redisを使えるようにした（`engine = "redis"`、#529）。RedisはSQLでは
  ないので、Redis専用のツールを出す: `redis_command`（読み取りのコマンドを1つ）・
  `scan_keys`（`SCAN`で鍵を一覧）・`get_schema`（版・鍵の数・`INFO keyspace`）。
  `allow_write = true`のときだけ`redis_write_command`（`SET`・`DEL`・`HSET`等）
  - 使えるのは、mcp-rsが決めた一覧のコマンドだけ。`KEYS`・`FLUSHALL`・`FLUSHDB`・
    `CONFIG`・`EVAL`・`SCRIPT`・`SELECT`・`CLIENT`・`ACL`・`MULTI`・`RENAME`等は、
    `allow_write`でも使えない。一覧は設定で変えられない
  - 返す量の上限: まるごと返すコマンド（`HGETALL`・`SMEMBERS`等）と範囲
    （`LRANGE`・`ZRANGE`）は、送る前に数を確かめ、`max_rows`を超えるなら断る。
    `XRANGE`には`COUNT`を足す。大きな値（`GET`等）は、長さを確かめ、
    `max_result_bytes`を超えるなら断る
  - `database`はデータベースの番号（省略時は0）、`username`はACLのユーザー
    （省略時は既定のユーザー）。`ssl_mode`は`disable`・`require`・`verify-full`
    （ループバック以外では必須）。`ssl_root_cert`は`verify-full`のときだけ
  - `redis_write_command`で、Redisが断ったのではない失敗（応答の時間切れ・
    切断等）では、コマンドが実行されたか分からないと書く（#808。送ったコマンドは
    取り消されないので、送り直すと`INCR`等が二重に当たる）
  - オブジェクトにできない`HGETALL`の結果（フィールドの名前がUTF-8でない）は、
    フィールドと値の組で数えて切る（#808。1つずつ数えると、値が落ちた）
  - 監査ログには、一覧にあるコマンドの名前と引数の数、失敗の種類だけを書く
    （鍵・値・Redisのエラーの文は書かない）
  - `username`を書いたら、パスワード（`password`か`keyring`）が要る（無いと、
    既定のユーザーとしてつながるため）
  - 既定のビルド（リリースの実行ファイル・Dockerイメージ）に含めた。`redis`
    クレートを0.26から1.8に上げた（TLSはrustls）
  - 設定の例: `configs/database/redis.toml`。CIで、実際のRedisでツールを確かめる
    （`tests/database_redis_tools_test.rs`）
- `[database]`で、SQLiteを使えるようにした（`engine = "sqlite"`、#528）。
  `database`にデータベースのファイルのパスを書く（相対パスは設定ファイルの
  フォルダが基準）。`host`・`port`・`username`・`password`・`keyring`・
  `ssl_mode`・`ssl_root_cert`は使わないので、書いてあれば、データベースの
  ツールを出さずに起動し、`mcp_rs_status`で理由を知らせる（資格情報ストアは
  読まない）。
  PostgreSQL・MySQLと同じ安全性にそろえた
  - ファイルは作らない。無ければ、データベースのツールを出さずに起動し、
    `mcp_rs_status`・`mcp-rs check`で知らせる
  - ファイルは読み取り専用で開き、`execute_query`・`get_schema`は、
    トランザクションの中で読んでロールバックする。`allow_write = true`の
    ときだけ、書き込みの接続も開き、`execute_command`を出す
  - 1つのSQLの時間の上限（`query_timeout_seconds`）。SQLiteの進み具合の
    コールバックで止める。ほかのプログラムのロックも、同じ時間まで待つ
  - 行数・結果のバイト数の上限。値は、列で宣言した型ではなく、値の型で返す
  - `get_schema`は、表・ビューの列（生成列も）・主キー・外部キー・インデックスを
    返す（表の名前はSQLに埋め込まない）。参照する列を書かない外部キーは、
    参照する表の主キーの列にする
  - 起動のときに、ファイルの中身を読み、SQLiteのファイルかを確かめる
  - `execute_command`の`last_insert_id`は、INSERTで行を足したときだけ返す
  - SQLの関数`load_extension()`・`fts3_tokenizer()`を断る
  - 同梱のSQLiteの脆弱性（RUSTSEC-2026-0459、#449）は、細工したデータベースの
    ファイルを開くと当たる。信頼できるファイルだけを開くよう、文書と設定の
    例（`configs/database/sqlite.toml`）に書き、`.cargo/audit.toml`の無視の
    理由を書き直した
  - ライブラリとして使う場合: `SqliteEngine`は、プール（読み取りと書き込み）を
    1つ持ち、接続の間で共有する（以前は接続ごとにプールを作っていた）。
    `SqliteConnection::query`は、読み取り専用の接続で読む。
    `SqliteConnection::pool()`は外した（PR #822のレビューで、書いていない
    ことを指摘された）。読み書きは`DatabaseConnection`のメソッド（`query`・
    `execute`・`prepare`・`begin_transaction`等）で行う。sqlxのプールが要る
    なら、呼ぶ側で作って持つ。`:memory:`は、
    1つの接続を共有し、使わない時間・接続の寿命（sqlxの既定は10分・30分）で
    閉じない（#808。閉じると、データベースが消えていた）
- データベースのツールに、SELECTの実行計画を返す`explain_query`を足した
  （#646。PostgreSQL・MySQL・MariaDB・SQLite）。遅いSQLの原因や、インデックスが
  使われるかを調べられる。PostgreSQLは`EXPLAIN (FORMAT JSON)`、MySQL・MariaDBは
  `EXPLAIN FORMAT=JSON`、SQLiteは`EXPLAIN QUERY PLAN`の結果を返す
  - SQLは実行しない（`EXPLAIN ANALYZE`は送らない。データベースが、計画を
    作るために行を少し読むことはある）。SQLは`EXPLAIN`を付けずに
    渡す。`execute_query`と同じ検査（1つのSELECTだけ等）を通し、読み取り専用の
    トランザクションの中で聞いて、ロールバックする
  - 読み取りだけのツールの印（`readOnlyHint`）を付け、`allow_write`に関わらず
    出す。呼び出しは、`execute_query`と同じく監査ログに記録する
  - 計画は切らない（`max_rows`は当てはめない）。`max_result_bytes`を超える
    なら、エラーにする

### Removed

- ライブラリとして使う場合: 列の暗号化を外した（#768）。mcp-rsの実行ファイル
  （`[database]`の設定）は使っていなかった機能で、実行ファイルの動きは変わら
  ない。ライブラリとして使う場合には互換性の無い変更。レビューで、鍵の管理に
  13件の指摘があった（`Local`の鍵をメモリにだけ持ち、再起動すると保存した
  値を復号できない。ローテーション・キャッシュ・排他の不具合。`AwsKms`・
  `Vault`を選んでもメモリの鍵で動く等）。直すには鍵の管理の作り直しが要る
  ので、不具合のあるAPIを公開したままにせず、外した。指摘は、外したので直さない。
  列を伏せる・復号する処理に頼っていた場合は、外したAPIの呼び出しを消すだけ
  では足りない（PR #822のレビュー）。`execute_query`は、列をデータベースの
  値のまま返す。見せてよい列だけを読めるロール・ビュー等の守りを、更新の
  前に用意する。保存済みの暗号文は、復号されないまま返る
  - `handlers::database::column_encryption`（`ColumnEncryptionManager`・
    `ColumnEncryptionConfig`・`KeyManager`・`KeyManagerConfig`・`KeyProvider`・
    `KeyMetadata`・`EncryptionAlgorithm`・`CacheStats`）
  - `handlers::database::column_encryption_rbac`（`ColumnEncryptionRbac`・
    `ColumnEncryptionPermission`・`EncryptionAuditLog`・`EncryptionOperation`）と、
    その表を作るマイグレーション（`migrations/20260201_column_encryption_rbac.sql`）
  - `DatabaseHandler::with_column_encryption`と、`execute_query`の結果の列を
    復号する・伏せる処理。`DatabaseHandler::execute_query_as`は残る
    （マイクロセグメンテーションが使う）
  - `handlers::database::column_provenance`の`resolve_column_provenance`・
    `ColumnProvenance`・`ProjectionResolution`（列の暗号化だけが使った）
  - HSM（PKCS#11）との連携: `security::hsm`、Cargoの機能`hsm`、依存クレートの
    `cryptoki`（列の暗号化の鍵だけが使った）
  - 例の`column_encryption_demo`と、結合テスト
    （`database_handler_column_encryption_test`・`column_encryption_rbac_test`・
    `hsm_pkcs11_integration_test`）

### Fixed

- v0.31.0のリリースのPR（#822）のレビュー（CodeRabbit）の指摘を直した
  - Dockerイメージに、Redis・MongoDBのエンジンが入っていなかった
    （`Dockerfile`が`database,mysql-backend`だけでビルドしていた）。既定の
    ビルドと同じエンジンを入れる
  - `[database]`の`ssl_root_cert`の`${VAR}`を展開していなかった（どの
    エンジンでも、`${VAR}`の文字のままファイルを開いて失敗した）。`${VAR}`で
    始まるパスは、今までの説明のとおり、展開した値をそのまま使う（相対
    パスでも、設定ファイルのフォルダを基準にしない）
  - Redisの`LPOP`・`RPOP`で、`max_result_bytes`を超える値を、取り出して
    から結果を切っていた（切った値は、もうリストに無い）。取り出す前に
    `LRANGE`で読み、返す形のJSONの長さを確かめ、超えるなら取り出さずに
    断る。結果は切らない。確かめた後に、ほかのクライアントがリストを
    変えたら、確かめた値と違う値を取り出し、`max_result_bytes`を超えて
    返すことがある
  - MongoDBの`insert_documents`で、mongosが結果を受け取れなかったエラー
    （`Write results unavailable`）が最初のドキュメントのとき、「入って
    いない」と言っていた。入ったか分からないと書く
  - MongoDBで、接続のプールが埋まって操作を始めなかったときのエラーが、
    いつも「待つのをやめた操作が埋めている」と言っていた。今の操作が
    使っているだけのこともあるので、送っていないので送り直してよいこと
    を先に書く
  - ライブラリとして使う場合: `MySqlTransaction`の`query`・`execute`が、
    パラメーターのJSON・日時を`NULL`にしていた。結果の符号なしの大きな
    整数を負の数に、`TIME`を`12:34:56:000001`の形にしていた。接続
    （`MySqlConnection`）と同じ変換を使う
  - `mcp-registry.yml`: PRで`server.json`を確かめるジョブにも、OIDCの
    トークンを取る権限（`id-token: write`）を付けていた。確かめるジョブと
    登録するジョブを分け、確かめるジョブには付けない
  - 結合テストの文書: SQLiteのエンジンはいつもビルドに入る（SQLiteの結合
    テストだけが`sqlite`のfeatureを見る）。`mod`の`--ignored`の実行の前に、
    MySQLのコンテナを起動して`TEST_MYSQL_URL`を設定する。Redis・MongoDBの
    結合テストは、ループバックでない接続先では、データを消す前に止める
- `upload_media`の`file_path`が許可フォルダ（`upload_allowed_dirs`）の外を
  指したときのエラーに、使える許可フォルダの一覧と、ファイルをそこへ写して
  やり直すよう書く（#819）。今までは、どのフォルダなら使えるかが書かれて
  いないので、AIは`file_data`（base64）に切り替えるしかなく、画像1枚に数分
  かかり、大きなファイルは送れなかった。一覧を出すのはstdioのときだけで、
  HTTP・WebSocketでは出さない（サーバーのフォルダを、つないできた相手に
  見せない）。外のファイルと無いファイルは、今までどおり同じエラー。
  `file_data`の説明に、小さなファイル向けで、使えるなら`file_path`を使うことを
  書いた
- データベースの設定例・ツールの説明・結合テストの文書を、実際の動きに
  合わせた（#810。v0.30.0からの変更のレビュー、#806）
  - `configs/database/sqlite.toml`: `max_connections`はプールごとの上限で、
    `allow_write = true`では読むプールと書くプールがあるので、合わせて2倍まで
    開く。`max_result_bytes`は目安の上限
  - `configs/database/redis.toml`: `SCAN`・`HSCAN`・`SSCAN`・`ZSCAN`の結果は
    切らないので、`max_rows`・`max_result_bytes`を超えることがある。
    `scan_keys`も、最後のまとまりは切らない
  - `configs/database/mongodb.toml`: `max_result_bytes`が数えるのは、
    ドキュメント（コンパクトなJSON）だけ。`get_schema`のコレクションの一覧には
    `maxTimeMS`が付かない（ドライバーに無い）。`insert_documents`の
    「既存のコレクションだけ」は、呼んだときに確かめるもの
  - `username`の説明に、MongoDBでも省けること（認証せずにつなぐ）を書いた
  - `execute_command`の説明（SQLite）: `last_insert_id`が`null`でも、書き込みは
    済んでいるので、送り直さない
  - `docs/development/integration-tests.md`: データベースのツールのテスト
    （MySQL・MariaDB・Redis・MongoDB）の変数と実行のしかたを書いた。Redis・
    MongoDBのテストは、決まった番号・名前のデータベースを消すので、使い捨ての
    サーバーを指すこと（テストのコメントにも書いた）
  - CI（`container-integration-tests.yml`）: Redis・MongoDBのツールのテストの
    ファイルだけを変えたときも動くようにした
- リリースのワークフロー（`release-binaries.yml`）: タグの実行をやり直すと、
  作り直したファイルで、リリースのファイルを`--clobber`で置き換えていた。
  作り直した`.mcpb`・`.zip`は、中身が同じでも時刻が変わるのでハッシュが変わり、
  MCP Registryに登録した`.mcpb`の`fileSha256`と合わなくなる（同じ版は
  登録し直せない）。今は、リリースにすでにあるファイルは置き換えず、無い
  ファイルだけを足す（#809）。`SHA256SUMS`は、リリースのファイルの
  `.sha256`から作り直す。出所の証明は、足すファイルだけに作る。リリースの
  `.sha256`がリリースのファイルと合わなければ、止める
- `mcp-rs check`が、`[database]`につなぐのを10秒で打ち切っていた。起動では、
  つないだ後の最初のコマンド（Redisの`PING`等）を`query_timeout_seconds`まで
  待つので、起動はできるのに`check`は失敗と言うことがあった。今は、起動と
  同じだけ（12秒と`query_timeout_seconds`）待つ（#808）
- ライブラリとして使う場合: ゼロトラスト（`zero_trust`。
  `DatabaseHandler::with_micro_segmentation`が使う）の、レビューの指摘を直した
  （#768）。mcp-rsの実行ファイルは、この機能を使わない
  - `MicroSegmentation::default()`・`setup_default_policies()`の既定のポリシーで、
    `AccessPolicy::new`の引数の順（名前、リソース、スコア）を取り違えていた。
    `user`のポリシーのリソースが`*`になり、信頼スコア30以上の`user`が、
    すべてのリソースを読めた。今は`/api/public/*`だけ。**既定のポリシーに
    頼って`user`にほかのリソースを読ませていた場合は、ポリシーを足すこと**
  - 既定のデータベースのセグメント（`db_admin`）が、どのリソースにも当たら
    なかった（セグメントの所属が`/api/database/*`との完全一致、ポリシーの
    リソースが`database`との完全一致）。今は、データベースのハンドラーが
    確かめるリソース（`database:<エンジンのID>:<表>`）と`/api/database/*`に当たる
  - `ResourceSegment`のリソースを、ポリシーと同じパターン（`…*`は前方一致）で
    比べる。以前は完全一致だった
  - `ZeroTrustContext::is_valid`が、`verified_at`が未来のとき、引き算があふれた
    （デバッグビルドではpanic）。今は無効とする
  - 信頼スコアの範囲（0〜100）を確かめる。`ZeroTrustContext`は、100を超える
    スコアを100に丸めてから平均する。`MicroSegmentation::evaluate_access`は、
    100を超えるスコアを断る
- ライブラリとして使う場合: マイクロセグメンテーション（`with_micro_segmentation`、
  表ごとの許可）が、`execute_command`にかかっていなかった。許可を確かめるのは
  `execute_query`だけだったので、許していない表にも書け、`INSERT … SELECT`や
  副問い合わせで読めた。今は、実行の前に、書き込む表には`write`（`DELETE`は
  `delete`）、文の中で読む表には`read`の許可を確かめる（#609）。`[database]`の
  設定からは使えない機能なので、mcp-rsの実行ファイルには影響しない
  - ぶつかった今の行を消してから入れる書き方（`REPLACE INTO`、SQLiteの
    `INSERT OR REPLACE`・`UPDATE OR REPLACE`）には、`write`に加えて`delete`の
    許可も要る。SQLに書いた分だけを見る: 表の定義で`UNIQUE … ON CONFLICT
    REPLACE`と決めたSQLiteの表では、普通の`INSERT`・`UPDATE`でも、ぶつかった
    同じ表の行が消えるが、これは`write`の許可だけで通る
  - MySQLの複数の表の`UPDATE`・`DELETE`は、`SET`の代入先（`u.note`の`u`）・
    `DELETE`の直後に書いた表（`DELETE o FROM …`の`o`）だけを書き込む表とし、
    ほかの`JOIN`した表には`read`を求める（#807。`JOIN`しただけの表の値を、
    `read`の許可なしに書き込みに使えた）。`SET note = …`のように代入先の表を
    見分けられないときは、`JOIN`した表のすべてに、書き込みと`read`の両方を
    求める。消す表を見分けられない`DELETE`と、表を分けられない形（`WITH`等）は
    断る
  - 検証済みの呼び出し元で書き込む`DatabaseHandler::execute_command_as`を足した
    （`execute_query_as`の書き込み版）。MCPのツールとして呼ぶと、呼び出し元が
    分からないので、ロールや信頼度の要る許可は通らない（`execute_query`と同じ）
  - 断るときのエラーに、足りない許可を書く（`… for table 'notes' (read)`）
  - 表の名前を確かに拾えない書き方は、`execute_query`でも断る: `TABLE name`
    （`SELECT * FROM name`と同じ意味。`UNION`の中でも）と、引用符の中に
    `.`のある名前（`"public.users"`は、`public.users`とは別の表）、文字列の
    SQL・表の名前で表を読むPostgreSQLの関数（`query_to_xml`・`table_to_xml`・
    `ts_stat`・`crosstab`・`connectby`等）。ユーザーが作った関数までは見分けられないので、
    本来の守りは、データベースのロールの権限
  - PostgreSQLでは、表の名前を許可と比べる前に、引用符で囲まない名前を
    小文字にそろえる（PostgreSQLと同じ。`Users`は`users`、`"Users"`は別の表）。
    SQLiteでは、引用符で囲んだ名前も小文字にそろえる（SQLiteは、表の名前の
    大文字・小文字を区別しない）。
    以前は書いたとおりに比べたので、`"Users"`への許可で、`Users`と書いて
    `users`に触れられた。**許可のリソースを大文字まじりで書いていた場合**
    （`database:pg:Users`・`database:sqlite:Notes`）は、SQLに合わなくなる
    （断る側）。許可のリソースの側は、そろえずに、書いたとおりに比べるので、
    次のように書き直すこと: PostgreSQLは、引用符なしで使う表を小文字で
    （`"Users"`と引用符で囲んで使う表は、そのまま`Users`）、SQLiteは、
    すべて小文字で。小文字にするのはASCIIの文字だけで、ほかの文字は
    そのまま（`ÄNotes`は`Änotes`。#825）
  - 確かめるのは、SQLに書いた表の名前だけ。その先でデータベースがすること
    （PostgreSQLの継承・パーティションの子の表、ビューの元の表、トリガー、
    `ON DELETE CASCADE`）は見ない。親の表・ビューへの許可は、それを通して
    届く行への許可になる
- ライブラリとして使う場合: MySQL・MariaDBのエンジンの、レビューの指摘を直した
  （#768）。どれも、mcp-rsの実行ファイル（`[database]`のツール）が通らない経路
  - `MySqlEngine::connect`が、最初の設定で作ったプールを、別の接続先の設定を
    渡されても使っていた。今は断る（接続先ごとにエンジンを作ること）
  - `MySqlEngine::connect`は、ループバック以外のホストで`ssl_mode`を省略したら
    断る（本体はハンドラーで断っていた。#722）。最初の接続で`SELECT VERSION()`を
    待つ時間も、接続の時間の上限に入れた
  - `MySqlEngine::health_check`が、確かめずに、いつも`Healthy`・
    `connection_count = 1`を返していた。今は、接続を受け取って`PING`を送る
    （まだつないでいなければ、エンジンの設定でつなぐ。つながらない・失敗・
    時間切れは`Critical`。`connection_count`は分からないので0）
  - `begin_transaction`の`START TRANSACTION`に、時間の上限を付けた
  - `MySqlParamConverter`: 日時をマイクロ秒まで渡す（以前はミリ秒まで）。
    文字列・コメントの中の`?`を、パラメーターとして数えない（実行される
    コメント`/*! … */`の中は数える）。i64に入らない符号なしの整数を、-1等に
    せず、文字列で返す
  - `MySqlPreparedStatement`は、パラメーターの数をサーバーから受け取る。
    `MySqlPreparedStatement`・`MySqlTransaction`の`last_insert_id`は、i64に
    入らないとき、負の数にせず、文字列で返す
  - ベンチマークとテストの`ssl_mode = "disabled"`（正しくは`"disable"`）を直した
- カスタム投稿タイプの`categories`・`tags`・`terms`が、`/wp/v2`の外の
  タクソノミー（`shop/v1`等）のとき、項目のIDを確かめずに送っていた。無い
  IDがあると、WordPressはそのIDを飛ばし、今の項目を外して成功と返しうる。
  今は、その名前空間の一覧（`/<名前空間>/<rest_base>?include=`）で確かめ、
  無いIDや、確かめられないとき（一覧が読めない、同じ`rest_base`の
  タクソノミーが2つ以上ある等）は送らずに断る。空の
  配列での全部外しは、今までどおり確かめずに送る（#785）
- `mcp-rs init`で、既にある設定ファイルを新しいものに置き換えると答えた
  後、質問の間に別のところで設定ファイルが編集されても、確かめずに置き
  換えていた（編集は控えには残る）。ファイルが無かったときに、質問の間に
  作られたファイルも同じ。今は、質問の前の中身（か、無かったこと）と書く
  直前の中身を比べ、変わっていれば書かずに止める（#785）

## [0.30.0] - 2026-11-26

### Added

- ライブラリとして使う場合: `HttpTransport::receive_request_with_context`と
  `HttpTransport::stop_server`を足した（#760）。`receive_request_with_context`は、
  要求・呼び出し元と一緒に、`MCP-Protocol-Version`ヘッダーで選んだ版
  （`ReceivedRequest::protocol_version`）を返す。
  `RequestDispatcher::handle_message_with_protocol_version`に渡すと、版が
  `Some`なら、`with_request_handler`と同じくその版で答える。`None`
  （ヘッダーの無い要求）なら、ディスパッチャーの共有の版（それまでの
  `initialize`で決まった版）で答える。`with_request_handler`の`/mcp`は、要求
  ごとに新しい接続の状態で、対応する一番古い版で答えるので、ツールの`title`の
  有無等が違うことがある（同じにするなら、`handle_message_in_session`に要求
  ごとの新しい`Session`を渡す）。クライアントの付けた`id`
  （`ReceivedRequest::original_id`、通知は`None`）も返す。`stop_server`は、
  待ち受けをやめ、接続を閉じる（`Transport::stop`も同じ）。
  `receive_request_with_caller`は今までどおり

- ライブラリとして使う場合: `HttpTransport::receive_request_with_caller`を
  足した（#749）。`with_request_handler`を使わないときに、HTTP・WebSocketで
  受けた要求を、呼び出し元（APIキー・トークンの名前と使えるツール、接続元の
  IPアドレス）と一緒に返す。`RequestDispatcher::handle_message_as`に渡せば、
  認証していない遠隔の呼び出し元には`mcp_rs_status`がバージョンだけを返し、
  監査ログにも呼び出し元が残る。`Transport::receive_message`は今までどおり
  （呼び出し元を返さない）。mcp-rs本体は`with_request_handler`を使うので、
  動きは変わらない

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
  公開日時・スラッグ・カテゴリ・タグは変えない）。リビジョンが有効なサイト
  では（`WP_POST_REVISIONS`）、WordPressが書き戻す前の内容をリビジョンとして
  残すので、たいていは書き戻しも元に戻せる。必ず残したいときは、先に
  `get_post`で読む。別の投稿のリビジョンは断る。原文が読めない（編集の
  権限が無い）ときは、表示用のHTMLで上書きしない
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

- `update_post`で抜粋（`excerpt`）を変えられるようにし、`get_post`は抜粋
  （`Excerpt:`）も出す（#774）。`review_post_seo`のプロンプトは、抜粋を
  確かめて、承認の後に`update_post`で直すよう頼むが、どちらのツールも抜粋を
  扱えなかった。抜粋に対応しない投稿タイプでは、WordPressは抜粋を捨てるので、
  結果にそう書く

- `get_terms`で、タクソノミーの名前と`rest_base`が別々のタクソノミーに
  合うときは、`rest_base`の合う方を読む（#774）。以前は、WordPressの応答で
  先に出た方を黙って読んでいた

- 全件を返す一覧（`get_posts`・`get_pages`・`get_all_content`）の説明に、
  同じ時点の一覧ではないことを書いた（#774）。読んでいる間に、読んだページで
  1件消え、読んでいないページで1件増えると、件数も重なりも変わらず、1件
  欠けうる。`create_comment`の説明にも、WordPressは新しいコメントを一度
  承認済みで保存してから、同じ要求の中で頼んだ状態にすることを書いた
  （#774。保存のときに動くプラグインには、承認待ちのコメントも承認済みに
  見えうる）

- `mcp-rs check --help`に、環境変数`MCP_RS_CONFIG`があれば`--config`より
  優先することを書いた（#774）。プラグインの後片付けに失敗したときの案内
  （`check`と終了時）に、Kubernetesではプラグインの`--context`・
  `--namespace`を付けて確かめるよう書いた。開発用の
  `scripts/clean-build.ps1`は、`cargo metadata`の`target_directory`
  （`.cargo/config.toml`の`build.target-dir`も含む）で、ビルド先を決める。
  `-CargoArgs`の`--config`・`--manifest-path`も`cargo metadata`に渡す

- ライブラリとして使う場合: `mcp_rs::types::JsonRpcRequest`の`id`を
  `RequestId`から`Option<RequestId>`にした（#760）。通知（`id`なし）は
  `None`で、書くときも`id`を付けない。以前は`id`が必須で、`HttpTransport`の
  チャンネル（`receive_message`・`receive_request_with_caller`・
  `receive_request_with_context`）に通知が届くと、"missing field `id`"の
  エラーになり、受信側が読めなかった。通知には応答しない（`id`が`null`の
  応答を`send_message`に渡しても、捨てる）。
  **互換性のない変更**: `request.id`を`RequestId`として使うコードは、
  `Option`を扱うよう直す必要がある。要求の`id`を応答に写すコード
  （`JsonRpcResponse { id: request.id, .. }`）は、`None`（通知）なら応答せず、
  `Some(id)`なら取り出した`id`（`HttpTransport`では内部の相関ID）を、その
  まま応答の`id`に入れる（例: `let Some(id) = request.id else { continue };`）。mcp-rs本体は
  `with_request_handler`を使うので、動きは変わらない

- ライブラリとして使う場合: `HttpTransport`のチャンネル
  （`receive_message`・`receive_request_with_caller`）で受け取る要求の`id`を、
  クライアントの付けた`id`から、要求ごとの内部の相関ID（`"mcp-rs-1"`等の
  文字列）に変えた（#760）。応答の`id`にそのまま入れて`send_message`で返せば、
  クライアントには元の`id`で届く（今までどおり、要求の`id`を応答に写す
  作りなら、変更は要らない）。取り消し（`notifications/cancelled`）の
  `params.requestId`は、クライアントの`id`のまま届くので、同じ呼び出し元
  （`caller`）の要求のうち、`ReceivedRequest::original_id`が同じものを
  取り消す（ほかの呼び出し元の要求は、同じ`id`でも取り消さない）。
  mcp-rs本体は`with_request_handler`を使うので、動きは変わらない

- `[database]`の`host`がループバック（`localhost`・`127.0.0.1`・`::1`。
  PostgreSQLはUnixソケットも）以外なら、`ssl_mode`を省略できないようにした（#722）。
  省略時の`prefer`は、サーバーがTLSを使えなければ暗号化せずにつなぎ、
  証明書も確かめないので、警告だけでは気付かれずに使われていた。省略すると、
  データベースのツールを起動せず、理由と書き方（`ssl_mode = "verify-full"`、
  確かめずにつなぐなら`"require"`等を書く）を`mcp_rs_status`に出す。
  PostgreSQL・MySQL・MariaDBで同じ。PostgreSQLは、`PGSSLMODE`に書いて
  あれば省略できる（読めない値は、sqlxが黙って`prefer`にするので、起動しない）。`mcp-rs check`も同じように止める。
  **互換性のない変更**: リモートのデータベースで`ssl_mode`を書いていない
  設定ファイルは、1行（`ssl_mode = "verify-full"`か、使うモード）を足す
  必要がある。`mcp-rs init`が書く設定ファイルは、既に`verify-full`を書いている

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


### Removed

- ライブラリとして使う場合: mcp-rs本体が使わない次のものを外した（#760）。
  ライブラリとして使う場合には互換性の無い変更。これらへのレビューの指摘は、
  外したので直さない
  - 動的な設定（`config::dynamic`・`DynamicConfigManager`）
  - WebSocketのクライアント（`transport::websocket`・`WebSocketTransport`・
    `WebSocketConfig`等）と、そのテスト・例
    （`websocket_transport_foundation`）。mcp-rs本体のWebSocket（HTTPの
    トランスポートの`/ws`）は別の実装で、変わらない。設定ファイルの
    `transport_type`は、今までどおり`stdio`・`http`だけを受け付ける
  - `types`のMCPの型（`Tool`・`Resource`・`Prompt`・`PromptArgument`・
    `ServerInfo`・`ServerCapabilities`・`ToolsCapability`・
    `ResourcesCapability`・`PromptsCapability`）。mcp-rs本体は
    `mcp::types`を使う。`Transport`が使うJSON-RPCの型（`JsonRpcRequest`等）は
    残す
  - 監査ログの`AuditLogger::redact_user`
  - `RateLimiter`の認証の失敗の記録とブロック（`record_auth_failure_ip`・
    `record_auth_failure_user`・`check_ip_blocked`・`check_user_blocked`・
    `reset_auth_failures_*`・`get_auth_failure_count_*`・
    `cleanup_auth_failures`）。HTTPのトランスポートの認証の失敗の制限は
    別の実装で、変わらない
  - IDSのアラートの通知のチャネル（`NotificationChannel`・
    `AlertManager::add_notification_channel`・`NotificationResult`）。
    メール・Slack・Webhookへは送らない。アラートは今までどおりログに書く
  - APIキーの生成（`ApiKeyManager::generate_key`）と、`ApiKeyConfig`
    （`key_length`等。生成のほかに使うところが無かった）。
    `ApiKeyManager::new()`は引数を取らない。mcp-rs本体は、設定ファイルの
    キーを`import_key`で取り込む

- テスト・開発用のcompose（`configs/docker/docker-compose.postgres.yml`）から、
  PostgreSQLのセカンダリ（`postgres-secondary`、ポート5433）と、その起動の
  スクリプト（`scripts/postgres/standby.sh`）を外した（#732）。
  レプリケーションの設定がPostgreSQL 15に合っておらず（`pg_basebackup -W`で
  止まる・`recovery.conf`・rootでの起動等）、起動しなかった。使うテストも
  無かった。pgAdminのサーバー一覧・`.env.example`（`POSTGRES_SECONDARY_PORT`・
  `TEST_POSTGRESQL_SECONDARY_URL`）・ヘルパー（`scripts/postgres/docker-compose.sh`）・
  ガイドからも消した。ヘルパーと`docs/DOCKER_POSTGRES_GUIDE.md`は、
  サービスを指定せずに起動する手順に戻した。ヘルパーの`up`・`down`は、
  残ったセカンダリのコンテナを`--remove-orphans`で消す。ボリュームの消し方は、
  ガイドに書いた

### Fixed

- `mcp-rs init`でWordPressとデータベースの両方を選び、既にあるデータベースの
  資格情報を入れ直したとき、データベースにつながらなくてもWordPressが使え
  れば、入れ直した資格情報を確定していた（#779の後退。PR #773の3回目の
  レビュー）。前の中身に戻せず、同じ項目を使うほかの設定もつながらなく
  なった。データベースにつながらなければ、データベースの資格情報だけを前の
  中身に戻し、WordPressの登録は続ける

- `update_post`で、本文に対応しない投稿タイプに本文と抜粋を渡すと、どちらも
  保存されないのに、ほかの変更は保存されたと返していた（PR #773の3回目の
  レビュー）。本文は、応答に本文があるときだけ、ほかの変更に数える

- `mcp-rs database-keyring set`で、`[database]`が既にこの項目を使い、
  パスワードだけを入れ直すとき、書いた後の読み戻しに失敗すると、前の中身に
  戻していなかった（PR #773の3回目のレビュー）。ほかの経路と同じく戻す

- HTTPで、認証の失敗が続くアドレスを止める仕組みの、覚えきれないアドレスを
  まとめて数える変更（#705、未リリース）で、認証が通って数が0になった
  アドレスも、1分の間、表に残していた（#779）。認証の通るアドレスで表
  （1万件）が埋まると、新しいアドレスは、まとめた数で、認証を試す前に
  429になりえた。数が0になったアドレスは、表から消す

- HTTPの`timeout_ms`（設定ファイルの`request_timeout_seconds`）が、
  リクエストを受け取る時間だけでなく、処理中のリクエストの無い接続を閉じる
  までの時間と、本文のある応答の送信の期限（#759）にも使われることを、説明に
  書いた（#779）。応答の送信が、この時間と、大きさに合わせた時間（64 KB/s）の
  間に進まなければ、接続を閉じる。大きな応答の送信中を何もしていない接続と
  しないのは、送信が進む間だけ。ツールの実行時間は、今までどおり制限しない

- ライブラリとして使う場合: `/ws`は1つの接続のメッセージを1つずつ処理する
  ので、`/ws`で処理中の要求への取り消し（`notifications/cancelled`）を同じ
  接続で送っても、その要求が終わるまで届かないことを、
  `ReceivedRequest::original_id`の説明に書いた（#779）。取り消すには、HTTPで
  送る

- `mcp-rs init`のデータベースで、設定ファイルを書けなかったとき・Ctrl+Cで
  止めたときに、入れたパスワードを戻さなかった（#779）。新しい項目は消し、
  置き換えた項目は前の中身に戻す（WordPressの資格情報と同じ）。戻せない・
  消せないときは、`mcp-rs database-keyring`で直すように知らせる

- `mcp-rs init`のデータベースで、新しい名前として選んだ項目が、質問の間に
  別のところで作られていると、上書きしていた（#779）。上書きせずに止め、
  やり直して別の名前を選ぶように知らせる

- `mcp-rs init`のデータベースで、質問の間に設定ファイルに`[database]`が
  足されると、重複した`[database]`を書き足し、設定ファイルが壊れた（#779）。
  質問の間に設定ファイルが変わっていれば、書かずに止める。
  `mcp-rs database-keyring set`で`[database]`を書き足すときも同じ

- `mcp-rs database-keyring set`で、質問の間に設定ファイルが変わっても、
  質問の前に読んだ中身から作った設定で上書きしていた（#779。変わった分が
  失われた）。書かずに止め、入れたパスワードを前の中身に戻す（新しく作った
  項目は消す）。`[database]`を書き足す・別の項目に変えるときも同じ。設定を
  変えないと答えたときは、入れたパスワードを残す

- `mcp-rs init`で、足せなかったサイトの資格情報を消せなかったとき、消すものの
  一覧から外していた（#779。終わるときにも消さなかった）。消せてから外す

- `mcp-rs init`で、重複する登録を消すかを、最初に1回だけ読んだ設定ファイルで
  決めていた（#779）。質問ごとに読み直す（質問の間に手で消したサイトを、
  足したサイトとみなさない）

- `MCP_RS_CONFIG`の`${a}/b-${c}`のように、値全体が1つの`${...}`でないパスを、
  拡張機能が埋めなかった値とみなして使わなかった（#779）。値全体が1つの
  `${...}`（中に`{`・`}`が無い）のときだけ、無いものとする

- `mcp-rs check --help`に、`MCP_RS_CONFIG`を設定していれば`--config`を
  渡しても`MCP_RS_CONFIG`を確かめることを、はっきり書いた（#779。順は
  `MCP_RS_CONFIG`・`--config`・既定のパス）

- プラグインの後片付けの案内（`check`・終了時）に、`docker_command`・
  `kubectl_command`を設定していれば（`podman`等）、そのコマンドに読み替える
  ことを書いた（#779）

- HTTPモードで終わるとき、HTTPの待ち受けを止める前にプラグインを止めていた
  （#774）。止めたプラグインに、要求が届きえた。待ち受けを止めてから、
  プラグインを止める

- ログの切り替えで、前のファイルの先頭の行（`continued`）が書けないままだと、
  いつまでも新しいファイルに切り替わらなかった（#774）。書けなければ、その行を
  あきらめて標準エラー出力に知らせ、切り替える。切り替えの判定と切り替えで
  時刻を別々に読んでいたので、その間に境目を越えると、書けていない先頭の行を
  捨てていた。時刻は1回だけ読む。起動の行（`started`）の`log:`も、行を作った
  後に切り替わると、実際のファイルと違いえた。決めたファイルに直接書く

- コンソールとファイルの両方にログを出すとき、spanのフィールドの整形の結果を
  共有し、ファイルにコンソールのANSIのエスケープが混ざりえた（#774。
  依存クレートのspanを`RUST_LOG=debug`等で出したとき）。ファイルは別に整形する

- ログの保持（古いログの削除）の既知の制限を、コードの説明に書いた（#774）。
  別のプロセスが1期間より長く書かず、その後に新しいファイルを開けないと、
  そのプロセスのファイルを消しうる（Unix系では、その後の記録が残らない）

- STDIOで、入力の終わり（EOF）の後に要求の処理がパニックすると、成功
  （終了コード0）として終わっていた（#774）。エラーで終わる。キーが重複した
  JSONの要求（`"method"`が2つ等）を、v0.30.0の変更で断らなくなっていた
  （後の値を使った）。以前と同じく`-32600`で断る

- `/ws`で、要求が期限（120秒）までに終わらないとき、何も返さず、クライアントは
  応答を待ち続けた（#774。`enable_websocket_upgrade`を有効にしたとき）。その
  要求の`id`を付けたエラー（`-32001`）を返す。ライブラリとして使う場合:
  チャンネル経由（`with_request_handler`を使わない）の`/ws`も同じ。キューの
  枠が空かないときは`-32000`を返す。チャンネル経由の通知は、元の文字列でなく
  確かめたJSONを渡す（HTTP・`/ws`）。キーが重複した通知で、
  `receive_request_with_context`がエラーを返していた

- 監査ログのファイルのロック（`audit.log.lock`）を、期限なしに待っていた
  （#774）。ほかのプロセスが止められたまま持つと、書き込みが止まった。5秒まで
  待ち、取れなければ警告する（メモリへの記録は続ける）

- IDS（既定は無効）のCMD-005が、`x;rm victim`のように、`rm`に拡張子の無い
  普通の語が続くものを見落としていた（#774）。小文字の`rm`だけ、普通の語の
  引数も検知する（`; del Toro`・`; RM Williams`等の文には当てない）

- ライブラリとして使う場合: PostgreSQLのエンジンが、接続先だけを比べ、
  `ssl_mode`・`ssl_root_cert`が違う`connect`にも、エンジンの設定のTLSで
  作ったプールを返していた（#774）。`ConfigurationError`で断る。mcp-rs本体は
  エンジンと接続に同じ設定を使うので、起きない

- ゼロトラストのアクセスポリシー（ライブラリ）の時間の範囲が、終わりの時刻の
  分の0秒で終わっていた（#774）。`time_end = "23:59"`のポリシーは、23:59:01から
  23:59:59まで拒んでいた。終わりの分の終わりまでを含む

- タイトル・本文に対応しない投稿タイプ（`editor`の無いカスタム投稿タイプ
  等）のリビジョンを、`get_revisions`・`get_revision`・`restore_revision`が
  読めなかった（#774）。WordPressは、親の投稿タイプが対応する項目だけを
  返す。無い項目は「無い」と出し、書き戻しでは送らない。`restore_revision`の
  結果は、実際に書き戻した項目だけを挙げる（以前は、送っていない抜粋も
  「書き戻した」と書いた）。抜粋も、タイトル・本文と同じく、原文が読めない
  ときは表示用のHTMLで上書きせずに断る

- `update_post`・`create_advanced_post`の`terms`で、タクソノミーに無い項目の
  IDを渡すと、WordPressはそのIDを黙って飛ばし、ほかの項目だけを外して成功と
  返していた（#774）。送る前に、項目がタクソノミーにあるかを
  （`/wp/v2/<rest_base>?include=`で）確かめ、無いIDは断る。空の配列
  （全部外す）は調べない

- `update_post`・`create_advanced_post`の`categories`・`tags`で、無い項目の
  IDを渡すと、`terms`と同じく、WordPressはそのIDを黙って飛ばし、今のタグ等
  だけを外して成功と返していた（#779）。投稿（`post`）でも、送る前に確かめ、
  無いIDは断る。エラーは`categories`・`tags`の名前で示す。空の配列
  （全部外す）と、`/wp/v2`の外のタクソノミーは調べない

- `create_comment`で、WordPressが頼んだ状態と違う状態（`approve`を頼んで
  承認待ち等）でコメントを作っても、成功と返していた（#779）。エラーにし、
  作成済みであること、コメントのID、実際の状態、`update_comment`での直し方を
  返す。作り直すと重複するので、作り直しは勧めない

- `update_post`で、抜粋に対応しない投稿タイプに抜粋だけを渡すと、抜粋を
  保存しないのに成功と返していた（#779）。エラーにする。ほかの項目も渡した
  ときは、今までどおり、ほかの変更は保存されたと伝える。空の`terms`（`{}`）・
  空の`date`等、WordPressに送らない項目と、投稿タイプに無い`categories`・
  `tags`の空の配列、反映されなかったアイキャッチ画像は、ほかの項目に数えない

- `get_revision`の`Format`で、本文もタイトルも無く、抜粋だけを持つ
  リビジョンを、いつも原文が読めないと出していた（#779）。抜粋の原文で決める

- `get_comments`の`post_id`が、`7.0`・`"7"`・`-1`等のとき、黙って無視され、
  全投稿のコメントを返していた（#774）。`7.0`は7とし、正の整数でなければ
  断る

- スラッグの検査で、パーセントエンコードを解かずに文字・数字を探していた
  （#774）。`%c2%a0`（NBSP）・`%e2%80%94`（ダッシュ）等だけのスラッグは、
  WordPressが空にし、公開済みの投稿ではタイトルからスラッグを作り直すが、
  検査を通っていた。WordPressが保存で空にする値だけを断る（#779）

- スラッグの検査で、WordPressが保存できる値を断っていた（#779。#774の
  後退）。`%c3%97`（×。WordPressは`x`にする）、`→`・`%e2%86%92`等の記号、
  `・`等の全角の記号、`_`は、WordPressが残すので通す。WordPressの
  `sanitize_title_with_dashes`と同じく、NBSP・ダッシュ・空白をハイフンにし、
  ソフトハイフン・曲がった引用符・`…`・`©`・幅の無い文字等を取り除いて、
  何も残らない値（`!!!`・`%c2%a0`・`%e2%80%94`等）だけを断る。WordPressと
  同じく、`£`（`remove_accents`が消す）を先に取り除き、符号化した後の200文字を
  超える部分は見ない。`é`等の`remove_accents`がASCIIにする文字は1文字と
  数え、エンティティ（`&amp;`等）は切り詰めた後に取り除く。タグは、PHPの
  `strip_tags`と同じく、引用符の中の`>`では閉じず、閉じないタグは残りを
  全部消し、`<?`は`?>`まで消す（`<?xml`は普通のタグ）。ノーブレーク
  ハイフン（WordPress 6.9からハイフンになる）だけの値、切り詰めで残った`%`
  だけの値も断る。WordPressがRESTの引数と保存で2回整えるのに合わせ、1回目で
  記号が取り除かれてできる`%xx`（`%c2!%a0`）も確かめる

- PUT（投稿の更新等）で、成功の状態コードの後に本文の受信が失敗すると、
  同じ変更を送り直していた（#774）。成功は分かっているので、送り直しても
  書き込みを繰り返すだけで、その間の他人の編集を戻しうる。POST・DELETEと
  同じく送り直さず、処理済みかもしれないので確かめるよう案内する
  （時間切れ・5xxでは、今までどおりPUTも送り直す）

- WordPressの成功の応答の本文に不正なUTF-8があると、置き換え文字にして
  読み進めていた（#774）。バイトのままJSONとして読み、読めなければ
  エラーにする（再送はしない）

- `mcp-rs database-keyring set`で、`[database]`が既にその項目
  （`keyring = "<名前>"`）を使っていて、ホスト・ポート・ユーザー名を変える
  とき、設定の変更を聞く前に項目を上書きしていた（#774）。断ったり、設定
  ファイルを書けなかったりすると、項目と設定が食い違い、起動でデータベースを
  使えなくなった。保存の前に聞き、断れば項目も設定ファイルも変えない。設定
  ファイルを書けない・書いた項目の読み戻しに失敗したときは、項目を前の中身に
  戻す。`keyring`は前後の空白を除いて比べる。同じ接続先のパスワードの
  入れ直しは今までどおり。保存の直前に、項目が別の種類（WordPressの資格
  情報等）になっていないかを確かめ直す。展開できた値は、`${`を含んでも
  既定にする

- `mcp-rs init`・`database-keyring set`で、データベースのパスワードを、TLS等の
  残りの質問と設定の確認より前に資格情報ストアに入れていた（#774）。途中で
  止めると、どの設定にも無い項目が残ったり、置き換えると答えた項目が前の
  中身に戻らなかったりした。すべての質問と確認の後で入れる

- `mcp-rs init`で、Ctrl+Cで止めたとき、新しく作ったWordPressの資格情報の項目の
  うち、まだ設定ファイルに書いていないものを消さなかった（#774）。Ctrl+Cや
  エラー（書いた後の読み戻しの失敗も含む）で終わったときも消す。入れている
  間に別のところで同じ名前の項目が作られたら、消さずに前の中身に戻す

- `mcp-rs init`で、`[handlers.wordpress]`が無効（`enabled = true`でない）で
  `url`が残っていると、「別のサイトを足す」に進み、資格情報を入れた後で
  断られた（#774）。最初のサイトとして足し、`enabled = true`にする。前の
  `username`・`password`・`encrypted_credentials`は`keyring`と一緒に使えない
  ので消す（`timeout_seconds`等は残す）

- `mcp-rs init`で、環境変数`MCP_RS_CONFIG`が相対パスのとき、`init`の
  フォルダで絶対パスにして比べ、一致すれば進んでいた（#774）。クライアントは
  それぞれの作業フォルダで読むので、別のファイルを読みうる。相対パスなら
  止め、変数を消すか絶対パスにするよう案内する。重複する登録を消すかの
  判断は、今の設定ファイルだけで決める（実行中に手で消したサイトを、移した
  とみなさない）

- 環境変数`MCP_RS_CONFIG`のパスのフォルダ名等に`${`を含むと、拡張機能が
  埋めなかった値とみなし、使わなかった（#774）。値の全体が`${...}`のとき
  だけ無いものとする

- `--config`無しで起動するとき、`~/.config/mcp-rs/config.toml`を権限等で
  確かめられないと、無いものとして設定なしで起動していた（#774）。理由を
  付けてエラーにする

- MySQL・MariaDBで、`host = "[::1]"`のような角括弧付きのIPv6アドレスに
  つなげなかった（#774）。角括弧を外して渡す

- READMEのセキュリティの説明で、AES-GCM-256での暗号化をデータベースの
  パスワードにも使えるように書いていた（#774）。WordPressの資格情報だけ

- `resources/list`の`description`・`mimeType`が、無いときに省かれず`null`に
  なっていた（#760）。MCPのスキーマでは、どちらも省いてよい文字列で、厳しい
  クライアントは一覧ごと拒みうる。`resources/templates/list`と同じく、無い
  ときは省く

- 同じ名前のツールを2つのハンドラーが持つとき、先順位のハンドラーが一時的に
  一覧を返せないと、`tools/call`は後順位のツールの定義（`readOnlyHint`等）で
  認可し、実行のときに一覧を取り直して、復旧した先順位のハンドラーで実行
  しえた（#760）。制限のある呼び出し元（`@read_only`等）が、使えないツールを
  呼べた。認可に使ったハンドラーとツールの定義で、そのまま実行する

- 制限のある呼び出し元のリソースの読み取り（`resources/list`・
  `resources/read`・`resources/templates/list`）が、対応するツールを、
  リソースのハンドラーの中だけで探していた（#760）。後順位のハンドラーの
  対応するツールが、先順位の同じ名前のツールに隠れていても、隠れた方の
  `readOnlyHint`で読めた。`tools/call`と同じ公開のルーティングで選ばれる
  ツールで判定し、それがリソースのハンドラーのものでなければ読ませない

- ライブラリとして使う場合: IDSの`IDSConfig`の`enabled: false`・
  `alert_enabled: false`・`min_confidence_threshold`が効いていなかった
  （#760）。`enabled: false`なら分析せず、統計・学習の状態も変えずに、
  遮断を求めない結果を返す。`alert_enabled: false`なら、検知・遮断は
  そのままで、アラートだけ作らない。信頼度が`min_confidence_threshold`に
  届かない検知は、侵入とせず、推奨アクションも`Monitor`にする。mcp-rs
  本体はこれらを設定できず、本体のIDSは信頼度の下限を設けない（今までと
  同じ）

- ライブラリとして使う場合: IDSのシグネチャ検知の`detection_type`を、
  一致したパターンの名前で組み込みのパターンから引き直していた（#760）。
  カスタムルール（`add_custom_rule`）だけが一致すると`Other`になり、
  組み込みと名前が重なると、一致していない組み込みの型になった。最初に
  一致したルール（組み込み、次にカスタムの順）の型を返す

- ライブラリとして使う場合: IDSのポートスキャンの判定で、しきい値
  （`port_scan_threshold`、既定は20パス）を超えても、超過の割合だけを
  スコアにしたので、33パスになるまでポートスキャンとしなかった（#760）。
  DDoS（#759）と同じく、しきい値を超えたら判定の値より大きいスコア
  （0.75から、しきい値の2倍で1.0）にし、超えなければ0にする。mcp-rs
  本体は`/mcp`・`/`のパスしか渡さないので、当たらない

- ライブラリとして使う場合: `init_logging`は、全体のtracingのsubscriberが
  既にあると、`.init()`でpanicしていた（#760）。panicの前に、使わない
  ログファイルに起動マーカーを書き、出力先（`current_log_destination`）も
  公開していた。先にsubscriberを設定し、設定できなければ（既にあれば）、
  何も書かずにエラーを返す。ログファイルを選んで起動マーカーを書き、
  出力先を公開するのは、設定できてから。使う側が既に`log`クレートの
  loggerを設定していても、`log`の橋渡しを設定できないことは警告（標準
  エラー出力）だけにし、ログは今までどおりファイル等に書く

- ライブラリとして使う場合: `HttpTransport`のチャンネル（`receive_message`・
  `receive_request_with_caller`）の応答を、クライアントの付けた`id`だけで
  照合していた（#760）。別のクライアントが同じ`id`を使うと、後の登録が先の
  登録を上書きし、応答が入れ替わった。要求ごとに内部の相関IDを付けて照合し、
  返すときに元の`id`に戻す。期限の後に届いた遅い応答（待ち先の無い内部の
  相関IDの応答）は、次の要求にも要求のキューにも入れずに捨てる。`id`が
  文字列・数値でない要求（`id: true`・`id: null`等）は、置き換えずに、
  `id: null`のInvalid Request（`-32600`、HTTPは400）で断る（`id`の無い
  通知は今までどおり）。
  クライアントが切る等でHTTPの要求が途中で捨てられると、応答の待ち先の登録が
  残ったので、その要求の登録だけを消す。キュー（1000件）が埋まると、
  `send().await`が期限なしに待ち、応答の期限が始まらなかったので、HTTP・
  WebSocketとも、キューの枠を期限（30秒、応答を待つ時間を含む）付きで取る
  （HTTPは取れなければ503）。キューに入れた後、受信側が取り出す前に、要求の
  期限を過ぎたか、応答を待つ処理が捨てられた（HTTPの要求の処理が途中で
  捨てられた等）要求は、受信側に渡さずに捨てる（408を返した後に処理され、
  やり直しで副作用が重ならないように。通知は今までどおり渡す）。
  WebSocketでは、応答を待つ間にクライアントが切っても、それだけでは捨てず、
  期限まで待つ。受信側が要求として読めない本文（JSONのオブジェクトでない・
  `jsonrpc`が`"2.0"`でない・`method`が文字列でない・`params`がオブジェクトか
  配列でない）は、通知として渡さず、Invalid Request（`-32600`、HTTPは400）で
  断る（受信側が読めずにエラーを返し、受信のループが止まらないように）。WebSocketで、枠を待つ間にアクセストークンの期限が切れたら、要求・
  通知を渡さずに接続を閉じる（HTTPと同じ）。2026-07-28版の要求の応答に、直接処理と同じ
  HTTPのステータス（`-32601`は404等）を付ける（以前は常に200）。mcp-rs本体は
  `with_request_handler`を使うので、動きは変わらない

- ライブラリとして使う場合: `HttpTransport`の管理のAPIを直した（#760）。
  `is_connected()`は、起動の前や起動に失敗した後も`true`だったので、
  待ち受けているときだけ`true`を返す。動いている間に`start_server()`を
  また呼ぶと、`bound_addr()`が消えるか、別のリスナーのアドレスで上書き
  されたので、起動を1つずつにし、アドレスを変えずにエラーを返す。
  `Transport::stop()`は何もせず、新しい接続を受け付け続けたので、待ち受けを
  やめ（ポートを放し）、HTTPの接続を閉じ、`/ws`の接続も閉じ、そのタスクが
  終わってから返る（キューの枠・応答を待っていた`/ws`の要求や、読まない
  相手への応答の送信も、そこでやめる）。止め始めた後は、HTTP（HTTP/2の
  ストリームを含む）・`/ws`とも、要求をキューに入れない（HTTPは503）。止める途中で
  捨てられても、止まるまでは`is_connected()`が`true`のままで、呼び直すと
  同じタスクを待つ。`connection_stats()`は、Tokioのタスクから呼ぶとパニックした
  （`blocking_read()`）ので、同期のロックで写しを取る

- ライブラリとして使う場合: `TransportError`への変換（`From<Error>`）で、
  `Error::TransportError`に包まれた元の種類（`Unauthorized`・
  `Configuration`等）が`Internal`になっていた（#760）。そのまま返す。
  `Transport::start()`のエラーも、`Internal`にせず元の種類で返す。
  `TransportType::WebSocket`の表示は、URLに`ws://`・`wss://`があっても
  `ws://`を付け、`ws://wss://...`になっていたので、スキームがあれば
  そのまま出す

- ライブラリとして使う場合: PostgreSQLのエンジンの`health_check`が、
  データベースに触れず、つなげなくても、接続先が止まっていても`Healthy`を
  返していた（#758）。接続先に`SELECT 1`を送り、答えれば`Healthy`、つなげない・
  答えない・期限（`query_timeout`）を過ぎたときは`Critical`と理由を返す
  （まだつないでいなければ、エンジンの設定でつなぐ）。`DatabasePool::
  health_check`も、エンジンの結果のほうが悪ければそれを返し、理由も返す。
  mcp-rs本体はこれらを使わない

- ライブラリとして使う場合: PostgreSQLのエンジンの`get_version`が、接続先に
  よらず`PostgreSQL 15 (Mock)`を返していた（#758）。`SHOW server_version`で
  読み、`PostgreSQL 16.4`の形で返す。つなげない・読めないときはエラー

- ライブラリとして使う場合: PostgreSQLのエンジンは、最初に作ったプールを
  以後の`connect`で共有するので、別の接続先（ホスト・ポート・データベース・
  ユーザー）の設定で`connect`すると、最初の接続先のプールで動き、接続の
  情報には後の接続先が出ていた（#758）。エンジンを作った設定と違う接続先
  なら`ConfigurationError`で断る（別の接続先には、別のエンジンを作る）。
  プールはいつもエンジンの設定の接続先に作るので、`health_check`・
  `get_version`が確かめる接続先とも合う

- MongoDB・Redisのエンジン（`mongodb-backend`・`redis-backend`）は、
  データベースのハンドラー（`[database]`）では使えないことを、`Cargo.toml`・
  ドキュメントコメント・文書に書いた（#758）。v0.31.0で使えるようにする予定
  （Redisは #529、MongoDBは #530）

- WordPressの`create_advanced_post`に`excerpt`が無かった（#759）。
  `draft_post`のプロンプトは抜粋を書かせるのに、保存できなかった。
  `excerpt`を受け、WordPressに送る（省略時は、今までどおりWordPressが
  本文から作る）。抜粋に対応しない独自の投稿タイプでは、WordPressが
  黙って捨てる（応答に`excerpt`が無い）ので、作ったことと、抜粋は保存
  されていないことを伝える（投稿・固定ページは保存する）

- WordPressのリソースの読み取り（`resources/read`）が、エラーの本文に
  ` 404 `を含むだけで「リソースが無い」（`-32002`）にしていた（#759）。
  503の本文に`upstream 404 from proxy`等があっても、無いことになった。
  WordPressの応答の状態コードが404のときだけにする

- WordPressの`create_comment`は、いつも`status`（既定は`hold`）を送るので、
  `moderate_comments`の権限の無いユーザー（投稿者・寄稿者）では、
  WordPressが`rest_comment_invalid_status`で断り、作れなかった（#759）。
  ツールの説明に権限が要ることを書き、断られたときは権限を案内する。
  `status`を外して送り直すと、すぐに公開されうるので、そうしない

- WordPressの`create_post_with_embeds`に`youtube-nocookie.com/embed/…`を
  渡しても、`www.youtube.com`のプレーヤーを埋め込んでいた（#759）。
  プライバシー強化モードのURLは、`www.youtube-nocookie.com`のまま埋め込む

- WordPressの一覧の全件取得で、合計の上限（64 MiB）を、ページの本文を
  読み終えてから確かめていた（#759。上限の近くでも、次のページを最大
  32 MiBまで読んでいた）。本文の受信に失敗して読み直したページは、失敗した
  試行で受け取った分を数えていなかった。一覧の残りの量までしか読まず、
  失敗した試行の分も数える。失敗した試行で残りを使い切ったら、同じ
  ページを送り直さない

- WordPressの`set_featured_image`が、応答の`featured_media`を確かめず、
  WordPressが画像を設定しなかったとき（無いメディアのID等）も成功と
  返していた（#759）。`create_post_with_featured_image`と同じく、要求した
  IDと照らし合わせ、違えばエラーにする

- WordPressのサイトのURL（`url`）に問い合わせ・フラグメントがあっても
  受け付けていた（#759）。`https://example.com/blog?lang=ja`だと、後ろに
  付けるREST APIのパスが問い合わせの一部になり、REST APIに届かなかった。
  起動のときに断る（`/blog`等のサブディレクトリはよい）

- WordPressの`wordpress_health_check`が問題を見つけて`isError: true`を
  返しても、監査ログには`SUCCESS`と残していた（#759）。WordPressの
  ツールの結果が`isError: true`なら、失敗（`FAILURE`）として残す

- WordPressの`wordpress_health_check`が、サイトに届くか・REST APIを
  使えるかを確かめるとき、応答の大きさの上限（32 MB）無しに、本文を
  全部読んでいた（#759）。ほかの読み取りと同じく、上限までしか読まない

- WordPressの`get_post`が、カテゴリー・タグの無い投稿（固定ページ等）
  でも、サイト全体のカテゴリー・タグの一覧を取得していた（#759）。
  IDのある分類だけ、名前を解決する

- WordPressの`update_post`で、予約済みの投稿に`status: "future"`だけ
  （`date`無し）を渡すと、`future`を送り直していた（#759）。読んだ後に
  別の編集者が予約を取り消していると、予約が戻るか、すぐに公開された。
  予約を保つだけなら`status`を送らない

- WordPressの一覧の全件取得で、読んでいる間に読み終えたページの項目が
  消えると、後ろのページがずれて1件欠けた（#759。件数の照合も通って
  いた）。1ページ目の総件数・総ページ数を覚え、変わったら初めから
  読み直す。減って後ろのページが範囲の外になり、WordPressが400の
  `rest_post_invalid_page_number`等を返したときも、初めから読み直す

- WordPressの`get_posts`に、カテゴリー・タグの無いカスタム投稿タイプで
  `categories`・`tags`を渡すと、そのまま送っていた（#759）。WordPressは
  絞り込みを無視し、絞り込んでいない一覧を返した。書き込みと同じく、
  投稿タイプのタクソノミーを調べ、無ければ送らずに断る

- WordPressの`wordpress_health_check`の「Media Upload Possible」は、
  メディアの一覧を読めるかで決めていて、アップロードの権限
  （`upload_files`）が無くても✅になった（#759）。「Media Readable」と
  出し、エラーの文も`Media read check failed`にする（`mcp-rs check`も）

- MySQL・MariaDBの時間の上限で`KILL QUERY`を送る前の確認が、止める
  接続の届いたサーバーの識別子が元と違っても、文の始めが同じなら止めて
  いた（#759）。プロキシ越しに、別のサーバーの同じID・同じユーザー・同じ
  文を止めうる。識別子が両方読めて違えば、止めない

- MySQL・MariaDBの`KILL QUERY`の前の確認で、サーバーの識別子
  （`@@server_uuid`等）が同じなら、文を確かめずに止めていた（#759）。
  識別子は再起動しても変わらないので、再起動の後に同じユーザーの別の
  セッションが同じ接続IDを得ると、止めうる。`Uptime`も読み、再起動して
  いれば止めない。起動が同じか分からないときは、文の始めで確かめる

- MySQL・MariaDBの時間の上限を超えたとき、`KILL QUERY`・切断が成功
  したか、利用者のSQLを送ったかに関係なく「was stopped」と返していた
  （#759）。`KILL QUERY`で止めた・接続を閉じただけ・止められず実行が
  続きうる・送る前だった、を分けて返す

- MySQL・MariaDBの`get_schema`で、ビューの`definition`が常に空だった
  （#759）。`information_schema.VIEWS`の`VIEW_DEFINITION`を返す。読む
  権限が無いビューは空のまま

- SIGTERM（`docker stop`・`systemctl stop`等）で終わると、プラグインを
  止めず、コンテナ・子プロセスが残っていた（#759。STDIO・HTTPとも）。
  Ctrl+Cと同じく、プラグインを止めてから終わる（WindowsではCtrl+Breakも）。
  SIGTERM（Ctrl+Break）を受けられないときも、警告してCtrl+Cは受ける

- プラグインの起動中（HTTPモードはHTTPサーバーを始めるまで）にCtrl+Cを
  押すと、プラグインを止めずに終わっていた（#759）。終了の合図はプラグインを
  起動する前から受け、起動を終えてからプラグインを止めて終わる

- ログの保持（`retention`の`Size`・`Count`・`Days`）を、起動したときにしか
  適用していなかった（#759）。長く動き続けると（特に`Hourly`）古いログが
  たまった。ファイルを切り替えるたびにも、別のスレッドで適用する

- ログの保持が、`mcp-rs`で始まり`.log`を含む名前のファイルをすべて消して
  いた（#759。`mcp-rs-backup.log`等、mcp-rsが作っていないファイルも）。
  mcp-rsが作る名前（`mcp-rs.log`・`mcp-rs.log.YYYY-MM-DD`・
  `mcp-rs.log.YYYY-MM-DD-HH`）の通常のファイルだけを消す

- 同じフォルダに複数のmcp-rsがログを書くとき、ログの保持が、別のプロセスが
  書いている今のファイルや、日付が変わった後にまだ書いていない別のプロセスの
  前日のファイルを消していた（#759。`Count(0)`・`Days(0)`・小さい`Size`）。
  今の期間と1つ前の期間のファイルは消さず、数・大きさにも数えない（別の
  プロセスは別の`rotation`かもしれないので、日・時・切り替えない名前の
  どれも。前の日は暦で求め、夏時間の切り替えの日も漏らさない）。
  古い期間の名前でも、1期間（`Hourly`は1時間、ほかは1日）の間に書かれた
  ファイルは消さない（切り替えに失敗した別のプロセスが書き続けるため）

- HTTPで、HTTP/2の相手が応答を読まないと、接続を閉じなかった（#759）。
  本文が送信のウィンドウを待つ間、処理中のリクエストとして数え続け、
  何もしていない接続として閉じることも、keep-aliveのpingで見つけることも
  なく、接続の枠（`max_connections`）を持ち続けられた。本文のある応答は、
  送信が`timeout_ms`（と、大きさに合わせた時間）の間に進まなければ、
  接続を閉じる。最後のデータをh2に渡した後も、相手のウィンドウが空いて
  送れるようになるまで、期限を外さない（最後の1バイトを、送れるように
  なってから終わりと一緒に渡すので、ウィンドウが本文の長さちょうどの相手
  にも最後まで送れる）

- HTTPの`enforce_https`（`tls_enabled`なし）で、`Forwarded`ヘッダー全体
  から`proto=https`を部分文字列で探していた（#759）。クライアントが
  `Forwarded: for=192.0.2.1;proto=https`を送ると、プロキシが`proto=http`の
  要素を足しても通った。要素・パラメーターに分け、クライアントから受けた
  プロキシが足した要素（右から見て、`for`が`trusted_proxies`でない最初の
  要素。すべてが`trusted_proxies`なら、いちばん右の要素）の`proto`だけを、
  完全一致で比べる。形の正しくないヘッダー（閉じていない引用符等）は、
  HTTPSとみなさない

- HTTPの`/ws`で、OAuthのトークンの期限の直前に受け取ったメッセージが、
  リプレイ攻撃対策・IDSの確認の間に期限を過ぎても、そのまま処理していた
  （#759）。処理を始める直前に期限を確かめ直し、過ぎていれば接続を閉じる

- `tls_enabled = true`で`min_tls_version = "1.4"`等を指定しても起動し、
  TLS 1.3の接続を受け付けていた（#759）。mcp-rsが自分でTLSを終えるときは
  TLS 1.3だけを使うので、それより新しい版を求める設定では起動しない
  （プロキシの後ろでは、今までどおりプロキシの知らせる版と比べる）

- HTTPで、対応していない`MCP-Protocol-Version`を指定した要求の`id`が
  オブジェクト等のとき、そのまま応答の`id`に返していた（#759）。文字列・
  数値以外は`null`にする

- STDIOで、ツールの処理中は次の行を読まなかった（#759）。
  `notifications/cancelled`も`ping`も処理が終わるまで待たされ、取り消しも
  ログに出すだけだった。読み取りと実行を分け、`ping`にはすぐ答え、
  `notifications/cancelled`は、処理中か順を待っている同じ`id`の要求を止める
  （応答は返さない）。ほかの要求は今までどおり受け取った順に1つずつ実行し、
  応答の書き込みは1つずつ行う。順を待つ要求は64件（合わせて16 MiB）までで、
  超えた形の正しい要求には`-32000`（server busy）を返し、通知は捨てる
  （形の正しくないものには、その誤りを返す）。順を待つ要求を取り消せば、
  その場で空く。相手が出力を読まなければ、応答を溜め続けず、読み取りも
  止まる。出力に書けなくなれば、実行中の要求を止めてエラーで終わる。
  Ctrl+C等で終わるときも、実行中の要求を止め、順を待つ要求は実行しない。
  標準出力に書けずに終わるときは、プラグインを止めた後、標準入力の終わりを
  待たずに終了コード1で終わる。ハンドラーが異常終了すれば、それ以上は
  受け付けずにエラーで終わる。空の行は数えない。`reason`が文字列でない
  取り消しは無視する。読み取りだけ（`readOnlyHint: true`。印を信じない
  ハンドラーでは数えない）でないツールの呼び出しは、取り消しても途中で
  止めず、最後まで行って応答だけを返さない。WordPress等に変更を加えた後、監査
  ログに残す前に止めると、記録の無い変更が残り、プラグインでは送信の途中で
  止めると接続ごと止まるため（プラグインには取り消しを伝えない）。その間も
  `ping`にはすぐ答える。`ping`の応答を出力に渡せずに待つ間も、実行の
  タスクの異常終了を見逃さない

- JSON-RPCの形を確かめずに、通知か要求かを決めていた（#759）。`id: null`は
  通知として黙って捨て、`jsonrpc`が`"2.0"`でなくても処理し、型の並びが
  合う配列も要求として読んでいた。オブジェクトであること・`jsonrpc`が
  `"2.0"`であること・`id`があれば文字列か数値であることを先に確かめ、
  違えば`-32600`（Invalid Request）を返す。`params`は、オブジェクト・配列
  （か`null`）でなければならない

- IDSのCMD-002（Critical）が、バッククォートで囲んだ語なら何にでも一致して
  いた（Markdownの`` `id` ``・`` `foo bar` ``でHTTP 403と永続ブロック）。
  中がシェルのコマンドと引数らしいもの（`` `cat /etc/hosts` ``）、パスを
  付けたコマンド（`` `/bin/id` ``）、`whoami`・`uname`、値に続けた
  コマンド（`` name=`id` ``）だけを見る。CMD-001も、バッククォートを
  区切りとして見ない（#759）

- IDSのCMD-003（Critical）が`$(...)`なら何にでも一致していた（jQueryの
  `$(document)`・`$('.x')`でHTTP 403と永続ブロック）。`$(`の中が
  シェルのコマンド（`$(id)`・`$(cat /etc/hosts)`・`$(id>/tmp/x)`等）の
  ときだけ見る（#759）

- IDSのPATH-008（High）が`.env`・`.config`・`.ini`・`.conf`を語の途中でも
  探していた（`.initial`・`.conference`でHTTP 403）。パス・値の始まりか
  `/`・`\`・`=`の後のファイル名が、これらで終わるもの（`/.env`・
  `/app/config.ini`・`../.env`・`.env.local`等）と、引用符で囲んだ値の
  全体がそのファイル名のもの（JSONの`{"file":".env"}`）だけを見る。
  `/`・`\`の後の名前は空白を含んでもよい（`/app/my config.ini`）（#759）

- `ids_enabled = true`で、IDSのCMD-001（Critical）が`;`・`&`・`|`・`$`・
  `<`・`>`の1文字だけで一致していた。`Cookie: a=b; c=d`、ブラウザの
  User-Agentの`;`、本文の`&`・`<`だけでHTTP 403になり、送り元のアドレスも
  ブロックした。区切りの後にシェルのコマンド（`rm`・`curl`・`sh`・`id`・
  `kill`等）と引数らしいもの（`-`・`/`等で始まるもの、数、`victim.txt`
  等のファイル名）等が続くときと、Shellshock（`() { :; };`）だけを見る。
  CMD-004（`| id`）・CMD-005（`; del`）も同じく絞り、`| identity`・
  Markdownの表の`| id |`・`; delivery`では一致しない（#759）

- IDSが、同じ名前のクエリの値（`?q=x%3Bid&q=benign`）を`, `でつないで
  調べていたので、値の終わりを見るCMD-001が`x;id, benign`に当たらな
  かった。値を1つずつ調べる。`RequestData::query_params`は
  `HashMap<String, Vec<String>>`になる（#759）

- IDSが、JSONの本文をエスケープ（`\"`等）が残ったまま調べていたので、
  引用符付きのパラメーターのdata: URI（`"data:application/javascript;
  name=\"a b\";base64,..."`）がXSS-011に当たらなかった。本文のままに
  加え、デコードした文字列の値も1つずつ調べる（#759）

- IDSのXSS-011（High）が`base64`という語だけで一致していた（本文の
  `"encoding":"base64"`でHTTP 403）。`data:text/html;base64,`等、
  スクリプト・HTMLとして実行される種類の`data:` URIだけを見る（種類と
  `;base64,`の間の`charset=utf-8`等のパラメーターはいくつあってもよい）。
  `data:image/png;base64,`等の画像は一致しない（#759）

- IDSのDDoSの判定で、設定したしきい値（`ddos_threshold_rps`、既定50）が
  スコアに効いていなかった（51件で0.02、DDoSとするのは86件から）。
  しきい値を超えたら、判定の値（0.7）より大きいスコア（0.75から、2倍で
  1.0）にする（#759）

- IDSの一時ブロックが、同じアドレスの永続ブロックを上書きしていた。同じ
  アドレスの2件が並んで通ると、永続ブロックの後に入った一時ブロックで、
  30分後に解けた。既にブロック中なら短くしない（永続はそのまま、一時は
  期限の遅いほう）。書き込みロックの中で確かめる（#759）

- IDSの振る舞いの学習で、頻度を常に毎分1件として学習していた。分散が0に
  近づき、普通の通信でも頻度の異常スコアが1.0になった。呼び出し元ごとに、
  1分の窓の要求を数え、窓が終わるたびに、その頻度で平均と標準偏差を学習
  する（窓の中のバーストに、ベースラインが慣れない）。今の頻度（直前の
  1分）も呼び出し元ごとに上限なしで数え（以前は全体で10,000件までの
  履歴で数え、頻度の高い呼び出し元や、他の呼び出し元の要求が多いときに
  少なく数えた）、学習する前のベースラインと比べ、平均より多い側だけを
  見る。
  標準偏差には1分の数のばらつき（少なくとも1件）の下限を置き、学習した
  窓が3つになるまでは頻度を判定しない（#759）

- STDIOモードで、ログをファイルに書けずコンソールだけにするとき（と、
  `console`・`file`のどちらも無効のとき）、標準出力にログを書いていた
  （#755）。起動のログ等が、MCPのJSON-RPCの流れに混ざった。標準エラー出力に
  書く

- ログの保持（`retention`の`Size`・`Count`・`Days`）が、今書いているログ
  ファイルも消していた（#755。`Size`で今のファイルが上限を超えているとき、
  `Count(0)`等）。mcp-rsは開いたファイルに書き続けるが、そのパスからは
  読めなくなった。今のファイルは消さず、数・大きさにも数えない

- サイトが1つのWordPressの設定で、`enabled = false`（か省略）でも
  `encrypted_credentials`を復号し、`WORDPRESS_MASTER_PASSWORD`が無いと
  起動しなかった（#755。WordPress以外も）。使わない設定では復号しない
  （複数のサイトの形は#684で直っていた）

- ライブラリとして使う場合: `McpConfig::save_to_file`が、
  `encrypted_credentials`から復号したユーザー名・パスワードを平文で
  書いていた（#755）。`keyring`と同じく書かない。
  `DynamicConfigManager`（`switch_config_file`・`reload_config`）が、
  `[transport] type = "…"`と書いた設定ファイルを`duplicate field`で
  読めなかった（#755）。`McpConfig::load`と同じく`transport_type`に
  そろえてから読む（TOMLとして読む）

- WordPressの`update_post_categories_tags`・`update_post`に、固定ページ
  （`post_type: "page"`）や、カテゴリー・タグの無いカスタム投稿タイプで
  `categories`・`tags`を渡すと、そのまま送り、WordPressは黙って無視するのに、
  成功と返していた（#755）。投稿を作るツール（`create_advanced_post`・
  `create_post_with_embeds`）は、投稿以外では黙って落としていた。投稿タイプの
  タクソノミーを調べ（`terms`と同じく`/wp/v2/taxonomies?type=`）、無ければ
  送る前に断る（空の配列は断らない）。投稿（`post`）は調べない。カテゴリー・
  タグを持つカスタム投稿タイプには送る

- WordPressの`categories`・`tags`のIDの配列で、正の整数でない要素を黙って
  落としていた（#755）。`tags: [7, 7.5]`は`[7]`になり、置き換えで今のタグが
  外れた（`update_post_categories_tags`・`update_post`・`create_advanced_post`・
  `create_post_with_categories_tags`・`create_post_with_embeds`）。`terms`と
  同じく、正の整数でない要素が1つでもあれば断る（`[]`は今までどおり受け付け、
  すべて外す）。配列でない値も断る。ツールのスキーマの要素も、`number`から
  `integer`（`minimum: 1`）にした。`7.0`は`7`と同じに読む（`terms`・`get_posts`の
  絞り込みも）。同じ分類を`categories`と`terms.categories`のように2つの
  引数で渡したら断る（作成と更新で、どちらを送るかがずれ、片方を黙って
  捨てていた）

- WordPressの監査ログの対象のID（`target_id`）を、決まった順のキー（`id`が
  先）で探していたので、`delete_post`に`post_id: 42`と余分な`id: 7`を渡すと、
  42を消して7と記録していた（#755）。ツールごとに、操作に使うキー
  （`post_id`・`media_id`等）から取る。対象のIDを持たないツール・知らない
  ツールでは記録しない

- WordPressの一覧の全件の取得で、ページの間に一覧が変わると（先頭に1件増え、
  末尾が1件減る等）、前のページの最後が次のページに再び現れ、件数も合うので
  気付かなかった（#755）。ページをまたいでIDの重なりを確かめ、重なれば初めから
  1度だけ読み直し、それでも重なればエラーにする（後でやり直すよう案内する）

- `mcp-rs encrypt-wordpress-credentials`で、NULを含むマスターパスワードを受け付けて
  いた（#750）。同じ値を`WORDPRESS_MASTER_PASSWORD`に設定できないので、
  作った設定では起動できなかった。暗号化する前に断る（エラーに値は出さない）

- 例`examples/auth_diagnosis.rs`・`examples/comprehensive_test.rs`が、
  パスワードの先頭8文字を出していた（#750）。設定済みかだけを出す

- `tls_enabled = true`でも、起動のログの`/mcp`のURLが`http://`だった
  （#750）。`https://`にする

- 文書: `plugins.md`・`security.md`・`wordpress.md`のパンくずが、ホーム・
  文書の一覧を指していなかった。`plugins.md`の「ログに起動の行が必ず出る」は、
  `log_level`が`info`以上のときだけと書いた。PostgreSQLのガイドの依存の
  書き方を、crates.ioではなくGitHubのタグにした。`update_post`の引数に
  `post_type`を足した（#750）

- プラグインの中継で、ツールの呼び出しの期限が、要求を書き込む途中で切れると、
  送りかけのフレームがパイプに残り、次の要求がその続きになって、プラグインが
  読めなくなっていた（#750）。書き込みの途中で中断したら、その接続を止める
  （応答を待つ間の期限切れは、今までどおり使い続ける）。期限の切れた前の
  要求への、`max_response_bytes`を超える応答が届くと、待っている要求を失敗に
  していた。捨てる行の先頭から`id`を読み、待っている要求のものでなければ
  読み飛ばす。行の上限を、末尾のLFと、その直前のCRを除いた長さで比べる
  （上限より1バイト長い行が、LFで終わると通っていた）

- プラグインが`isError: true`の結果を返しても、監査ログに`SUCCESS`と
  記録していた（#750）。`FAILURE`として記録する（中継する結果は変えない）

- プラグインが`initialize`に、mcp-rsの知らないMCPの版で答えても、警告だけで
  使っていた（#750）。初期化の通知を送る前に、起動のエラーにする（仕様では、
  クライアントは切断すべき）。
  **互換性**: 2025-11-25・2025-06-18・2025-03-26・2024-11-05以外の版で
  答えるプラグインは、起動しなくなる

- 2024-11-05の版のクライアントに、`content`が空で`structuredContent`だけの
  ツールの結果を返すと、`structuredContent`を除くので、空の成功になっていた
  （#750）。`content`が空なら、`structuredContent`を文字列にした
  テキストを入れる（プラグインの結果に`content`が無ければ、今までどおり
  仕様に合わない結果としてエラーにする）

- `[[plugins]]`の`enabled = nan`等の不正な値を、省略と同じに扱い、
  プラグインを既定の値で起動していた（#750。`call_timeout_seconds`等の数も）。
  読み込みのエラーにする。`[[plugins]]`の後の`[plugins.env]`・
  `[plugins.rate_limit]`を、古い形式の`[plugins]`とみなし、別の読み込みの
  エラーに、古い形式を消す案内を付けていた。`[[plugins]]`より後の
  `[plugins.*]`は古い形式としない

- WordPressの`create_post_with_featured_image`で、無いメディアのIDを渡すと、
  WordPressは画像なしで投稿を保存するのに、画像付きで作ったと返していた
  （#750）。応答の`featured_media`と頼んだIDを比べ、違えば、作った投稿の
  IDと、画像を設定していないことを返す（`set_featured_image`を案内する。
  投稿は作ってあるので、エラーにはしない）

- WordPressの`create_advanced_post`（と、それを通る投稿を作るツール）が、
  `429`・接続の失敗でも再送しなかった（#750）。ほかのPOSTと同じく
  `send_with_retry`を通し、WordPressに届いていない`429`・接続の失敗だけ
  再送する（5xx・期限切れは、重なるので再送しない）

- WordPressへのGET等で、成功の状態コードを受けた後、本文の受信が途中で
  失敗すると（期限切れ・接続の切断）、再送しなかった（#750）。再送して
  よいメソッドは、本文の受信まで再試行に含める（受信しきった不正なJSONは
  再送しない）。POST（`upload_media`・投稿を作るツール）・DELETEでは、
  再送せず、WordPressで保存・処理済みかもしれないので確かめてからやり直す
  よう、エラーに書く（以前は、ただの通信のエラーで、やり直すとメディア・
  投稿が重なり得た）

- WordPressの投稿を作るツールのスキーマが、`status`を`enum`で標準の値に
  縛っていたので、スキーマを確かめるクライアントは、サイトに登録された
  独自のステータス（PublishPressの`pitch`等）を送れなかった（#750。
  mcp-rsは通す）。`enum`を外し、標準の値は説明に書く

- WordPressの`slug`に、文字・数字を含まない値（`!!!`・`&amp;`等）を渡せた（#750）。
  WordPressはこれを空にするので、下書きの更新では今のスラッグが消えた。
  空と同じく断る（日本語・パーセントエンコードのスラッグは通す）

- WordPressの`url`に、ユーザー名・パスワード（`https://user:pass@…`）を書くと、
  `mcp_rs_status`の表示と起動のエラーに、パスワードがそのまま出ていた（#746）。
  そのような`url`と、URLとして読めない`url`は断る（`mcp-rs init`と同じ）。
  エラーには、URLを出さない。
  **互換性**: こう書いた設定ファイルでは、WordPressのツールが起動しなくなる。
  資格情報は`username`・`password`（か`keyring`）に書く

- WordPressの`encrypted_credentials`を使う設定で、使われない`username`・
  `password`に`${VAR}`が残っていると、その変数が無いときに起動しなかった
  （#746。この2つは復号した値で上書きする）。`encrypted_credentials`があれば
  展開しない（`url`は今までどおり展開する）

- ライブラリとして使う場合: `mcp_rs_status`で、認証していない呼び出し元かを、
  動作のモードが`http`かで決めていた（#746）。モードを設定せずにHTTPで
  使うと、認証していない相手にもファイルのパス等を見せていた。接続元の
  IPアドレスがあり、名前の無い呼び出し元にも、バージョンだけを返す

- `mcp-rs --config --version`で、`--version`を設定ファイルのパスとして
  読んでいた（#746。`--help`・`-V`・`-h`も）。どの位置でも、表示して終わる

- 文書で、ログの書き先を「`C:\Program Files\MCP-rs`に入れると必ずユーザーの
  フォルダ」としていたが、そこにも書けなければ次の候補を使う。`${VAR}`の
  変数が無いと起動が止まるものに、`[[transport.http.api_keys]]`の`key`を足し、
  `[[plugins]]`の`env`はその項目を渡さないだけと書いた（#746）

- `deploy.yml`のコメントに、リリースのタグの実行も、待っている間に`main`への
  pushが来ると取り消され、バージョンのタグのイメージを公開しないことと、
  やり直す手順を書いた（#746）。`.cargo/audit.toml`のSQLiteの説明を、
  mcp-rsがSQLiteを開かない本当の理由（`[database]`が`engine = "sqlite"`を
  受け付けない）に直した。例`examples/secure_config_test.rs`を、変数が無いと
  読み込みが失敗する今の動きに合わせた

- ログのファイルを切り替えた直後の書き込みが失敗すると（ディスクがいっぱい
  等）、新しいファイルの先頭の`=== … continued … / instance … ===`の行を
  失っていた（#744）。書けるまで残し、次の書き込みで書く

- 夏時間で時刻が30分だけ飛ぶ地域（`Australia/Lord_Howe`等）で、1時間ごとの
  切り替え（`rotation = "hourly"`）が次の正時まで遅れ、飛んだ後の30分のログが
  前の時間のファイルに入っていた（#744）。飛んだ後の最初の時刻で切り替える

- 起動の行を書いた後の確かめで、書く前のファイルの名前を使っていたので、
  その間に日付が変わると、書けたのに書けないと判断し、ログの書き先を
  代わりの場所に移していた（#744）

- 文書で、ログの行の`[pid …]`を「その上の直近の起動の行」と合わせて読む、と
  していたが、2つのプロセスが同じファイルに書くと、別のプロセスの行になり
  得る。同じ`pid`の直近の起動・`continued`の行と書いた（#744）

- 監査ログを、複数のプロセス（既定の場所は設定ファイルに関係なく同じ。
  Claude Desktopは2つ起動する）が同時にローテーションすると、回した後の
  `audit.log.N`を上書きし、記録を失うことがあった（#741）。隣の
  `audit.log.lock`のロックを取ってから、大きさを確かめ直し、回し、追記する

- Windows以外で、`XDG_STATE_HOME`が空か相対パスでも使い、ログ・監査ログの
  既定の場所が作業フォルダで変わっていた（#741）。XDGの仕様どおり、空・
  相対パスは無視して`~/.local/state`を使う

- HTTPモードで、許可していない`Origin`を付けたCORSのpreflight（`OPTIONS /mcp`）に、
  403ではなく200を返していた（#741）。許可のヘッダーは付けないのでブラウザーは
  先に進めないが、MCPの仕様は、正しくない`Origin`に403を求める。`Origin`の
  検証をCORSより先に行う

- プラグインが、tasks（MCP 2025-11-25）でしか呼べないツール
  （`execution.taskSupport = "required"`）を挙げると、そのまま登録し、呼んでも
  断られていた（#741。mcp-rsはtasksを実装していない）。登録せず、警告を出す

- MySQL・MariaDBで、時間の上限を超えた文・読むのをやめた文を`KILL QUERY`で
  止める前に、止める接続で`information_schema.PROCESSLIST`を見て、そのIDの
  接続が同じユーザーで、同じサーバー（MySQLは`server_uuid`、MariaDBは
  ホスト名・ポート・`server_id`）か、mcp-rsが送った文（最初の`?`の前まで）を
  実行中か確かめる（#723）。プロキシ・ロードバランサーで、止める接続が別の
  サーバーに届くと、同じ番号の関係ない文を止めることがあった。違えば
  `KILL QUERY`を送らず、元の接続を閉じるだけにする。同じサーバーなら、
  トリガーの中の文を実行中でも止める。別のサーバーで、同じユーザーの同じ
  始まりの文は見分けられず、確かめてから送るまでの間に相手が変わることも
  あり得る（減らすが、無くせない）

- PostgreSQLで、`bigint`等の整数の列に、小数部の無い大きな数（`1e20`等）を
  渡すと、`i64`の最大・最小に丸めて、違う値を書き込んでいた（#737）。範囲の
  外は断る

- `execute_query`で、`WITH ids(id) AS (VALUES (1), (2)) SELECT id FROM ids`の
  ように、`WITH`の中で`VALUES`を使う読み取りのSQLを断っていた（#737）。
  `WITH`の中の`VALUES`は通す（単独の`VALUES`は、今までどおり断る）

- `database`機能なしでビルドしたmcp-rsで、`[database]`の`${VAR}`の環境変数が
  無いと、「このビルドはデータベースを使えない」ではなく、環境変数のエラーを
  出していた（#737）。機能が無ければ、先にそう伝える

- PostgreSQLのNUMERICで、桁（1万進）が32768個以上（整数部が約13万桁）の値を、
  送るのも読むのもできなかった（#737）

- ライブラリとして使う場合: `PostgreSqlConnection::new()`・`from_config()`で
  作った、その接続だけのプールを、`close()`で閉じなかった（#737）。共有の
  プール（`with_pool`）は、今までどおり閉じない

- 文書のサイトで、相対の`.md`リンク（`../security.md`・`./guides/postgres-integration.md`
  等、18か所）が404になっていた（#737）。ページは`permalink`の`/docs/security/`
  等に出るが、ビルドにリンクを書き換えるプラグインが無い。`{{ site.baseurl }}`と
  permalinkで書いた。無いページ（Performance Tuning Guide）へのリンクは消した

- PostgreSQLのガイドで、一部の表だけを見せるときに、`ALTER DEFAULT PRIVILEGES`を
  残すと、後で作った表（隠したい表も）を読めることを書いていなかった（#737）

- Securityの文書（Listening on the Network）に、認証は通信を暗号化しないので、
  ほかのPCからつなぐ前にTLS（`tls_enabled`か、TLSを終端するリバースプロキシ）を
  使うよう書いた（#737。平文のHTTPでは、APIキー・トークンを読まれる）。
  プロキシとmcp-rsの間も、同じPCでループバックにするか、mcp-rsでもTLSを使う

- `execute_query`が返した値をそのまま`params`に渡せるのは、挙げた型（uuid・日付・
  numeric等）だけと分かるように、文書とPostgreSQLの`params`の説明を直した（#737。
  配列等はキャストが要る。`json`・`jsonb`は、JSONの値で返るので、JSONの文字列に
  して渡す）

- Redisのエンジン（`redis-backend`）で、パスワードに`@`・`%`・`/`等が
  あっても、つなげる（#724、PR #730のレビュー）。以前は、パスワードを接続の
  URLにそのまま入れ、redisクレートが`%`の符号を戻すので、別の値になって
  認証に失敗した。また、`use_tls = true`は、つなぐ前に「このビルドはRedisの
  TLSを使えない」と断る（以前は、redisクレートのTLSの機能が無いという
  分かりにくいエラーで失敗した）
- データベースの`execute_query`で、受け取った行がバイト数の上限
  （`max_result_bytes`）を超えたら、`truncated: true`にする（#724）。
  以前は、列の多い大きな行が1つだけ残り、値を切り詰めなかったとき、
  後の行を落としているのに`truncated: false`だった
- データベースのエラーを監査ログに書くとき、閉じた値の後に閉じていない
  値が続く文（`value 'a' and 'secret`）でも、最後まで伏せる（#724）
- 設定の型の誤り（`invalid type: string "…", expected u16 for key
  \`transport.http.port\``）で、値だけを伏せ、項目の場所は残す（#724）。
  以前は場所も伏せたので、どの項目を直せばよいか分からなかった
- `mcp-rs wordpress-keyring set`で、`https://`でないサイトを登録しない
  （#724）。WordPressのハンドラーは`https://`しか使わないので、以前は
  `http://`で登録でき、使えなかった
- Unixで、`--config`の`~/`を展開するとき、パスの`\`を`/`に変えない
  （#724）。`~\`の展開と、`\`を区切り文字として扱うのは、Windowsだけ
- ライブラリとして使う場合の修正（#724。mcp-rs本体の動作は変わらない）:
  - `McpConfig::save_to_file`で、`keyring`のサイトの、資格情報ストアから
    読んだユーザー名・パスワードを書かない（以前は平文で書き、読み直せ
    なかった）
  - IDSの`NetworkMonitor`で、IPごとの記録の上限を、しきい値
    （`ddos_threshold_rps`・`port_scan_threshold`）の2倍以上にする（以前は
    固定で、しきい値を上げると、判定に届かなかった）。ポートスキャンは、
    `port_scan_window_seconds`の間に見たパスの数で判定する（以前は累積の
    数で、ゆっくり多くのパスを使う相手もポートスキャンとした）。
    **互換性のない変更**: `TrafficPattern`に`path_last_seen`を足した
    （構造体を直接作っているコードは、足す必要がある）
  - IDSの`BehavioralDetector`で、覚えるパスが上限に達したら、いちばん
    少ないパスを忘れて、新しいパスを覚える（以前は覚えず、未知のパスの
    ままだった）。パスの異常は、そのリクエストを覚える前の状態で判定する
    （覚えた後では、未知のパスも見たことになり、検知しなかった）
- `mcp-rs wordpress-keyring set`の後に表示する設定の例で、名前を
  TOMLの文字列として書く（#724。`"`・`\`を含む名前でも、そのまま読める）
- HTTPで、`/ws`のOAuthのトークンの期限（`exp`）が小数なら、検証と同じく
  丸める（#724。以前は切り捨てて、最大1秒早く閉じた）
- HTTPで、`trusted_proxies`に、IPv4をIPv6で表した範囲
  （`::ffff:10.0.0.5/128`等）を書いても、そのIPv4のプロキシを信頼する
  （#724。以前は一致しなかった）
- HTTPで、`max_connections`・`websocket_max_connections`が大きすぎる値
  なら、起動で断る（#724。以前は起動の後にパニックし、何も受け付けな
  かった）
- HTTPで、認証の失敗の監査で抑えた件数の報告を、接続が切れても書き
  終える（#724。以前は、そのリクエストの処理が破棄されると、件数が
  失われた）
- リリースのファイルを作るワークフロー（`release-binaries.yml`）で、
  手動の実行（`workflow_dispatch`）でタグを選んだとき、公開済みの
  リリースのアーカイブ・チェックサムを置き換えない（#717）。公開する
  ジョブは、タグのpushのときだけ動く
- 手で入れる手順（ガイド）で、SHA256が合ったときだけ、展開・コピー・
  実行に進む（#717）。以前は、合わなくてもPowerShellは`False`と出す
  だけ、bashも止まらずに次へ進んだ。PowerShellでは、前のダウンロードを
  展開したフォルダが残っていても上書きし、展開・コピーが失敗したら止まる
  （以前は、古いファイルをコピーして実行し得た）
- MySQL・MariaDBの`execute_command`で、`COMMIT`を送った後に応答を確かめ
  られなかったとき（時間切れ・接続が切れた等）は、「止めた」ではなく、
  変更が保存されたか分からないので、やり直す前にデータを確かめるよう
  返す（#717）。以前は、サーバーがコミットした後に応答だけが遅れても
  「止めた」と返し、やり直すと行が重複し得た
- MySQL・MariaDBの`execute_query`・`execute_command`の`params`に、JSONの
  オブジェクト・配列も渡せる（#717）。MySQLのJSON列は`execute_query`が
  オブジェクト・配列で返すが、引数の定義が許さず、そのまま渡し直せな
  かった。説明の「JSONは文字列で返す」も直した（MariaDBは文字列で返す）
- MySQL・MariaDBの`get_schema`で、式を含むインデックス（`(tenant_id,
  LOWER(email))`の一意インデックス等）を、全体として除く（#717）。以前は
  式の部分だけを除き、`tenant_id`だけで一意のように返した
- MySQL・MariaDBのエンジン（ライブラリ）で、`get_table_schema`にビューを
  指定すると、列を返す（#717。以前は「table not found」）。また、
  `MySqlEngine::connect`・`validate_config`でも、`ssl_root_cert`と
  `ssl_mode`の組み合わせを確かめる（mcp-rs本体の経路では、以前から
  確かめていた）
- `mcp-rs init`・`mcp-rs uninstall`で、Copilot CLIの設定の場所を、
  環境変数`COPILOT_HOME`があればその中（`$COPILOT_HOME/mcp-config.json`）に
  する（#710）。Copilot CLIは`COPILOT_HOME`で設定の場所を変えられるが、
  以前は`~/.copilot/mcp-config.json`だけを見たので、前の登録・別の名前の
  mcp-rsを探す・消すとき、登録に失敗して前の登録を戻すときに、Copilot CLIが
  使っていないファイルを読み書きした（登録そのものは`copilot mcp add`で
  行うので、正しい場所に入っていた）
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
  （#710）。足せなかったサイトのために入れ直した項目も戻す。以前は、確かめる
  前に上書きしたままで、同じ項目を使うほかの設定ファイルも、通らない資格
  情報を読んだ。前の中身を読めなければ、上書きしない。戻す書き込みが失敗
  したときは戻らないので、`mcp-rs wordpress-keyring set <name>`で入れ直すよう
  知らせる。Ctrl+Cで止めたときも、前の中身に戻してから終わる（#715。終了
  コードは130）。パスワードの入力中のCtrl+C・Escも同じ。書き込み・戻す
  途中でCtrl+Cを受けたら、終わるのを待ってから戻す
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
- `wordpress_health_check`（MCPのツール）で、確かめていない項目は、
  理由を付けて「not checked」と出す（#710）。サインインが通らなければ
  権限とメディアのアップロード、サイトに届かなければREST API・サイン
  イン・権限・メディア。以前は❌と出し、確かめて失敗したように見えた。
  `mcp-rs check`は以前から区別していた
- Windowsのインストールスクリプト（`install.ps1`）で、新しい`mcp-rs.exe`の
  コピーが途中で失敗したら、書きかけのファイルがあっても前の版に戻す
  （#710）。以前は`mcp-rs.exe`が無いときだけ戻したので、書きかけのまま
  残り得た。前の版を戻すのは、この実行で脇に移したときだけ（古い
  `mcp-rs.exe.old`で置き換えない）
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

- Windowsで、WordPressの`upload_media`の`file_path`に`\\host\share\…`の
  ようなUNCのパスを渡すと、許可したフォルダかを確かめる前の正規化で、SMBの
  接続が始まり、Windowsの資格情報が送られ得た（#750。`upload_allowed_dirs`
  を設定したときだけ）。ドライブ名で始まるパスだけ受け付け、UNC・デバイス
  （`\\.\…`）等のパスは、正規化の前に断る

- WordPressの`upload_media`の`file_path`で、許可したフォルダの中かを
  確かめた後、同じパスでファイルを開き直していた（#750）。その間に、
  フォルダの中の項目をシンボリックリンク・ジャンクションに差し替えると、
  外のファイルを読んでWordPressに送れた（フォルダに書ける人が要る）。
  開いたファイルが、確かめたファイルかを比べる（Windowsは開いたファイルの
  実際のパス、Linux・macOSは正規化し直したパスとデバイス・inode）

- HTTPで、リプレイ攻撃対策（`anti_replay_enabled`）のnonceの記録が、1つの
  呼び出し元の上限に達すると、送られるたびに、書き込みのロックを持った
  まま全件（既定で最大10万件）を見て、期限の切れた記録を探していた
  （#724）。ほかの呼び出し元も待たされた。今は、いちばん早い期限を覚え、
  それより前は見ない
- HTTPで、リプレイ攻撃対策のnonceと侵入検知（IDS）の記録を、呼び出し元の
  名前だけでなく、利用者（OAuthのトークンの`sub`、APIキーの`user_id`）まで
  区別して分ける（#724）。以前は、OAuthで同じクライアント（公開クライアント
  等）を使う別の利用者が、記録と上限を共有し、ほかの人の検知に巻き込まれ
  得た。**同じ`name`のAPIキーが2つあると、起動しない**（名前で呼び出し元を
  区別するため。以前は通り、記録が混ざった）
- HTTPで、OAuthのトークンの期限が、本文を読んでいる間に切れたら、処理
  せずに401を返す（#724）。以前は、認証を本文を読む前に確かめるだけ
  だったので、ゆっくり送ると、期限の後に処理を始められた。`/ws`でも、
  期限とメッセージが同時に届いたら、期限を先に見て閉じる
- HTTPで、信頼するプロキシのヘッダーにある、IPv4をIPv6で表したアドレス
  （`::ffff:192.0.2.1`）をIPv4にそろえる（#724）。以前は、IDSのブロック
  リスト等で、同じアドレスが別のものとして数えられた
- HTTPで、プロキシが送るTLSの版（`X-Forwarded-TLS-Version`・
  `X-SSL-Protocol`）を、`TLSv1.2`（nginxの`$ssl_protocol`）・`TLS 1.2`の形でも
  読み、読めない値は断る（#724、PR #729のレビュー）。以前は`1.2`・`TLS1.2`の
  形しか読めず、読めない値を通していたので、`min_tls_version = "1.3"`でも、
  nginxの後ろでTLS 1.2の接続を受け付けていた
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
  `..Default::default()`を使う必要がある（`TokenInfo`は`Default`を
  持たないので、`expires_at`を足す。#724で書き足した）
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
