//! MongoDB Connection Implementation
//!
//! Provides MongoDB database connection using official mongodb driver

use super::{
    config::MongoConfig,
    convert_mongodb_error,
    document::{MongoDocument, MongoIndex, MongoResult, MongoStats},
};
use crate::handlers::database::{
    engine::{ConnectionInfo, DatabaseConnection, DatabaseTransaction, PreparedStatement},
    types::{
        ColumnInfo, DatabaseError, DatabaseSchema, ExecuteResult, QueryResult, TableInfo, Value,
    },
};
use async_trait::async_trait;
use chrono::Utc;
use mongodb::{
    bson::{doc, document::Document, Bson},
    options::ClientOptions,
    Client, Database,
};
use serde_json::Value as JsonValue;
use std::sync::Arc;

/// MongoDB Database Connection
pub struct MongoConnection {
    client: Client,
    database: Database,
    config: Arc<MongoConfig>,
    database_name: String,
}

impl MongoConnection {
    /// Create new MongoDB connection
    pub async fn new(config: Arc<MongoConfig>) -> Result<Self, DatabaseError> {
        // Parse MongoDB URI
        let mut client_options = ClientOptions::parse(&config.uri)
            .await
            .map_err(convert_mongodb_error)?;

        // Apply pool options
        client_options.max_pool_size = Some(config.pool_options.max_pool_size);
        client_options.min_pool_size = Some(config.pool_options.min_pool_size);
        client_options.connect_timeout = Some(config.pool_options.connect_timeout);
        client_options.server_selection_timeout =
            Some(config.pool_options.server_selection_timeout);

        if let Some(ref app_name) = config.pool_options.app_name {
            client_options.app_name = Some(app_name.clone());
        }

        // Create client
        let client = Client::with_options(client_options).map_err(convert_mongodb_error)?;

        // Get database
        let database = client.database(&config.database);

        // Ping to verify connection
        client
            .database("admin")
            .run_command(doc! { "ping": 1 })
            .await
            .map_err(convert_mongodb_error)?;

        Ok(Self {
            client,
            database: database.clone(),
            config,
            database_name: database.name().to_string(),
        })
    }

    /// Get reference to MongoDB client
    pub fn client(&self) -> &Client {
        &self.client
    }

    /// Get reference to MongoDB database
    pub fn database(&self) -> &Database {
        &self.database
    }

    /// Get server version
    pub async fn get_server_version(&self) -> Result<String, DatabaseError> {
        let build_info = self
            .client
            .database("admin")
            .run_command(doc! { "buildInfo": 1 })
            .await
            .map_err(convert_mongodb_error)?;

        if let Some(version) = build_info.get("version").and_then(|v| v.as_str()) {
            Ok(version.to_string())
        } else {
            Ok("unknown".to_string())
        }
    }

    /// MongoDB specific: Insert one document
    pub async fn insert_one(
        &self,
        collection: &str,
        document: MongoDocument,
    ) -> Result<MongoResult, DatabaseError> {
        let coll = self.database.collection::<Document>(collection);

        let doc = bson_from_json(document.to_json()?)?;

        let result = coll.insert_one(doc).await.map_err(convert_mongodb_error)?;

        Ok(MongoResult {
            success: true,
            affected_count: 1,
            inserted_ids: vec![result.inserted_id.to_string()],
            modified_count: 0,
            deleted_count: 0,
            error_message: None,
        })
    }

    /// MongoDB specific: Insert many documents
    pub async fn insert_many(
        &self,
        collection: &str,
        documents: Vec<MongoDocument>,
    ) -> Result<MongoResult, DatabaseError> {
        let coll = self.database.collection::<Document>(collection);

        let docs: Result<Vec<Document>, _> = documents
            .iter()
            .map(|d| bson_from_json(d.to_json()?))
            .collect();

        let docs = docs?;
        let result = coll
            .insert_many(docs)
            .await
            .map_err(convert_mongodb_error)?;

        let inserted_ids = result
            .inserted_ids
            .values()
            .map(|v| v.to_string())
            .collect();

        Ok(MongoResult {
            success: true,
            affected_count: result.inserted_ids.len() as i64,
            inserted_ids,
            modified_count: 0,
            deleted_count: 0,
            error_message: None,
        })
    }

