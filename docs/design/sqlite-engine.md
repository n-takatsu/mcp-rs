# SQLite Engine Design Document

## 概要

SQLiteは軽量なファイルベースのRDBMSで、サーバーレスアーキテクチャを採用しています。
本ドキュメントでは、mcp-rsプロジェクトにおけるSQLiteエンジンの設計と実装状況について説明します。

## 実装ステータス

**実装状況**: 🚧 実装中（PR #105 Open）  
**実装PR**: [#105 - SQLite Database Handler complete implementation](https://github.com/n-takatsu/mcp-rs/pull/105)  
**作成日**: 2025-12-05  
**実装規模**: 1,424行（新規）- 276行（削除）= +1,148行  
**テスト**: 13/13 合格（100%）

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

### 実装済みの構造

```rust
// src/handlers/database/engines/sqlite/mod.rs
pub struct SqliteEngine {
    config: DatabaseConfig,
    security: Arc<DatabaseSecurity>,
}

// src/handlers/database/engines/sqlite/connection.rs
pub struct SqliteConnection {
    pool: SqlitePool,  // sqlx::SqlitePool
    config: DatabaseConfig,
    security: Arc<DatabaseSecurity>,
}

// src/handlers/database/engines/sqlite/transaction.rs
pub struct SqliteTransaction {
    tx: Arc<Mutex<Option<sqlx::Transaction<'static, Sqlite>>>>,
}

// src/handlers/database/engines/sqlite/prepared.rs
pub struct SqlitePreparedStatement {
    pool: SqlitePool,
    sql: String,
    param_count: usize,
}
```

### 設定構造（実装済み）

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

## 実装状況

### ✅ Phase 1: 基本実装（完了）

- ✅ **SqliteEngine構造体**: DatabaseEngineトレイト完全実装
- ✅ **接続管理**: sqlx::SqlitePool使用
- ✅ **基本CRUD**: SELECT, INSERT, UPDATE, DELETE完全サポート
- ✅ **設定検証**: ファイルパス、権限チェック

**ファイル**: `engine.rs` (200行), `connection.rs` (450行)

### ✅ Phase 2: 高度な機能（完了）

- ✅ **トランザクション**: BEGIN, COMMIT, ROLLBACK実装
- ✅ **プリペアドステートメント**: パラメータ化クエリ完全対応
- ✅ **スキーマ情報**: PRAGMA table_info統合
- 🔄 **パフォーマンス最適化**: WALモード対応（設定可能）

**ファイル**: `transaction.rs` (423行), `prepared.rs` (286行)

### 🚧 Phase 3: 統合・テスト（完了）

- ✅ **MCP統合**: DatabaseEngineトレイト統合
- ✅ **セキュリティ**: SQLインジェクション対策
- ✅ **テスト**: 13/13 統合テスト合格
- ✅ **ドキュメント**: 実装ガイド作成済み

## 実装の詳細

## 技術的実装

### 使用ライブラリ（実装済み）

```toml
[dependencies]
sqlx = { version = "0.8", features = ["sqlite", "runtime-tokio-rustls"] }
```

**実装特徴**:

- sqlx 0.8: 最新の非同期SQLiteサポート
- 型安全: Rustの型システム活用
- 接続プーリング: 効率的なリソース管理

## 接続文字列形式

```bash
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

## ファイル構造（実装済み）

```text
src/handlers/database/engines/sqlite/
├── mod.rs          # モジュールエクスポート (14行)
├── engine.rs       # SqliteEngine実装 (200行)
├── connection.rs   # SqliteConnection実装 (450行)
├── transaction.rs  # SqliteTransaction実装 (423行)
└── prepared.rs     # SqlitePreparedStatement実装 (286行)

合計: 1,424行（新規）- 276行（削除）= +1,148行
```

## テスト状況

### 実装済みテスト

```text
tests/
└── sqlite_integration_tests.rs  # 統合テスト (13/13合格)

テスト合格率: 100%
Clippy警告: 0
フォーマット: 適用済み
```

## 既知の制約

### 1. トランザクション分離の制約

- **現状**: クエリがプールを使用（完全な分離は非対応）
- **理由**: DatabaseTransactionトレイトが`&self`を要求
- **影響**: トランザクション内のクエリが新しい接続を使用する可能性
- **対策**: 将来的にアーキテクチャ改善を検討

### 2. PRAGMA table_infoの型情報

- **対応済み**: NULL型名に対するfallback処理実装
- **動作**: 型情報が欠落している場合でも安全に処理

## 実装完了機能のまとめ

### ✅ 完全実装済み

1. **接続管理**: sqlx::SqlitePool使用
2. **基本CRUD**: 全SQL操作対応
3. **トランザクション**: BEGIN/COMMIT/ROLLBACK、セーブポイント対応
4. **プリペアドステートメント**: SQLインジェクション防止
5. **スキーマ情報**: PRAGMA table_info統合
6. **エラーハンドリング**: 包括的エラー処理
7. **データ型サポート**: INTEGER, REAL, TEXT, BLOB, NULL

### 🔄 将来改善予定

1. **トランザクション分離**: アーキテクチャ改善
2. **WALモード最適化**: 並行アクセス性能向上
3. **キャッシュ戦略**: メモリ使用最適化

## 使用例（実装済み）

### 基本的な接続と使用

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

## セキュリティ実装状況

### ✅ 実装済み

1. **ファイル権限**: 適切なファイルアクセス権設定サポート
2. **SQLインジェクション**: プリペアドステートメント強制
3. **パス検証**: ディレクトリトラバーサル防止
4. **型安全性**: Rustの型システムによる保護

### 🔄 将来実装予定

1. **暗号化**: SQLCipher拡張統合検討

## テスト戦略

### ✅ 実装済みテスト

- ✅ エンジン初期化テスト
- ✅ 基本CRUD操作テスト
- ✅ トランザクション管理テスト
- ✅ エラーハンドリングテスト
- ✅ プリペアドステートメントテスト
- ✅ スキーマ情報取得テスト

### パフォーマンステスト

- ✅ 読み取り性能: 高速（インメモリ最適）
- ✅ 書き込み性能: 良好
- ✅ 同時接続数: 複数リーダー対応
- ✅ メモリ使用量: 軽量

## 関連ドキュメント

- **実装ステータス**: `docs/guides/database-implementation-status.md`
- **統合テスト**: `tests/sqlite_integration_tests.rs`
- **PR**: [#105](https://github.com/n-takatsu/mcp-rs/pull/105)

## まとめ

SQLiteエンジンは**ほぼ完全実装済み**（PR #105でレビュー中）で、
軽量で高速なファイルベースデータベースアクセスを提供します。
開発環境、プロトタイピング、小規模アプリケーションに最適です。

**実装完了日**: 2025年12月5日  
**ステータス**: レビュー待ち  
**実装者**: @n-takatsu
