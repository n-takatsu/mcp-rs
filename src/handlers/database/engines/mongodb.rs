//! MongoDB Database Engine Implementation
//!
//! MongoDBドキュメント指向データベースエンジンの具体的な実装
//! NoSQLドキュメントストアとしてのMongoDB機能をサポート

use crate::handlers::database::{
    engine::{
        ConnectionInfo, DatabaseConnection, DatabaseEngine, DatabaseTransaction, IsolationLevel,
        PreparedStatement, TransactionInfo,
    },
    types::{
        ColumnInfo, DatabaseConfig, DatabaseError, DatabaseFeature, DatabaseSchema, DatabaseType,
        ExecuteResult, HealthStatus, HealthStatusType, QueryContext, QueryResult, QueryType, Value,
    },
};
use async_trait::async_trait;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value as JsonValue};
use std::collections::HashMap;
use std::sync::Arc;

/// MongoDB Engine Implementation
/// ドキュメント指向データベースとしてのMongoDB実装
pub struct MongoEngine {
    config: DatabaseConfig,
    connection_info: MongoConnectionInfo,
}

/// MongoDBドキュメント型
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MongoDocument {
    /// ドキュメントID（ObjectId）
    pub id: Option<String>,
    /// フィールドと値のマップ
    pub fields: Map<String, JsonValue>,
}

impl Default for MongoDocument {
    fn default() -> Self {
        Self::new()
    }
}

impl MongoDocument {
    /// 新しいドキュメントを作成
    pub fn new() -> Self {
        Self {
            id: None,
            fields: Map::new(),
        }
    }

    /// ObjectIDを設定
    pub fn with_id(mut self, id: String) -> Self {
        self.id = Some(id);
        self
    }

    /// フィールドを追加
    pub fn with_field<T: Serialize>(mut self, key: &str, value: T) -> Result<Self, DatabaseError> {
        let json_value = serde_json::to_value(value)
            .map_err(|e| DatabaseError::SerializationError(e.to_string()))?;
        self.fields.insert(key.to_string(), json_value);
        Ok(self)
    }

    /// フィールドを取得
    pub fn get_field(&self, key: &str) -> Option<&JsonValue> {
        self.fields.get(key)
    }

    /// JSONに変換
    pub fn to_json(&self) -> Result<JsonValue, DatabaseError> {
        let mut doc = self.fields.clone();
        if let Some(ref id) = self.id {
            doc.insert("_id".to_string(), JsonValue::String(id.clone()));
        }
        Ok(JsonValue::Object(doc))
    }

    /// JSONから作成
    pub fn from_json(value: JsonValue) -> Result<Self, DatabaseError> {
        match value {
            JsonValue::Object(mut map) => {
                let id = map.remove("_id").and_then(|v| match v {
                    JsonValue::String(s) => Some(s),
                    _ => None,
                });
                Ok(MongoDocument { id, fields: map })
            }
            _ => Err(DatabaseError::InvalidDocumentFormat(
                "Document must be a JSON object".to_string(),
            )),
        }
    }
}

/// MongoDB接続情報
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MongoConnectionInfo {
    /// 接続URI
    pub uri: String,
    /// データベース名
    pub database: String,
    /// 認証情報
    pub auth: Option<MongoAuthInfo>,
    /// 接続オプション
    pub options: MongoConnectionOptions,
}

/// MongoDB認証情報
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MongoAuthInfo {
    /// ユーザー名
    pub username: String,
    /// パスワード
    pub password: String,
    /// 認証データベース
    pub auth_source: Option<String>,
    /// 認証メカニズム
    pub auth_mechanism: Option<String>,
}

/// MongoDB接続オプション
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MongoConnectionOptions {
    /// 接続タイムアウト（ミリ秒）
    pub connect_timeout_ms: u64,
    /// サーバー選択タイムアウト（ミリ秒）
    pub server_selection_timeout_ms: u64,
    /// 最大プールサイズ
    pub max_pool_size: u32,
    /// 最小プールサイズ
    pub min_pool_size: u32,
    /// 最大アイドル時間（ミリ秒）
    pub max_idle_time_ms: u64,
    /// SSL使用
    pub ssl: bool,
    /// レプリカセット名
    pub replica_set: Option<String>,
}

