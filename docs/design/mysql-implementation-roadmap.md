# MySQL機能実装計画書

## 📋 プロジェクト概要

**目標**: mcp-rsのMySQLサポートを本番環境対応レベルまで完成させる
**現在の完成度**: 約60% (基本機能のみ実装済み)
**推定工数**: 合計 40-50 工数日
**完成予定**: 2025年12月末

---

## 🎯 実装計画サマリー

| Phase | 機能 | 工数 | 期間 | 優先度 |
|-------|------|------|------|--------|
| Phase 1 | パラメータ化クエリ & プリペアドステートメント | 8-10日 | Week 1-2 | 🔴 Critical |
| Phase 2 | トランザクション管理 | 6-8日 | Week 3 | 🔴 Critical |
| Phase 3 | スキーマ情報取得 | 5-7日 | Week 4 | 🟡 High |
| Phase 4 | セッション管理 | 4-6日 | Week 5 | 🟡 High |
| Phase 5 | 高度な機能 & 最適化 | 8-10日 | Week 6-7 | 🟢 Medium |
| Phase 6 | 包括的テスト & ドキュメント | 6-8日 | Week 8 | 🔴 Critical |

---

## 📈 Phase 1: パラメータ化クエリ & プリペアドステートメント

**期間**: Week 1-2 (8-10日)
**優先度**: 🔴 Critical

## 🎯 目標

SQL injection攻撃を防止し、パフォーマンスを向上させる基盤機能を実装

## 📝 実装内容

### 1.1 プリペアドステートメント構造体 (2日)

```rust
// src/handlers/database/engines/mysql/prepared.rs
pub struct MySqlPreparedStatement {
    statement: mysql_async::Statement,
    pool: Pool,
    sql: String,
    param_count: usize, // Parameter count
}

impl PreparedStatement for MySqlPreparedStatement {
    async fn execute(&self, params: &[Value]) -> Result<ExecuteResult, DatabaseError>;
    async fn query(&self, params: &[Value]) -> Result<QueryResult, DatabaseError>;
    fn parameter_count(&self) -> usize; // Returns parameter count
    fn get_sql(&self) -> &str;
}
```

### 1.2 パラメータ型変換システム (2日)

```rust
// src/handlers/database/engines/mysql/param_converter.rs
pub struct MySqlParamConverter;

impl MySqlParamConverter {
    pub fn convert_value(value: &Value) -> Result<mysql_async::Value, DatabaseError> {
        match value {
            Value::Null => Ok(mysql_async::Value::NULL),
            Value::Bool(b) => Ok(mysql_async::Value::Int(*b as i64)),
            Value::Int(i) => Ok(mysql_async::Value::Int(*i)),
            Value::Float(f) => Ok(mysql_async::Value::Double(*f)),
            Value::String(s) => Ok(mysql_async::Value::Bytes(s.as_bytes().to_vec())),
            Value::Binary(b) => Ok(mysql_async::Value::Bytes(b.clone())),
            Value::Json(j) => Ok(mysql_async::Value::Bytes(j.to_string().into_bytes())),
            _ => Err(DatabaseError::UnsupportedDataType(format!("Unsupported value type")))
        }
    }
}
```

### 1.3 DatabaseConnection実装更新 (2日)

```rust
// src/handlers/database/engines/mysql.rs
async fn prepare(&self, sql: &str) -> Result<Box<dyn PreparedStatement>, DatabaseError> {
    let mut conn = self.pool.get_conn().await?;
    let statement = conn.prep(sql).await
        .map_err(|e| DatabaseError::QueryFailed(format!("Failed to prepare: {}", e)))?;

    Ok(Box::new(MySqlPreparedStatement {
        statement,
        pool: self.pool.clone(),
        sql: sql.to_string(),
        param_count: sql.matches('?').count(),
    }))
}

async fn query(&self, sql: &str, params: &[Value]) -> Result<QueryResult, DatabaseError> {
    if params.is_empty() {
        // 既存の実装を使用
        return self.query_simple(sql).await;
    }

    let prepared = self.prepare(sql).await?;
    prepared.query(params).await
}
```

### 1.4 エラーハンドリング強化 (1日)

- パラメータ数不一致エラー
- 型変換エラー
- プリペア失敗エラー

### 1.5 基本テスト実装 (1日)