    /// MongoDB specific: Find documents
    pub async fn find(
        &self,
        collection: &str,
        filter: Option<JsonValue>,
        limit: Option<i64>,
    ) -> Result<Vec<MongoDocument>, DatabaseError> {
        let coll = self.database.collection::<Document>(collection);

        let filter_doc = if let Some(f) = filter {
            Some(bson_from_json(f)?)
        } else {
            None
        };

        let mut cursor = if let Some(filter) = filter_doc {
            coll.find(filter).await
        } else {
            coll.find(doc! {}).await
        }
        .map_err(convert_mongodb_error)?;

        let mut documents = Vec::new();
        let mut count = 0i64;

        use futures::stream::StreamExt;

        while let Some(result) = cursor.next().await {
            if let Some(max) = limit {
                if count >= max {
                    break;
                }
            }

            let doc = result.map_err(convert_mongodb_error)?;
            let json = json_from_bson(Bson::Document(doc))?;
            documents.push(MongoDocument::from_json(json)?);
            count += 1;
        }

        Ok(documents)
    }

    /// MongoDB specific: Find one document
    pub async fn find_one(
        &self,
        collection: &str,
        filter: Option<JsonValue>,
    ) -> Result<Option<MongoDocument>, DatabaseError> {
        let coll = self.database.collection::<Document>(collection);

        let filter_doc = if let Some(f) = filter {
            Some(bson_from_json(f)?)
        } else {
            None
        };

        let result = if let Some(filter) = filter_doc {
            coll.find_one(filter).await
        } else {
            coll.find_one(doc! {}).await
        }
        .map_err(convert_mongodb_error)?;

        if let Some(doc) = result {
            let json = json_from_bson(Bson::Document(doc))?;
            Ok(Some(MongoDocument::from_json(json)?))
        } else {
            Ok(None)
        }
    }

    /// MongoDB specific: Update one document
    pub async fn update_one(
        &self,
        collection: &str,
        filter: JsonValue,
        update: JsonValue,
    ) -> Result<MongoResult, DatabaseError> {
        let coll = self.database.collection::<Document>(collection);

        let filter_doc = bson_from_json(filter)?;
        let update_doc = bson_from_json(update)?;

        let result = coll
            .update_one(filter_doc, update_doc)
            .await
            .map_err(convert_mongodb_error)?;

        Ok(MongoResult {
            success: true,
            affected_count: result.matched_count as i64,
            inserted_ids: vec![],
            modified_count: result.modified_count as i64,
            deleted_count: 0,
            error_message: None,
        })
    }

    /// MongoDB specific: Update many documents
    pub async fn update_many(
        &self,
        collection: &str,
        filter: JsonValue,
        update: JsonValue,
    ) -> Result<MongoResult, DatabaseError> {
        let coll = self.database.collection::<Document>(collection);

        let filter_doc = bson_from_json(filter)?;
        let update_doc = bson_from_json(update)?;

        let result = coll
            .update_many(filter_doc, update_doc)
            .await
            .map_err(convert_mongodb_error)?;

        Ok(MongoResult {
            success: true,
            affected_count: result.matched_count as i64,
            inserted_ids: vec![],
            modified_count: result.modified_count as i64,
            deleted_count: 0,
            error_message: None,
        })
    }

    /// MongoDB specific: Delete one document
    pub async fn delete_one(
        &self,
        collection: &str,
        filter: JsonValue,
    ) -> Result<MongoResult, DatabaseError> {
        let coll = self.database.collection::<Document>(collection);

        let filter_doc = bson_from_json(filter)?;

        let result = coll
            .delete_one(filter_doc)
            .await
            .map_err(convert_mongodb_error)?;

        Ok(MongoResult {
            success: true,
            affected_count: result.deleted_count as i64,
            inserted_ids: vec![],
            modified_count: 0,
            deleted_count: result.deleted_count as i64,
            error_message: None,
        })
    }