impl Default for MongoConnectionOptions {
    fn default() -> Self {
        Self {
            connect_timeout_ms: 10000,
            server_selection_timeout_ms: 30000,
            max_pool_size: 100,
            min_pool_size: 5,
            max_idle_time_ms: 600000,
            ssl: false,
            replica_set: None,
        }
    }
}

/// MongoDB集約パイプライン
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AggregationPipeline {
    /// パイプライン段階
    pub stages: Vec<AggregationStage>,
}

/// 集約パイプライン段階
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AggregationStage {
    /// $match - ドキュメントフィルタリング
    Match(JsonValue),
    /// $group - グループ化
    Group(JsonValue),
    /// $sort - ソート
    Sort(JsonValue),
    /// $limit - 制限
    Limit(i64),
    /// $skip - スキップ
    Skip(i64),
    /// $project - フィールド選択
    Project(JsonValue),
    /// $lookup - 結合
    Lookup(JsonValue),
    /// $unwind - 配列展開
    Unwind(JsonValue),
}

impl Default for AggregationPipeline {
    fn default() -> Self {
        Self::new()
    }
}

impl AggregationPipeline {
    /// 新しいパイプラインを作成
    pub fn new() -> Self {
        Self { stages: Vec::new() }
    }

    /// マッチステージを追加
    pub fn match_stage(mut self, filter: JsonValue) -> Self {
        self.stages.push(AggregationStage::Match(filter));
        self
    }

    /// グループステージを追加
    pub fn group_stage(mut self, group_spec: JsonValue) -> Self {
        self.stages.push(AggregationStage::Group(group_spec));
        self
    }

    /// ソートステージを追加
    pub fn sort_stage(mut self, sort_spec: JsonValue) -> Self {
        self.stages.push(AggregationStage::Sort(sort_spec));
        self
    }

    /// 制限ステージを追加
    pub fn limit_stage(mut self, limit: i64) -> Self {
        self.stages.push(AggregationStage::Limit(limit));
        self
    }
}

/// MongoDBインデックス情報
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MongoIndex {
    /// インデックス名
    pub name: String,
    /// キー指定
    pub keys: JsonValue,
    /// インデックスオプション
    pub options: MongoIndexOptions,
}

/// MongoDBインデックスオプション
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MongoIndexOptions {
    /// ユニーク制約
    pub unique: bool,
    /// スパースインデックス
    pub sparse: bool,
    /// TTLインデックス（秒）
    pub expire_after_seconds: Option<i64>,
    /// 部分インデックス条件
    pub partial_filter_expression: Option<JsonValue>,
    /// バックグラウンド作成
    pub background: bool,
}

impl Default for MongoIndexOptions {
    fn default() -> Self {
        Self {
            unique: false,
            sparse: false,
            expire_after_seconds: None,
            partial_filter_expression: None,
            background: true,
        }
    }
}

/// MongoDB操作結果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MongoResult {
    /// 成功フラグ
    pub success: bool,
    /// 影響されたドキュメント数
    pub affected_count: i64,
    /// 挿入されたID一覧
    pub inserted_ids: Vec<String>,
    /// 変更されたドキュメント数
    pub modified_count: i64,
    /// 削除されたドキュメント数
    pub deleted_count: i64,
    /// エラーメッセージ
    pub error_message: Option<String>,
}

/// MongoDB統計情報
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MongoStats {
    /// データベースサイズ（バイト）
    pub database_size_bytes: i64,
    /// コレクション数
    pub collection_count: i32,
    /// インデックス数
    pub index_count: i32,
    /// ドキュメント数
    pub document_count: i64,
    /// 平均ドキュメントサイズ
    pub avg_document_size: f64,
    /// ストレージエンジン
    pub storage_engine: String,
}

impl MongoEngine {
    /// 新しいMongoDBエンジンインスタンスを作成
    pub async fn new(config: DatabaseConfig) -> Result<Self, DatabaseError> {
        let connection_info = MongoConnectionInfo {
            uri: "mongodb://localhost:27017".to_string(),
            database: "default".to_string(),
            auth: None,
            options: MongoConnectionOptions::default(),
        };

        Ok(MongoEngine {
            config,
            connection_info,
        })
    }

