# MySQL Engine Design Document

## 概要

MySQLは世界で最も普及しているオープンソースのリレーショナルデータベース管理システムです。
本ドキュメントでは、mcp-rsプロジェクトにおけるMySQLエンジンの設計について説明します。

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

## 1. エンジン構造

```rust
pub struct MySqlEngine {
    config: DatabaseConfig,
    mysql_config: MySqlConfig,
    security: Arc<DatabaseSecurity>,
    pool: Arc<MySqlPool>,  // sqlx::MySqlPool または mysql_async::Pool
}

pub struct MySqlConnection {
    inner: sqlx::MySqlConnection, // または mysql_async::Conn
    config: DatabaseConfig,
    security: Arc<DatabaseSecurity>,
    session_info: MySqlSessionInfo,
}
```

## 2. 設定構造

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

## 実装戦略

## Phase 1: 基本実装

1. **基本エンジン**: MySqlEngine構造体
2. **接続管理**: sqlx::MySqlPool使用
3. **基本CRUD**: SELECT, INSERT, UPDATE, DELETE
4. **設定検証**: 接続文字列、認証情報

## Phase 2: 高度な機能

1. **トランザクション**: BEGIN, COMMIT, ROLLBACK, SAVEPOINT
2. **プリペアドステートメント**: パラメータ化クエリ
3. **スキーマ情報**: INFORMATION_SCHEMA活用
4. **SSL/TLS**: セキュア接続

## Phase 3: 最適化・統合

1. **接続プール**: デッドロック検出、再接続
2. **パフォーマンス**: クエリキャッシュ、バッチ処理
3. **MCP統合**: 6つのツール実装
4. **レプリケーション**: 読み書き分離（将来実装）

## 技術的考慮事項

## 依存関係選択

### Option A: sqlx (推奨)

```toml
[dependencies]
sqlx = { version = "0.7", features = ["mysql", "runtime-tokio-rustls", "chrono", "uuid", "tls-rustls"] }
```

**利点**:

- 型安全なクエリ
- コンパイル時検証
- 統一されたAPI
- 活発な開発

### Option B: mysql_async

```toml
[dependencies]
mysql_async = "0.34"
tokio = { version = "1.0", features = ["full"] }
```

**利点**:

- MySQL特化設計
- 高性能
- 詳細な制御
- 軽量

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

## ファイル構造

```text
src/handlers/database/engines/
├── mysql.rs

## メインエンジン実装

├── mysql/
│   ├── connection.rs

## 接続管理

│   ├── transaction.rs

## トランザクション

│   ├── prepared.rs

## プリペアドステートメント

│   ├── config.rs

## MySQL固有設定

│   ├── error.rs

## エラー定義

│   ├── pool.rs

## 接続プール

│   └── utils.rs

## ユーティリティ

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

## 使用例

## 基本設定

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

## 高可用性設定

```rust
// レプリケーション対応（将来実装）
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

## パフォーマンス目標

- **接続時間**: < 100ms (ローカル), < 500ms (リモート)
- **クエリレスポンス**: < 10ms (単純), < 100ms (複雑)
- **スループット**: > 1000 QPS (読み取り), > 500 QPS (書き込み)
- **同時接続**: 100+ 接続対応

## まとめ

MySQLエンジンは高性能で信頼性の高いデータベースソリューションとして、
Webアプリケーション、データ分析、エンタープライズシステムに幅広く適用できます。
適切な実装により、スケーラブルで安全なデータベースアクセスを提供できます。