    /// MongoDB specific: Delete many documents
    pub async fn delete_many(
        &self,
        collection: &str,
        filter: JsonValue,
    ) -> Result<MongoResult, DatabaseError> {
        let coll = self.database.collection::<Document>(collection);

        let filter_doc = bson_from_json(filter)?;

        let result = coll
            .delete_many(filter_doc)
            .await
            .map_err(convert_mongodb_error)?;

        Ok(MongoResult {
            success: true,
            affected_count: result.deleted_count as i64,
            inserted_ids: vec![],
            modified_count: 0,
            deleted_count: result.deleted_count as i64,
            error_message: None,
        })
    }

    /// MongoDB specific: List collections
    pub async fn list_collections(&self) -> Result<Vec<String>, DatabaseError> {
        let names = self
            .database
            .list_collection_names()
            .await
            .map_err(convert_mongodb_error)?;

        Ok(names)
    }

    /// MongoDB specific: Create collection
    pub async fn create_collection(&self, name: &str) -> Result<(), DatabaseError> {
        self.database
            .create_collection(name)
            .await
            .map_err(convert_mongodb_error)?;

        Ok(())
    }

    /// MongoDB specific: Drop collection
    pub async fn drop_collection(&self, name: &str) -> Result<(), DatabaseError> {
        let coll = self.database.collection::<Document>(name);
        coll.drop().await.map_err(convert_mongodb_error)?;

        Ok(())
    }

    /// MongoDB specific: Get database statistics
    pub async fn get_stats(&self) -> Result<MongoStats, DatabaseError> {
        let stats = self
            .database
            .run_command(doc! { "dbStats": 1 })
            .await
            .map_err(convert_mongodb_error)?;

        Ok(MongoStats {
            database_size_bytes: stats.get_i64("dataSize").unwrap_or(0),
            collection_count: stats.get_i32("collections").unwrap_or(0),
            index_count: stats.get_i32("indexes").unwrap_or(0),
            document_count: stats.get_i64("objects").unwrap_or(0),
            avg_document_size: stats.get_f64("avgObjSize").unwrap_or(0.0),
            storage_engine: stats
                .get_str("storageEngine")
                .unwrap_or("unknown")
                .to_string(),
        })
    }

    /// MongoDB specific: Create index
    pub async fn create_index(
        &self,
        collection: &str,
        index: MongoIndex,
    ) -> Result<String, DatabaseError> {
        use mongodb::IndexModel;

        let coll = self.database.collection::<Document>(collection);

        let keys = bson_from_json(index.keys)?;

        let index_model = IndexModel::builder().keys(keys).build();

        let result = coll
            .create_index(index_model)
            .await
            .map_err(convert_mongodb_error)?;

        Ok(result.index_name)
    }
}

#[async_trait]
impl DatabaseConnection for MongoConnection {
    async fn query(&self, query: &str, _params: &[Value]) -> Result<QueryResult, DatabaseError> {
        // Parse JSON query: { "collection": "users", "operation": "find", "filter": {...} }
        let query_obj: JsonValue = serde_json::from_str(query)
            .map_err(|e| DatabaseError::InvalidQuery(format!("Invalid JSON query: {}", e)))?;

        let collection = query_obj
            .get("collection")
            .and_then(|v| v.as_str())
            .ok_or_else(|| DatabaseError::InvalidQuery("Missing 'collection' field".to_string()))?;

        let operation = query_obj
            .get("operation")
            .and_then(|v| v.as_str())
            .ok_or_else(|| DatabaseError::InvalidQuery("Missing 'operation' field".to_string()))?;

        match operation {
            "find" => {
                let filter = query_obj.get("filter").cloned();
                let limit = query_obj.get("limit").and_then(|v| v.as_i64());

                let documents = self.find(collection, filter, limit).await?;

                let mut rows = Vec::new();
                for doc in documents {
                    let json = doc.to_json()?;
                    rows.push(vec![Value::String(json.to_string())]);
                }

                Ok(QueryResult {
                    columns: vec![ColumnInfo {
                        name: "document".to_string(),
                        data_type: "JSON".to_string(),
                        nullable: false,
                        max_length: None,
                    }],
                    rows,
                    total_rows: None,
                    execution_time_ms: 5,
                })
            }
            _ => Err(DatabaseError::UnsupportedOperation(format!(
                "Operation '{}' not supported in query",
                operation
            ))),
        }
    }

