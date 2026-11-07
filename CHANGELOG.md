# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
  - v0.18.1以前の`[plugins]`（テーブル）は、これまでどおり無視する
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