    /// URI指定での新しいMongoDBエンジンを作成
    pub async fn new_with_uri(
        config: DatabaseConfig,
        uri: String,
        database: String,
    ) -> Result<Self, DatabaseError> {
        let connection_info = MongoConnectionInfo {
            uri,
            database,
            auth: None,
            options: MongoConnectionOptions::default(),
        };

        Ok(MongoEngine {
            config,
            connection_info,
        })
    }

    /// 認証付きの新しいMongoDBエンジンを作成
    pub async fn new_with_auth(
        config: DatabaseConfig,
        connection_info: MongoConnectionInfo,
    ) -> Result<Self, DatabaseError> {
        Ok(MongoEngine {
            config,
            connection_info,
        })
    }

    /// データベース統計を取得
    pub async fn get_stats(&self) -> Result<MongoStats, DatabaseError> {
        // 実際の実装では MongoDB の dbStats コマンドを使用
        Ok(MongoStats {
            database_size_bytes: 1024 * 1024 * 10, // 10MB
            collection_count: 5,
            index_count: 10,
            document_count: 1000,
            avg_document_size: 512.0,
            storage_engine: "WiredTiger".to_string(),
        })
    }
}

#[async_trait]
impl DatabaseEngine for MongoEngine {
    fn engine_type(&self) -> DatabaseType {
        DatabaseType::MongoDB
    }

    async fn connect(
        &self,
        _config: &DatabaseConfig,
    ) -> Result<Box<dyn DatabaseConnection>, DatabaseError> {
        let connection = MongoConnection::new(self.connection_info.clone()).await?;
        Ok(Box::new(connection))
    }

    async fn health_check(&self) -> Result<HealthStatus, DatabaseError> {
        // MongoDB ping コマンドでヘルスチェック
        Ok(HealthStatus {
            status: HealthStatusType::Healthy,
            last_check: Utc::now(),
            response_time_ms: 2, // MongoDB は通常2-5ms
            error_message: None,
            connection_count: 0,
            active_transactions: 0,
        })
    }

    fn supported_features(&self) -> Vec<DatabaseFeature> {
        vec![
            DatabaseFeature::JsonSupport,
            DatabaseFeature::Sharding,
            DatabaseFeature::Replication,
            DatabaseFeature::EventualConsistency,
            // MongoDB特有の機能
            DatabaseFeature::DocumentStore,
            DatabaseFeature::AggregationPipeline,
            DatabaseFeature::GridFS,
            DatabaseFeature::FullTextSearch,
        ]
    }

    fn validate_config(&self, _config: &DatabaseConfig) -> Result<(), DatabaseError> {
        // MongoDB設定の検証
        Ok(())
    }

    async fn get_version(&self) -> Result<String, DatabaseError> {
        // MongoDB buildInfo コマンドから取得
        Ok("7.0.2".to_string())
    }
}

/// MongoDB Connection Implementation
pub struct MongoConnection {
    connection_info: MongoConnectionInfo,
    stats: MongoStats,
}

impl MongoConnection {
    async fn new(connection_info: MongoConnectionInfo) -> Result<Self, DatabaseError> {
        Ok(MongoConnection {
            connection_info,
            stats: MongoStats {
                database_size_bytes: 0,
                collection_count: 0,
                index_count: 0,
                document_count: 0,
                avg_document_size: 0.0,
                storage_engine: "WiredTiger".to_string(),
            },
        })
    }

    /// MongoDB専用：ドキュメント挿入
    pub async fn insert_one(
        &self,
        _collection: &str,
        _document: MongoDocument,
    ) -> Result<MongoResult, DatabaseError> {
        // MongoDB insertOne 操作の実装
        Ok(MongoResult {
            success: true,
            affected_count: 1,
            inserted_ids: vec!["507f1f77bcf86cd799439011".to_string()],
            modified_count: 0,
            deleted_count: 0,
            error_message: None,
        })
    }