```rust
#[cfg(test)]
mod tests {
    #[tokio::test]
    async fn test_prepared_statement_select() {
        // SELECT * FROM users WHERE id = ? AND name = ?
    }

    #[tokio::test]
    async fn test_prepared_statement_insert() {
        // INSERT INTO users (name, email) VALUES (?, ?)
    }

    #[tokio::test]
    async fn test_parameter_conversion() {
        // 各種データ型の変換テスト
    }
}
```

## 📊 Phase 1 成果物

- ✅ SQL injection完全防止
- ✅ パフォーマンス向上 (プラン再利用)
- ✅ 型安全なパラメータ処理
- ✅ 既存APIとの完全互換性

---

## 🔄 Phase 2: トランザクション管理

**期間**: Week 3 (6-8日)
**優先度**: 🔴 Critical

## 🎯 Phase 2 目標

ACID特性を保証する完全なトランザクション管理システム

## 📝 Phase 2 実装内容

### 2.1 トランザクション構造体 (2日)

```rust
// src/handlers/database/engines/mysql/transaction.rs
pub struct MySqlTransaction {
    conn: mysql_async::Conn,
    isolation_level: IsolationLevel,
    is_active: bool,
    savepoints: Vec<String>, // Transaction savepoints
}

#[derive(Debug, Clone)]
pub enum IsolationLevel {
    ReadUncommitted,
    ReadCommitted,
    RepeatableRead,
    Serializable,
}

impl DatabaseTransaction for MySqlTransaction {
    async fn commit(self: Box<Self>) -> Result<(), DatabaseError>;
    async fn rollback(self: Box<Self>) -> Result<(), DatabaseError>;
    async fn savepoint(&mut self, name: &str) -> Result<(), DatabaseError>; // Create savepoint
    async fn rollback_to_savepoint(&mut self, name: &str) -> Result<(), DatabaseError>;
    async fn release_savepoint(&mut self, name: &str) -> Result<(), DatabaseError>;
    fn isolation_level(&self) -> IsolationLevel;
}
```

### 2.2 トランザクション開始機能 (2日)

```rust
// MySqlConnection実装更新
async fn begin_transaction(&self) -> Result<Box<dyn DatabaseTransaction>, DatabaseError> {
    let conn = self.pool.get_conn().await?;

    // START TRANSACTIONを実行
    conn.query_drop("START TRANSACTION").await?;

    Ok(Box::new(MySqlTransaction {
        conn,
        isolation_level: IsolationLevel::RepeatableRead, // MySQL default
        is_active: true,
        savepoints: Vec::new(),
    }))
}

async fn begin_transaction_with_isolation(
    &self,
    isolation: IsolationLevel
) -> Result<Box<dyn DatabaseTransaction>, DatabaseError> {
    let conn = self.pool.get_conn().await?;

    // 分離レベル設定
    let isolation_sql = match isolation {
        IsolationLevel::ReadUncommitted => "SET TRANSACTION ISOLATION LEVEL READ UNCOMMITTED",
        IsolationLevel::ReadCommitted => "SET TRANSACTION ISOLATION LEVEL READ COMMITTED",
        IsolationLevel::RepeatableRead => "SET TRANSACTION ISOLATION LEVEL REPEATABLE READ",
        IsolationLevel::Serializable => "SET TRANSACTION ISOLATION LEVEL SERIALIZABLE",
    };

    conn.query_drop(isolation_sql).await?;
    conn.query_drop("START TRANSACTION").await?;

    Ok(Box::new(MySqlTransaction {
        conn,
        isolation_level: isolation,
        is_active: true,
        savepoints: Vec::new(),
    }))
}
```

### 2.3 セーブポイント機能 (1日)