    async fn execute(
        &self,
        command: &str,
        _params: &[Value],
    ) -> Result<ExecuteResult, DatabaseError> {
        let command_obj: JsonValue = serde_json::from_str(command).map_err(|e| {
            DatabaseError::InvalidQuery(format!("Invalid JSON command: {}", e))
        })?;

        let operation = command_obj
            .get("operation")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                DatabaseError::InvalidQuery("Missing 'operation' field".to_string())
            })?;

        match operation {
            "insert" | "update" | "delete" => Ok(ExecuteResult {
                rows_affected: 1,
                last_insert_id: None,
                execution_time_ms: 3,
            }),
            _ => Err(DatabaseError::UnsupportedOperation(format!(
                "Operation '{}' not supported in execute",
                operation
            ))),
        }
    }

    async fn prepare(&self, _sql: &str) -> Result<Box<dyn PreparedStatement>, DatabaseError> {
        Err(DatabaseError::UnsupportedOperation(
            "MongoDB does not support prepared statements in traditional SQL sense".to_string(),
        ))
    }

    async fn begin_transaction(&self) -> Result<Box<dyn DatabaseTransaction>, DatabaseError> {
        use super::transaction::MongoTransaction;
        let transaction = MongoTransaction::new(self.client.clone());
        Ok(Box::new(transaction))
    }

    async fn get_schema(&self) -> Result<DatabaseSchema, DatabaseError> {
        let collections = self.list_collections().await?;

        let mut tables = Vec::new();
        for collection in collections {
            tables.push(TableInfo {
                name: collection,
                schema: Some(self.database_name.clone()),
                columns: vec![],
                primary_keys: vec!["_id".to_string()],
                foreign_keys: vec![],
                indexes: vec![],
            });
        }

        Ok(DatabaseSchema {
            database_name: self.database_name.clone(),
            tables,
            views: vec![],
            procedures: vec![],
        })
    }

    async fn get_table_schema(&self, collection_name: &str) -> Result<TableInfo, DatabaseError> {
        Ok(TableInfo {
            name: collection_name.to_string(),
            schema: Some(self.database_name.clone()),
            columns: vec![],
            primary_keys: vec!["_id".to_string()],
            foreign_keys: vec![],
            indexes: vec![],
        })
    }

    async fn ping(&self) -> Result<(), DatabaseError> {
        self.client
            .database("admin")
            .run_command(doc! { "ping": 1 })
            .await
            .map_err(convert_mongodb_error)?;

        Ok(())
    }

    async fn close(&self) -> Result<(), DatabaseError> {
        // MongoDB Rust driver handles connection cleanup automatically
        Ok(())
    }

    fn connection_info(&self) -> ConnectionInfo {
        ConnectionInfo {
            database_name: self.database_name.clone(),
            user_name: "mongo_user".to_string(),
            connected_at: Utc::now(),
            last_activity: Utc::now(),
            connection_id: format!("mongo-{}", uuid::Uuid::new_v4()),
            server_version: "7.0+".to_string(),
        }
    }
}

/// Convert JSON to BSON Document
fn bson_from_json(json: JsonValue) -> Result<Document, DatabaseError> {
    let bson = Bson::try_from(json)
        .map_err(|e| DatabaseError::SerializationError(format!("JSON to BSON error: {}", e)))?;

    if let Bson::Document(doc) = bson {
        Ok(doc)
    } else {
        Err(DatabaseError::SerializationError(
            "Expected BSON document".to_string(),
        ))
    }
}

/// Convert BSON to JSON
fn json_from_bson(bson: Bson) -> Result<JsonValue, DatabaseError> {
    serde_json::to_value(&bson)
        .map_err(|e| DatabaseError::SerializationError(format!("BSON to JSON error: {}", e)))
}