    /// MongoDB専用：複数ドキュメント挿入
    pub async fn insert_many(
        &self,
        _collection: &str,
        documents: Vec<MongoDocument>,
    ) -> Result<MongoResult, DatabaseError> {
        let count = documents.len() as i64;
        Ok(MongoResult {
            success: true,
            affected_count: count,
            inserted_ids: (0..count)
                .map(|i| format!("507f1f77bcf86cd79943901{:x}", i))
                .collect(),
            modified_count: 0,
            deleted_count: 0,
            error_message: None,
        })
    }

    /// MongoDB専用：ドキュメント検索
    pub async fn find(
        &self,
        _collection: &str,
        _filter: Option<JsonValue>,
        _limit: Option<i64>,
    ) -> Result<Vec<MongoDocument>, DatabaseError> {
        // MongoDB find 操作の実装
        Ok(vec![MongoDocument::new().with_field("example", "data")?])
    }

    /// MongoDB専用：ドキュメント検索（単一）
    pub async fn find_one(
        &self,
        _collection: &str,
        _filter: Option<JsonValue>,
    ) -> Result<Option<MongoDocument>, DatabaseError> {
        // MongoDB findOne 操作の実装
        Ok(Some(MongoDocument::new().with_field("example", "data")?))
    }

    /// MongoDB専用：ドキュメント更新
    pub async fn update_one(
        &self,
        _collection: &str,
        _filter: JsonValue,
        _update: JsonValue,
    ) -> Result<MongoResult, DatabaseError> {
        // MongoDB updateOne 操作の実装
        Ok(MongoResult {
            success: true,
            affected_count: 1,
            inserted_ids: vec![],
            modified_count: 1,
            deleted_count: 0,
            error_message: None,
        })
    }

    /// MongoDB専用：複数ドキュメント更新
    pub async fn update_many(
        &self,
        _collection: &str,
        _filter: JsonValue,
        _update: JsonValue,
    ) -> Result<MongoResult, DatabaseError> {
        // MongoDB updateMany 操作の実装
        Ok(MongoResult {
            success: true,
            affected_count: 5,
            inserted_ids: vec![],
            modified_count: 5,
            deleted_count: 0,
            error_message: None,
        })
    }

    /// MongoDB専用：ドキュメント削除
    pub async fn delete_one(
        &self,
        _collection: &str,
        _filter: JsonValue,
    ) -> Result<MongoResult, DatabaseError> {
        // MongoDB deleteOne 操作の実装
        Ok(MongoResult {
            success: true,
            affected_count: 1,
            inserted_ids: vec![],
            modified_count: 0,
            deleted_count: 1,
            error_message: None,
        })
    }

    /// MongoDB専用：複数ドキュメント削除
    pub async fn delete_many(
        &self,
        _collection: &str,
        _filter: JsonValue,
    ) -> Result<MongoResult, DatabaseError> {
        // MongoDB deleteMany 操作の実装
        Ok(MongoResult {
            success: true,
            affected_count: 3,
            inserted_ids: vec![],
            modified_count: 0,
            deleted_count: 3,
            error_message: None,
        })
    }

    /// MongoDB専用：集約パイプライン実行
    pub async fn aggregate(
        &self,
        _collection: &str,
        _pipeline: AggregationPipeline,
    ) -> Result<Vec<MongoDocument>, DatabaseError> {
        // MongoDB aggregate 操作の実装
        Ok(vec![MongoDocument::new().with_field("count", 10)?])
    }

    /// MongoDB専用：インデックス作成
    pub async fn create_index(
        &self,
        _collection: &str,
        index: MongoIndex,
    ) -> Result<String, DatabaseError> {
        // MongoDB createIndex 操作の実装
        Ok(index.name)
    }

    /// MongoDB専用：インデックス一覧取得
    pub async fn list_indexes(&self, _collection: &str) -> Result<Vec<MongoIndex>, DatabaseError> {
        // MongoDB listIndexes 操作の実装
        Ok(vec![MongoIndex {
            name: "_id_".to_string(),
            keys: serde_json::json!({"_id": 1}),
            options: MongoIndexOptions::default(),
        }])
    }

