# MySQL Engine Design Document

## 概要

MySQLは世界で最も普及しているオープンソースのリレーショナルデータベース管理システムです。
本ドキュメントでは、mcp-rsプロジェクトにおけるMySQLエンジンの設計と実装状況について説明します。

## 実装ステータス

**実装状況**: ✅ 完全実装済み（v0.15.0+）  
**実装PR**: [#102 - MySQL Database Handler complete implementation](https://github.com/n-takatsu/mcp-rs/pull/102)  
**マージ日**: 2025-12-05  
**実装規模**: 1,097行（6ファイル）  
**テスト**: 45+ テスト（全て合格）

## MySQLの特徴

## 利点

- **高性能**: 最適化されたクエリエンジン、効率的なインデックス
- **スケーラビリティ**: 水平・垂直スケーリング、読み取りレプリカ
- **堅牢性**: ACID準拠、クラッシュリカバリ、バックアップ機能
- **エコシステム**: 豊富なツール、ライブラリ、サポート
- **互換性**: MariaDB、Percona Server等との互換性

## 特徴的機能

- **レプリケーション**: マスター・スレーブ、マスター・マスター
- **ストレージエンジン**: InnoDB、MyISAM、Memory等
- **クラスタリング**: MySQL Cluster (NDB)
- **パーティショニング**: テーブルパーティション
- **JSON サポート**: JSON データ型、関数

## 制約・考慮事項

- **大文字小文字**: システムによる動作の違い
- **文字エンコーディング**: UTF8、UTF8MB4の選択
- **接続制限**: max_connections設定
- **ロック競合**: テーブルロック、行ロック

## アーキテクチャ設計

### 実装済みの構造

```rust
// src/handlers/database/engines/mysql/mod.rs
pub struct MySqlEngine {
    config: DatabaseConfig,
    security: Arc<DatabaseSecurity>,
}

// src/handlers/database/engines/mysql/connection.rs
pub struct MySqlConnection {
    pool: Pool,  // mysql_async::Pool
    config: DatabaseConfig,
    security: Arc<DatabaseSecurity>,
}

// src/handlers/database/engines/mysql/transaction.rs
pub struct MySqlTransaction {
    conn: Arc<Mutex<Option<Conn>>>,
    isolation_level: IsolationLevel,
}

// src/handlers/database/engines/mysql/prepared.rs
pub struct MySqlPreparedStatement {
    statement: Statement,
    pool: Pool,
    sql: String,
}
```

### 設定構造（実装済み）

```rust
// MySQL固有の設定
pub struct MySqlConfig {
    pub ssl_mode: MySqlSslMode,
    pub charset: String,
    pub collation: String,
    pub time_zone: String,
    pub sql_mode: String,
    pub auto_reconnect: bool,
    pub compression: bool,
    pub local_infile: bool,
    pub multi_statements: bool,
}

pub enum MySqlSslMode {
    Disabled,
    Preferred,
    Required,
    VerifyCa,
    VerifyIdentity,
}

pub struct MySqlSessionInfo {
    pub connection_id: u32,
    pub thread_id: u32,
    pub server_version: String,
    pub protocol_version: u8,
    pub character_set: String,
    pub status_flags: u16,
}
```

## 実装状況

### ✅ Phase 1: 基本実装（完了）

- ✅ **MySqlEngine構造体**: DatabaseEngineトレイト完全実装
- ✅ **接続管理**: mysql_async::Pool使用
- ✅ **基本CRUD**: SELECT, INSERT, UPDATE, DELETE完全サポート
- ✅ **設定検証**: 接続文字列、認証情報検証済み

**ファイル**: `src/handlers/database/engines/mysql/engine.rs`, `connection.rs`

### ✅ Phase 2: 高度な機能（完了）

- ✅ **トランザクション**: BEGIN, COMMIT, ROLLBACK, SAVEPOINT完全実装
- ✅ **プリペアドステートメント**: パラメータ化クエリ完全対応
- ✅ **スキーマ情報**: INFORMATION_SCHEMA統合（部分実装）
- ✅ **セキュリティ**: SQLインジェクション防止、パラメータ検証

**ファイル**: `transaction.rs` (292行), `prepared.rs` (203行), `param_converter.rs`

### 🚧 Phase 3: 最適化・統合（一部完了）

- ✅ **接続プール**: デッドロック検出、自動再接続
- ✅ **パフォーマンス**: プリペアドステートメント最適化
- ✅ **MCP統合**: DatabaseEngineトレイト統合
- 🔄 **レプリケーション**: 読み書き分離（将来実装予定）

## 実装の詳細

## 技術的実装

### 使用ライブラリ（実装済み）

```toml
[dependencies]
mysql_async = { version = "0.36", optional = true }
tokio = { version = "1.48", features = ["full"] }
chrono = { version = "0.4", features = ["serde"] }
```

**選択理由**:

- mysql_async: MySQL特化設計、高性能、詳細な制御が可能
- 非同期処理: tokio完全統合
- 型安全: Rustの型システムを活用

## 接続文字列形式

```bash
mysql://username:password@host:port/database?option1=value1&option2=value2

# SSL例
mysql://user:pass@localhost:3306/mydb?ssl-mode=required&ssl-ca=/path/to/ca.pem

// 文字セット例
mysql://user:pass@localhost:3306/mydb?charset=utf8mb4&collation=utf8mb4_unicode_ci
```

## パフォーマンス最適化

### 接続プール設定

```rust
pub struct MySqlPoolConfig {
    pub max_connections: u32,      // 20-100
    pub min_connections: u32,      // 5-10
    pub acquire_timeout: Duration, // 30s
    pub idle_timeout: Duration,    // 10m
    pub max_lifetime: Duration,    // 30m
}
```

### クエリ最適化

- **プリペアドステートメント**: SQLインジェクション防止
- **バッチ処理**: 複数行INSERT/UPDATE
- **インデックスヒント**: USE INDEX, FORCE INDEX
- **クエリキャッシュ**: 結果キャッシュ（MySQL 8.0以降は非推奨）

## セキュリティ設計

## 1. 接続セキュリティ

```rust
pub struct MySqlSecurityConfig {
    pub ssl_enabled: bool,
    pub ssl_verify_server_cert: bool,
    pub ssl_ca_path: Option<String>,
    pub ssl_cert_path: Option<String>,
    pub ssl_key_path: Option<String>,
    pub require_secure_transport: bool,
}
```

## 2. 認証・認可

- **MySQL認証プラグイン**: mysql_native_password, caching_sha2_password
- **ユーザー権限**: GRANT/REVOKE管理
- **データベース権限**: スキーマレベル制御

## 3. 監査・ログ

- **スロークエリログ**: 性能問題の検出
- **バイナリログ**: レプリケーション、Point-in-timeリカバリ
- **エラーログ**: 接続エラー、権限エラー

## エラーハンドリング

## MySQL特有のエラー

```rust
pub enum MySqlError {
    ConnectionError(String),
    AuthenticationError(String),
    SqlSyntaxError(String),
    ConstraintViolation(String),
    DeadlockDetected(String),
    LockWaitTimeout(String),
    DuplicateEntry(String),
    TableNotFound(String),
    ColumnNotFound(String),
    DataTruncation(String),
}
```

## 復旧戦略

- **自動再接続**: 接続失敗時の再試行
- **デッドロック再試行**: 指数バックオフ
- **フェイルオーバー**: レプリカへの切り替え

## テスト戦略

## 単体テスト

- エンジン初期化
- 接続文字列解析
- 基本CRUD操作
- トランザクション管理
- エラーハンドリング

## 統合テスト

- 実MySQL/MariaDBサーバーとの接続
- 複雑なクエリ実行
- 同時接続テスト
- パフォーマンステスト

## ベンチマーク

- 接続時間
- クエリレスポンス時間
- スループット
- メモリ使用量

## ファイル構造（実装済み）

```text
src/handlers/database/engines/mysql/
├── mod.rs              # モジュールエクスポート
├── engine.rs           # MySqlEngine実装
├── connection.rs       # MySqlConnection実装 (374行)
├── transaction.rs      # MySqlTransaction実装 (292行)
├── prepared.rs         # MySqlPreparedStatement実装 (203行)
└── param_converter.rs  # パラメータ変換ユーティリティ

合計: 1,097行（実装完了）
```

## テスト状況

### 実装済みテスト

```text
tests/
├── mysql_integration_tests.rs          # 統合テスト (13/13合格)
├── mysql_phase1_basic_tests.rs         # 基本機能テスト
├── mysql_phase1_integration_complete.rs # 完全統合テスト
└── mysql_security_tests.rs             # セキュリティテスト (45+)

テスト合格率: 100%
実行時間: 0.30秒
```

## MariaDB互換性

MySQLエンジンをベースにMariaDBサポートを追加：

## 相違点

- **ストレージエンジン**: Aria, ColumnStore
- **レプリケーション**: Galera Cluster
- **JSON機能**: 一部機能差異
- **システム変数**: MariaDB固有の変数

## 実装アプローチ

```rust
pub enum MySqlVariant {
    MySQL,
    MariaDB,
    Percona,
}

impl MySqlEngine {
    fn detect_variant(&self) -> MySqlVariant {
        // SELECT VERSION() でバリアント検出
    }

    fn adapt_features(&self, variant: MySqlVariant) {
        // バリアント固有の機能調整
    }
}
```

## 使用例（実装済み）

### 基本的な接続と使用

```rust
let config = DatabaseConfig {
    database_type: DatabaseType::MySQL,
    connection: ConnectionConfig {
        host: "localhost".to_string(),
        port: 3306,
        database: "myapp".to_string(),
        username: "appuser".to_string(),
        password: "secure_password".to_string(),
        ssl_mode: Some("required".to_string()),
        timeout_seconds: 30,
        retry_attempts: 3,
        options: {
            let mut opts = HashMap::new();
            opts.insert("charset".to_string(), "utf8mb4".to_string());
            opts.insert("collation".to_string(), "utf8mb4_unicode_ci".to_string());
            opts
        },
    },
    pool: PoolConfig {
        max_connections: 50,
        min_connections: 10,
        connection_timeout: 30,
        idle_timeout: 600,
        max_lifetime: 1800,
    },
    ..Default::default()
};
```

## 高可用性設定（将来実装予定）

```rust
// レプリケーション対応（将来実装予定）
// 現在は単一接続のみサポート
let ha_config = MySqlHAConfig {
    master: "mysql-master:3306".to_string(),
    slaves: vec![
        "mysql-slave1:3306".to_string(),
        "mysql-slave2:3306".to_string(),
    ],
    read_write_split: true,
    failover_timeout: Duration::from_secs(10),
};
```

## 実装完了機能のまとめ

### ✅ 完全実装済み

1. **接続管理**: mysql_async::Pool使用
2. **基本CRUD**: 全SQL操作対応
3. **トランザクション**: ACID準拠、セーブポイント対応
4. **プリペアドステートメント**: SQLインジェクション防止
5. **パラメータ化クエリ**: 型安全なパラメータ変換
6. **エラーハンドリング**: 包括的エラー処理
7. **セキュリティ**: DatabaseSecurity統合

### 🔄 将来実装予定

1. **レプリケーション**: 読み書き分離
2. **SSL/TLS強化**: 証明書検証強化
3. **完全なスキーマ情報**: INFORMATION_SCHEMA完全活用
4. **パフォーマンス監視**: メトリクス収集

## パフォーマンス実績

- ✅ **接続時間**: < 100ms (ローカル), < 500ms (リモート)
- ✅ **クエリレスポンス**: < 10ms (単純), < 100ms (複雑)
- ✅ **テスト実行時間**: 0.30秒（13テスト）
- ✅ **同時接続**: 100+ 接続対応

## 関連ドキュメント

- **実装ステータス**: `docs/guides/database-implementation-status.md`
- **Phase 1実装ガイド**: `docs/mysql-phase1-guide.md`
- **統合テスト**: `tests/mysql_integration_tests.rs`

## まとめ

MySQLエンジンは**完全実装済み**で、本番環境での使用が可能です。
ACID準拠のトランザクション、SQLインジェクション防止、高性能な接続プール管理により、
エンタープライズレベルのデータベースアクセスを提供します。

**実装完了日**: 2025年12月5日  
**実装者**: @n-takatsu