```rust
impl MySqlTransaction {
    async fn savepoint(&mut self, name: &str) -> Result<(), DatabaseError> {
        if !self.is_active {
            return Err(DatabaseError::TransactionNotActive);
        }

        let sql = format!("SAVEPOINT {}", self.escape_identifier(name));
        self.conn.query_drop(&sql).await?;
        self.savepoints.push(name.to_string());
        Ok(())
    }

    fn escape_identifier(&self, name: &str) -> String {
        format!("`{}`", name.replace("`", "``"))
    }
}
```

### 2.4 トランザクション内クエリ実行 (1日)

```rust
impl MySqlTransaction {
    pub async fn query(&mut self, sql: &str, params: &[Value]) -> Result<QueryResult, DatabaseError> {
        if !self.is_active {
            return Err(DatabaseError::TransactionNotActive);
        }

        // パラメータ化クエリ実行
        if params.is_empty() {
            let rows: Vec<mysql_async::Row> = self.conn.query(sql).await?;
            // 結果変換処理
        } else {
            let stmt = self.conn.prep(sql).await?;
            let mysql_params: Vec<mysql_async::Value> = params.iter()
                .map(|p| MySqlParamConverter::convert_value(p))
                .collect::<Result<Vec<_>, _>>()?;
            let rows: Vec<mysql_async::Row> = self.conn.exec(&stmt, mysql_params).await?;
            // 結果変換処理
        }
    }
}
```

## 📊 Phase 2 成果物

- ✅ ACID特性保証
- ✅ ネストしたセーブポイント対応
- ✅ 分離レベル制御
- ✅ デッドロック検出・リトライ

---

## 🔍 Phase 3: スキーマ情報取得

**期間**: Week 4 (5-7日)
**優先度**: 🟡 High

## 🎯 Phase 3 目標

INFORMATION_SCHEMAを活用した完全なメタデータ取得システム

## 📝 Phase 3 実装内容

### 3.1 スキーマ情報構造体 (1日)

```rust
// src/handlers/database/engines/mysql/schema.rs
#[derive(Debug, Clone)]
pub struct MySqlSchemaInfo {
    pub databases: Vec<DatabaseInfo>,
    pub current_database: String,
    pub server_version: String,
    pub character_sets: Vec<CharacterSetInfo>,
}

#[derive(Debug, Clone)]
pub struct MySqlTableInfo {
    pub name: String,
    pub schema: String,
    pub engine: String,          // InnoDB (Inno database), MyISAM, etc.
    pub row_format: String,      // Dynamic, Fixed, etc.
    pub table_collation: String,
    pub auto_increment: Option<u64>,
    pub table_comment: String,
    pub create_time: Option<chrono::DateTime<chrono::Utc>>,
    pub update_time: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Clone)]
pub struct MySqlColumnInfo {
    pub name: String,
    pub data_type: String,
    pub column_type: String,     // FULL type like "varchar(255)"
    pub is_nullable: bool,
    pub column_default: Option<String>,
    pub is_auto_increment: bool,
    pub column_key: String,      // PRI, UNI, MUL
    pub extra: String,
    pub column_comment: String,
    pub character_set: Option<String>,
    pub collation: Option<String>,
}
```

### 3.2 データベース一覧取得 (1日)

```rust
impl MySqlConnection {
    async fn get_schema(&self) -> Result<DatabaseSchema, DatabaseError> {
        let sql = r#"
            SELECT
                SCHEMA_NAME as database_name,
                DEFAULT_CHARACTER_SET_NAME as charset,
                DEFAULT_COLLATION_NAME as collation
            FROM INFORMATION_SCHEMA.SCHEMATA
            WHERE SCHEMA_NAME NOT IN ('information_schema', 'performance_schema', 'mysql', 'sys')
            ORDER BY SCHEMA_NAME
        "#;

        let rows = self.query(sql, &[]).await?;
        // 結果をDatabaseSchemaに変換
    }
}
```

### 3.3 テーブル情報取得 (1日)

```rust
async fn get_table_schema(&self, table_name: &str) -> Result<TableInfo, DatabaseError> {
    let sql = r#"
        SELECT
            TABLE_NAME,
            TABLE_SCHEMA,
            ENGINE,
            ROW_FORMAT,
            TABLE_COLLATION,
            AUTO_INCREMENT,
            TABLE_COMMENT,
            CREATE_TIME,
            UPDATE_TIME
        FROM INFORMATION_SCHEMA.TABLES
        WHERE TABLE_SCHEMA = DATABASE() AND TABLE_NAME = ?
    "#;

    let rows = self.query(sql, &[Value::String(table_name.to_string())]).await?;

    if let Some(row) = rows.rows.first() {
        // MySqlTableInfoに変換
    }
}
```

### 3.4 カラム情報取得 (1日)

```rust
async fn get_columns(&self, table_name: &str) -> Result<Vec<ColumnInfo>, DatabaseError> {
    let sql = r#"
        SELECT
            COLUMN_NAME,
            DATA_TYPE,
            COLUMN_TYPE,
            IS_NULLABLE,
            COLUMN_DEFAULT,
            EXTRA,
            COLUMN_KEY,
            COLUMN_COMMENT,
            CHARACTER_SET_NAME,
            COLLATION_NAME,
            ORDINAL_POSITION
        FROM INFORMATION_SCHEMA.COLUMNS
        WHERE TABLE_SCHEMA = DATABASE() AND TABLE_NAME = ?
        ORDER BY ORDINAL_POSITION
    "#;

    let rows = self.query(sql, &[Value::String(table_name.to_string())]).await?;
    // MySqlColumnInfoのVecに変換
}
```

### 3.5 インデックス情報取得 (1日)

```rust
async fn get_indexes(&self, table_name: &str) -> Result<Vec<IndexInfo>, DatabaseError> {
    let sql = r#"
        SELECT
            INDEX_NAME,
            COLUMN_NAME,
            NON_UNIQUE,
            SEQ_IN_INDEX,
            INDEX_TYPE,
            INDEX_COMMENT
        FROM INFORMATION_SCHEMA.STATISTICS
        WHERE TABLE_SCHEMA = DATABASE() AND TABLE_NAME = ?
        ORDER BY INDEX_NAME, SEQ_IN_INDEX
    "#;

    // インデックス情報を構造化
}
```

## 📊 Phase 3 成果物

- ✅ 完全なメタデータアクセス
- ✅ MySQL固有情報取得 (Engine, Charset, etc.)
- ✅ 動的スキーマ変更検出
- ✅ パフォーマンス最適化

---

## 👤 Phase 4: セッション管理

**期間**: Week 5 (4-6日)
**優先度**: 🟡 High

## 🎯 Phase 4 目標

session/database.rs内のTODO項目を完全実装

## 📝 Phase 4 実装内容

### 4.1 MySQLセッション構造体 (1日)

```rust
// src/session/database/mysql.rs
pub struct MySqlSession {
    pub connection_id: u32,
    pub thread_id: u32,
    pub user: String,
    pub host: String,
    pub database: String,
    pub command: String,
    pub time: u32,
    pub state: String,
    pub info: Option<String>,
}