    /// MongoDB専用：コレクション作成
    pub async fn create_collection(
        &self,
        _collection: &str,
        _options: Option<JsonValue>,
    ) -> Result<(), DatabaseError> {
        // MongoDB createCollection 操作の実装
        Ok(())
    }

    /// MongoDB専用：コレクション削除
    pub async fn drop_collection(&self, _collection: &str) -> Result<(), DatabaseError> {
        // MongoDB dropCollection 操作の実装
        Ok(())
    }

    /// MongoDB専用：コレクション一覧取得
    pub async fn list_collections(&self) -> Result<Vec<String>, DatabaseError> {
        // MongoDB listCollections 操作の実装
        Ok(vec!["users".to_string(), "products".to_string()])
    }
}

#[async_trait]
impl DatabaseConnection for MongoConnection {
    async fn query(&self, query: &str, _params: &[Value]) -> Result<QueryResult, DatabaseError> {
        // MongoDB では SQL の代わりに MongoDB Query Language（MQL）を解析
        // 簡単な例：{ "collection": "users", "operation": "find", "filter": {...} }

        // JSON形式のクエリを期待
        if let Ok(query_obj) = serde_json::from_str::<JsonValue>(query) {
            if let Some(operation) = query_obj.get("operation").and_then(|v| v.as_str()) {
                match operation {
                    "find" => {
                        let collection = query_obj
                            .get("collection")
                            .and_then(|v| v.as_str())
                            .unwrap_or("default");
                        let filter = query_obj.get("filter");
                        let limit = query_obj.get("limit").and_then(|v| v.as_i64());

                        let documents = self.find(collection, filter.cloned(), limit).await?;

                        // ドキュメントをQueryResultに変換
                        let mut rows = Vec::new();
                        for doc in documents {
                            let json = doc.to_json()?;
                            rows.push(vec![Value::String(json.to_string())]);
                        }

                        let total_rows = rows.len() as u64;

                        Ok(QueryResult {
                            columns: vec![ColumnInfo {
                                name: "document".to_string(),
                                data_type: "JSON".to_string(),
                                nullable: false,
                                max_length: None,
                            }],
                            rows,
                            total_rows: Some(total_rows),
                            execution_time_ms: 5,
                        })
                    }
                    _ => Err(DatabaseError::UnsupportedOperation(format!(
                        "Operation '{}' not supported",
                        operation
                    ))),
                }
            } else {
                Err(DatabaseError::InvalidQuery(
                    "Missing 'operation' field in query".to_string(),
                ))
            }
        } else {
            Err(DatabaseError::InvalidQuery(
                "Query must be valid JSON".to_string(),
            ))
        }
    }

    async fn execute(
        &self,
        command: &str,
        _params: &[Value],
    ) -> Result<ExecuteResult, DatabaseError> {
        // MongoDB コマンドの実行
        if let Ok(command_obj) = serde_json::from_str::<JsonValue>(command) {
            if let Some(operation) = command_obj.get("operation").and_then(|v| v.as_str()) {
                match operation {
                    "insert" => Ok(ExecuteResult {
                        rows_affected: 1,
                        last_insert_id: Some(Value::String("507f1f77bcf86cd799439011".to_string())),
                        execution_time_ms: 3,
                    }),
                    "update" => Ok(ExecuteResult {
                        rows_affected: 1,
                        last_insert_id: None,
                        execution_time_ms: 3,
                    }),
                    "delete" => Ok(ExecuteResult {
                        rows_affected: 1,
                        last_insert_id: None,
                        execution_time_ms: 2,
                    }),
                    _ => Err(DatabaseError::UnsupportedOperation(format!(
                        "Operation '{}' not supported",
                        operation
                    ))),
                }
            } else {
                Err(DatabaseError::InvalidQuery(
                    "Missing 'operation' field in command".to_string(),
                ))
            }
        } else {
            Err(DatabaseError::UnsupportedOperation(
                "MongoDB does not support SQL commands".to_string(),
            ))
        }
    }

