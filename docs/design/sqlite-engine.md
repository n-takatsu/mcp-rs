# SQLite Engine Design Document

## 概要

SQLiteは軽量なファイルベースのRDBMSで、サーバーレスアーキテクチャを採用しています。
本ドキュメントでは、mcp-rsプロジェクトにおけるSQLiteエンジンの設計について説明します。

## SQLiteの特徴

## 利点

- **軽量**: サーバープロセス不要、単一ファイルで動作
- **高速**: 読み取り処理が非常に高速
- **移植性**: クロスプラットフォーム対応
- **信頼性**: ACID準拠、堅牢なトランザクション
- **設定不要**: インストールや設定が簡単

## 制約

- **同時書き込み**: 単一ライターのみ（複数リーダーは可能）
- **ネットワーク**: 直接ネットワーク接続はサポートなし
- **サイズ制限**: 大規模データには不適切（推奨上限: 数TB）
- **ユーザー管理**: 組み込みユーザー認証なし

## アーキテクチャ設計

## 1. 接続管理

```rust
pub struct SqliteEngine {
    config: DatabaseConfig,
    security: Arc<DatabaseSecurity>,
    pool: Arc<SqlitePool>,  // sqlx::SqlitePool
}

pub struct SqliteConnection {
    inner: sqlx::SqliteConnection,
    config: DatabaseConfig,
    security: Arc<DatabaseSecurity>,
}
```

## 2. 設定構造

```rust
// SQLite固有の設定
pub struct SqliteConfig {
    pub file_path: String,           // データベースファイルパス
    pub mode: SqliteMode,            // 接続モード
    pub journal_mode: JournalMode,   // ジャーナルモード
    pub synchronous: SynchronousMode, // 同期モード
    pub cache_size: i32,             // キャッシュサイズ
    pub foreign_keys: bool,          // 外部キー制約
    pub auto_vacuum: AutoVacuum,     // 自動バキューム
}

pub enum SqliteMode {
    ReadWrite,     // 読み書き
    ReadOnly,      // 読み取り専用
    Memory,        // インメモリ
}

pub enum JournalMode {
    Delete,        // デフォルト
    Truncate,      // 切り詰め
    Persist,       // 永続化
    Memory,        // メモリ
    Wal,           // Write-Ahead Logging
    Off,           // ジャーナル無効
}
```

## 実装戦略

## Phase 1: 基本実装

1. **基本エンジン**: SqliteEngine構造体
2. **接続管理**: sqlx::SqlitePool使用
3. **基本CRUD**: SELECT, INSERT, UPDATE, DELETE
4. **設定検証**: ファイルパス、権限チェック

## Phase 2: 高度な機能

1. **トランザクション**: BEGIN, COMMIT, ROLLBACK
2. **プリペアドステートメント**: パラメータ化クエリ
3. **スキーマ情報**: PRAGMA table_info等
4. **パフォーマンス最適化**: WALモード、キャッシュ

## Phase 3: 統合・テスト

1. **MCP統合**: ツール実装
2. **セキュリティ**: SQLインジェクション対策
3. **テスト**: 単体・統合テスト
4. **ドキュメント**: API documentation

## 技術的考慮事項

## 依存関係

```toml
[dependencies]
sqlx = { version = "0.7", features = ["sqlite", "runtime-tokio-rustls", "chrono", "uuid"] }
```

## 接続文字列形式

```
sqlite:path/to/database.db
sqlite::memory:           

## インメモリ

sqlite:database.db?mode=ro 

## 読み取り専用

```

## パフォーマンス最適化

- **WALモード**: 並行読み取り可能
- **CONNECTION_LIMIT**: 適切なプールサイズ
- **PRAGMA最適化**: cache_size, temp_store等

## ファイル構造

```
src/handlers/database/engines/
├── sqlite.rs              

## メインエンジン実装

├── sqlite/
│   ├── connection.rs       

## 接続管理

│   ├── transaction.rs      

## トランザクション

│   ├── prepared.rs         

## プリペアドステートメント

│   ├── config.rs          

## SQLite固有設定

│   └── utils.rs           

## ユーティリティ

```

## 使用例

## 基本設定

```rust
let config = DatabaseConfig {
    database_type: DatabaseType::SQLite,
    connection: ConnectionConfig {
        host: "".to_string(),  // 未使用
        port: 0,               // 未使用
        database: "app.db".to_string(),  // ファイルパス
        username: "".to_string(),        // 未使用
        password: "".to_string(),        // 未使用
        ..Default::default()
    },
    ..Default::default()
};
```

## インメモリ使用

```rust
let config = DatabaseConfig {
    database_type: DatabaseType::SQLite,
    connection: ConnectionConfig {
        database: ":memory:".to_string(),
        ..Default::default()
    },
    ..Default::default()
};
```

## セキュリティ考慮事項

1. **ファイル権限**: 適切なファイルアクセス権設定
2. **SQLインジェクション**: プリペアドステートメント強制
3. **パス検証**: ディレクトリトラバーサル防止
4. **暗号化**: SQLCipher拡張（将来的に）

## テスト戦略

## 単体テスト

- エンジン初期化
- 基本CRUD操作
- トランザクション管理
- エラーハンドリング

## 統合テスト

- MCP プロトコル統合
- 複数接続での動作
- パフォーマンステスト

## ベンチマーク

- 読み取り性能
- 書き込み性能
- 同時接続数
- メモリ使用量

## まとめ

SQLiteエンジンは軽量で使いやすいデータベースソリューションとして、
特に開発環境、プロトタイピング、小規模アプリケーションに適しています。
適切な実装により、高性能で信頼性の高いデータベースアクセスを提供できます。