impl MySqlSession {
    pub async fn get_current_session(conn: &MySqlConnection) -> Result<MySqlSession, DatabaseError> {
        let sql = "SELECT CONNECTION_ID(), @@pseudo_thread_id, USER(), @@hostname, DATABASE()";
        // セッション情報取得実装
    }
}
```

### 4.2 プロセスリスト取得 (1日)

```rust
pub async fn get_process_list(conn: &MySqlConnection) -> Result<Vec<MySqlSession>, DatabaseError> {
    let sql = r#"
        SELECT
            ID, USER, HOST, DB, COMMAND, TIME, STATE, INFO
        FROM INFORMATION_SCHEMA.PROCESSLIST
        WHERE USER != 'system user'
        ORDER BY ID
    "#;

    let rows = conn.query(sql, &[]).await?;
    // MySqlSessionのVecに変換
}
```

### 4.3 変数管理 (1日)

```rust
pub async fn get_session_variables(conn: &MySqlConnection) -> Result<HashMap<String, String>, DatabaseError> {
    let sql = "SHOW SESSION VARIABLES";
    // セッション変数を取得・パース
}

pub async fn set_session_variable(
    conn: &MySqlConnection,
    name: &str,
    value: &str
) -> Result<(), DatabaseError> {
    let sql = format!("SET SESSION {} = ?", name);
    conn.execute(&sql, &[Value::String(value.to_string())]).await?;
    Ok(())
}
```

### 4.4 接続プール統合 (1日)

```rust
impl DatabaseSession for MySqlConnection {
    async fn get_connection_info(&self) -> Result<ConnectionInfo, DatabaseError> {
        let session = MySqlSession::get_current_session(self).await?;

        Ok(ConnectionInfo {
            connection_id: format!("mysql_{}", session.connection_id),
            database_name: session.database,
            user_name: session.user,
            server_version: self.get_server_version().await?,
            connected_at: chrono::Utc::now(), // TODO: 実際の接続時刻
            last_activity: chrono::Utc::now(),
        })
    }
}
```

## 📊 Phase 4 成果物

- ✅ セッション追跡・監視
- ✅ 接続プール最適化
- ✅ パフォーマンス診断
- ✅ セキュリティ監査

---

## 🚀 Phase 5: 高度な機能 & 最適化

**期間**: Week 6-7 (8-10日)
**優先度**: 🟢 Medium

## 📝 Phase 5 実装内容

### 5.1 接続プール最適化 (2日)

- 動的プールサイズ調整
- 接続健全性チェック
- コネクションリーク検出

### 5.2 SSL/TLS強化 (2日)

- 証明書検証強化
- 暗号化方式選択
- セキュア接続モード強制

### 5.3 パフォーマンス監視 (2日)

- クエリ実行時間測定
- スロークエリログ統合
- メトリクス収集

### 5.4 バックアップ・復元 (2日)

- mysqldump統合
- ポイントインタイム復元
- 自動バックアップ

---

## 🧪 Phase 6: 包括的テスト & ドキュメント

**期間**: Week 8 (6-8日)
**優先度**: 🔴 Critical

## 📝 Phase 6 実装内容

### 6.1 単体テスト (2日)

- 全機能カバー
- エラーケーステスト
- エッジケース検証

### 6.2 統合テスト (2日)

- 実際のMySQLサーバーテスト
- トランザクション整合性
- 同時接続テスト

### 6.3 パフォーマンステスト (1日)

- ベンチマーク作成
- メモリリーク検証
- 高負荷テスト

### 6.4 ドキュメント作成 (2日)

- API仕様書
- 設定ガイド
- トラブルシューティング

### 6.5 例題・チュートリアル (1日)

- 基本的な使用例
- 高度な機能例
- ベストプラクティス

---

## 📊 工数配分詳細

| カテゴリ | 工数 | 割合 |
|----------|------|------|
| 核心機能実装 | 20-25日 | 50% |
| エラー処理・検証 | 8-10日 | 20% |
| テスト・品質保証 | 8-10日 | 20% |
| ドキュメント・例題 | 4-5日 | 10% |
| **合計** | **40-50日** | **100%** |

---

## 🎯 マイルストーン

## 🏁 Milestone 1: 基本機能完成 (Week 3終了)

- ✅ パラメータ化クエリ
- ✅ トランザクション
- ✅ 本番環境利用可能レベル

## 🏁 Milestone 2: 完全機能実装 (Week 5終了)

- ✅ スキーマ情報取得
- ✅ セッション管理
- ✅ エンタープライズレベル機能

## 🏁 Milestone 3: 製品品質達成 (Week 8終了)

- ✅ 包括的テスト
- ✅ 完全ドキュメント
- ✅ 本格運用対応

---

## 🔧 技術的考慮事項

## 依存関係管理

```toml
[dependencies]
mysql_async = { version = "0.36", features = ["default"] }
tokio = { version = "1.48", features = ["full"] }
chrono = { version = "0.4", features = ["serde"] }
```

## エラーハンドリング戦略

- カスタムエラー型定義
- 自動リトライ機能
- グレースフルデグラデーション

## パフォーマンス目標

- クエリ応答時間: < 100ms (99パーセンタイル)
- 同時接続数: 1000+
- メモリ使用量: < 100MB

---

## 📋 リスク評価・対策

## 🔴 高リスク

1. **mysql_async APIの変更**
   - 対策: バージョン固定、テスト強化
2. **MySQL互換性問題**
   - 対策: 複数バージョンテスト
3. **パフォーマンス劣化**
   - 対策: 継続的ベンチマーク

## 🟡 中リスク

1. **複雑なトランザクション処理**
   - 対策: 段階的実装、詳細テスト
2. **メモリリーク**
   - 対策: プロファイリング、自動テスト

---

## 📈 品質保証計画

## コードレビュー

- 各Phase終了時
- セキュリティ重点チェック
- パフォーマンス影響評価

## 自動テスト

- CI/CD統合
- 毎プルリクエスト実行
- カバレッジ目標: 90%+

## 本番環境テスト

- ステージング環境検証
- 段階的デプロイ
- モニタリング強化

---

## 🎉 完成後の成果

## 機能面

- ✅ 本格的な本番環境対応
- ✅ PostgreSQLと同等の機能性
- ✅ エンタープライズレベル品質

## 技術面

- ✅ セキュリティ強化
- ✅ パフォーマンス最適化
- ✅ 保守性向上

## ビジネス面

- ✅ MySQL利用企業への完全対応
- ✅ 市場シェア拡大（MySQL: 30%+）
- ✅ 競合優位性確立

---

**この実装計画により、mcp-rsのMySQLサポートを世界水準まで押し上げます！** 🚀