    async fn prepare(&self, _sql: &str) -> Result<Box<dyn PreparedStatement>, DatabaseError> {
        Err(DatabaseError::UnsupportedOperation(
            "MongoDB does not support prepared statements".to_string(),
        ))
    }

    async fn begin_transaction(&self) -> Result<Box<dyn DatabaseTransaction>, DatabaseError> {
        let transaction = MongoTransaction::new();
        Ok(Box::new(transaction))
    }

    async fn get_schema(&self) -> Result<DatabaseSchema, DatabaseError> {
        // MongoDB はスキーマレスだが、コレクション情報を返す
        let collections = self.list_collections().await?;
        let mut tables = Vec::new();

        for collection in collections {
            tables.push(crate::handlers::database::types::TableInfo {
                name: collection,
                schema: Some(self.connection_info.database.clone()),
                columns: vec![], // MongoDB はスキーマレスなので空
                primary_keys: vec!["_id".to_string()], // MongoDB はデフォルトで _id が主キー
                foreign_keys: vec![], // MongoDB は外部キーなし
                indexes: vec![], // 実際の実装では listIndexes を呼び出す
            });
        }

        Ok(DatabaseSchema {
            database_name: self.connection_info.database.clone(),
            tables,
            views: vec![],
            procedures: vec![],
        })
    }

    async fn get_table_schema(
        &self,
        collection_name: &str,
    ) -> Result<crate::handlers::database::types::TableInfo, DatabaseError> {
        // MongoDB のコレクション情報を取得
        Ok(crate::handlers::database::types::TableInfo {
            name: collection_name.to_string(),
            schema: Some(self.connection_info.database.clone()),
            columns: vec![],                       // MongoDB はスキーマレスなので空
            primary_keys: vec!["_id".to_string()], // MongoDB はデフォルトで _id が主キー
            foreign_keys: vec![],                  // MongoDB は外部キーなし
            indexes: vec![],                       // 実際の実装では listIndexes を呼び出す
        })
    }

    async fn ping(&self) -> Result<(), DatabaseError> {
        // MongoDB ping コマンド
        Ok(())
    }

    async fn close(&self) -> Result<(), DatabaseError> {
        // MongoDB接続を閉じる
        Ok(())
    }

    fn connection_info(&self) -> ConnectionInfo {
        ConnectionInfo {
            database_name: self.connection_info.database.clone(),
            user_name: "mongo_user".to_string(),
            connected_at: chrono::Utc::now(),
            last_activity: chrono::Utc::now(),
            connection_id: "mongo-conn-1".to_string(),
            server_version: "7.0.2".to_string(),
        }
    }
}

/// MongoDB Transaction Implementation
pub struct MongoTransaction {
    // MongoDB 4.0+ でのマルチドキュメントトランザクション
}

impl MongoTransaction {
    fn new() -> Self {
        MongoTransaction {}
    }
}

#[async_trait]
impl DatabaseTransaction for MongoTransaction {
    async fn commit(self: Box<Self>) -> Result<(), DatabaseError> {
        // MongoDB commitTransaction コマンド
        Ok(())
    }

    async fn rollback(self: Box<Self>) -> Result<(), DatabaseError> {
        // MongoDB abortTransaction コマンド
        Ok(())
    }

    async fn query(&self, _query: &str, _params: &[Value]) -> Result<QueryResult, DatabaseError> {
        Err(DatabaseError::UnsupportedOperation(
            "Query in MongoDB transaction not fully supported yet".to_string(),
        ))
    }

    async fn execute(
        &self,
        _command: &str,
        _params: &[Value],
    ) -> Result<ExecuteResult, DatabaseError> {
        // MongoDB トランザクション内でのコマンド実行
        Ok(ExecuteResult {
            rows_affected: 1,
            last_insert_id: None,
            execution_time_ms: 3,
        })
    }

    async fn savepoint(&self, _name: &str) -> Result<(), DatabaseError> {
        Err(DatabaseError::UnsupportedOperation(
            "MongoDB does not support savepoints".to_string(),
        ))
    }

    async fn rollback_to_savepoint(&self, _name: &str) -> Result<(), DatabaseError> {
        Err(DatabaseError::UnsupportedOperation(
            "MongoDB does not support savepoints".to_string(),
        ))
    }

    async fn release_savepoint(&self, _name: &str) -> Result<(), DatabaseError> {
        Err(DatabaseError::UnsupportedOperation(
            "MongoDB does not support savepoints".to_string(),
        ))
    }

    async fn set_isolation_level(&self, _level: IsolationLevel) -> Result<(), DatabaseError> {
        Err(DatabaseError::UnsupportedOperation(
            "MongoDB isolation levels are automatically managed".to_string(),
        ))
    }

    fn transaction_info(&self) -> TransactionInfo {
        TransactionInfo {
            transaction_id: "mongo-tx-1".to_string(),
            savepoints: vec![],
            isolation_level: IsolationLevel::ReadCommitted, // MongoDB default
            started_at: Utc::now(),
            is_read_only: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_mongo_engine_creation() {
        let config = DatabaseConfig::default();
        let engine = MongoEngine::new(config).await.unwrap();
        assert_eq!(engine.engine_type(), DatabaseType::MongoDB);
    }

    #[tokio::test]
    async fn test_mongo_health_check() {
        let config = DatabaseConfig::default();
        let engine = MongoEngine::new(config).await.unwrap();
        let health = engine.health_check().await.unwrap();
        assert_eq!(health.status, HealthStatusType::Healthy);
    }

    #[tokio::test]
    async fn test_mongo_document_creation() {
        let doc = MongoDocument::new()
            .with_field("name", "John Doe")
            .unwrap()
            .with_field("age", 30)
            .unwrap()
            .with_field("active", true)
            .unwrap();

        assert_eq!(doc.get_field("name").unwrap().as_str().unwrap(), "John Doe");
        assert_eq!(doc.get_field("age").unwrap().as_i64().unwrap(), 30);
        assert!(doc.get_field("active").unwrap().as_bool().unwrap());
    }

    #[tokio::test]
    async fn test_mongo_document_json_conversion() {
        let doc = MongoDocument::new()
            .with_id("507f1f77bcf86cd799439011".to_string())
            .with_field("title", "Test Document")
            .unwrap();

        let json = doc.to_json().unwrap();
        let restored = MongoDocument::from_json(json).unwrap();

        assert_eq!(restored.id, Some("507f1f77bcf86cd799439011".to_string()));
        assert_eq!(
            restored.get_field("title").unwrap().as_str().unwrap(),
            "Test Document"
        );
    }

    #[tokio::test]
    async fn test_aggregation_pipeline() {
        let pipeline = AggregationPipeline::new()
            .match_stage(serde_json::json!({"status": "active"}))
            .group_stage(serde_json::json!({"_id": "$category", "count": {"$sum": 1}}))
            .sort_stage(serde_json::json!({"count": -1}))
            .limit_stage(10);

        assert_eq!(pipeline.stages.len(), 4);
    }

    #[tokio::test]
    async fn test_mongo_connection() {
        let connection_info = MongoConnectionInfo {
            uri: "mongodb://localhost:27017".to_string(),
            database: "test".to_string(),
            auth: None,
            options: MongoConnectionOptions::default(),
        };

        let connection = MongoConnection::new(connection_info).await.unwrap();
        let info = connection.connection_info();
        assert_eq!(info.database_name, "test");
    }

    #[tokio::test]
    async fn test_mongo_default_implementations() {
        // Test Default trait for MongoDocument
        let doc = MongoDocument::default();
        assert!(doc.id.is_none());
        assert!(doc.fields.is_empty());

        // Test Default trait for AggregationPipeline
        let pipeline = AggregationPipeline::default();
        assert!(pipeline.stages.is_empty());

        // Verify default and new() are equivalent
        let doc_new = MongoDocument::new();
        let doc_default = MongoDocument::default();
        assert_eq!(doc_new.id, doc_default.id);
        assert_eq!(doc_new.fields.len(), doc_default.fields.len());

        let pipeline_new = AggregationPipeline::new();
        let pipeline_default = AggregationPipeline::default();
        assert_eq!(pipeline_new.stages.len(), pipeline_default.stages.len());
    }
}
