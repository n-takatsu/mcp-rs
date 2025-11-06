//! Database MCP Handler Implementation
//!
//! データベース操作のためのMCPハンドラー実装

use crate::handlers::database::{
    engine::{DatabaseEngine, DatabaseEngineBuilder, EngineRegistry},
    pool::{ConnectionPool, PoolManager},
    security::DatabaseSecurity,
    types::{
        DatabaseConfig, DatabaseError, DatabaseType, ExecuteResult, QueryContext, QueryResult,
        QueryType, Value,
    },
};
use crate::mcp::{
    InitializeParams, McpError, McpHandler, Resource, ResourceReadParams, Tool, ToolCallParams,
};
// use crate::threat_intelligence::ThreatDetectionEngine;
use crate::handlers::database::security::ThreatDetectionEngine;
use async_trait::async_trait;
use base64::prelude::*;
use serde_json::{json, Value as JsonValue};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// データベースMCPハンドラー
pub struct DatabaseHandler {
    /// エンジンレジストリ
    engines: Arc<RwLock<EngineRegistry>>,
    /// 接続プールマネージャー
    pool_manager: Arc<PoolManager>,
    /// アクティブなエンジンID
    active_engine: Arc<RwLock<Option<String>>>,
    /// データベース設定
    configs: Arc<RwLock<HashMap<String, DatabaseConfig>>>,
    /// セキュリティレイヤー
    security: Arc<DatabaseSecurity>,
    /// 脅威インテリジェンス
    threat_intelligence: Option<Arc<ThreatDetectionEngine>>,
}

impl DatabaseHandler {
    /// 新しいデータベースハンドラーを作成
    pub async fn new(
        threat_intelligence: Option<Arc<ThreatDetectionEngine>>,
    ) -> Result<Self, DatabaseError> {
        let security = Arc::new(DatabaseSecurity::new(
            Default::default(), // デフォルトセキュリティ設定
            threat_intelligence.clone(),
        ));

        Ok(Self {
            engines: Arc::new(RwLock::new(EngineRegistry::new())),
            pool_manager: Arc::new(PoolManager::new()),
            active_engine: Arc::new(RwLock::new(None)),
            configs: Arc::new(RwLock::new(HashMap::new())),
            security,
            threat_intelligence,
        })
    }

    /// データベースエンジンを追加
    pub async fn add_database(
        &self,
        id: String,
        config: DatabaseConfig,
    ) -> Result<(), DatabaseError> {
        // エンジンを作成
        let engine = DatabaseEngineBuilder::build(&config).await?;

        // 接続プールを作成
        let _pool = self
            .pool_manager
            .create_pool(id.clone(), engine.clone(), &config)
            .await?;

        // エンジンを登録
        {
            let mut engines = self.engines.write().await;
            engines.register(engine);
        }

        // 設定を保存
        {
            let mut configs = self.configs.write().await;
            configs.insert(id.clone(), config);
        }

        // 最初のエンジンをアクティブに設定
        {
            let mut active = self.active_engine.write().await;
            if active.is_none() {
                *active = Some(id);
            }
        }

        Ok(())
    }

    /// アクティブエンジンを切り替え
    pub async fn switch_engine(&self, engine_id: &str) -> Result<(), DatabaseError> {
        let configs = self.configs.read().await;
        if !configs.contains_key(engine_id) {
            return Err(DatabaseError::ConfigurationError(format!(
                "Engine not found: {}",
                engine_id
            )));
        }

        let mut active = self.active_engine.write().await;
        *active = Some(engine_id.to_string());

        Ok(())
    }

    /// アクティブなプールを取得
    async fn get_active_pool(&self) -> Result<Arc<ConnectionPool>, DatabaseError> {
        let active_id = {
            let active = self.active_engine.read().await;
            active.clone()
        };

        let engine_id = active_id.ok_or_else(|| {
            DatabaseError::ConfigurationError("No active database engine".to_string())
        })?;

        self.pool_manager
            .get_pool(&engine_id)
            .await
            .ok_or_else(|| DatabaseError::PoolError(format!("Pool not found: {}", engine_id)))
    }

    /// クエリ実行処理
    async fn handle_execute_query(&self, args: JsonValue) -> Result<JsonValue, McpError> {
        let sql: String = args
            .get("sql")
            .and_then(|v| v.as_str())
            .ok_or_else(|| McpError::InvalidRequest("Missing 'sql' parameter".to_string()))?
            .to_string();

        let params: Vec<Value> = if let Some(params_json) = args.get("params") {
            self.parse_parameters(params_json)?
        } else {
            Vec::new()
        };

        // 接続プールから接続を取得
        let pool = self
            .get_active_pool()
            .await
            .map_err(|e| McpError::InvalidRequest(e.to_string()))?;

        let config = {
            let configs = self.configs.read().await;
            let active_id = self.active_engine.read().await;
            configs.get(active_id.as_ref().unwrap()).unwrap().clone()
        };

        let connection = pool
            .get_connection(&config)
            .await
            .map_err(|e| McpError::InvalidRequest(e.to_string()))?;

        // クエリ実行
        let result = connection
            .inner()
            .query(&sql, &params)
            .await
            .map_err(|e| McpError::InvalidRequest(e.to_string()))?;

        // 結果をJSON形式で返す
        Ok(json!({
            "columns": result.columns.iter().map(|col| json!({
                "name": col.name,
                "type": col.data_type,
                "nullable": col.nullable,
                "max_length": col.max_length
            })).collect::<Vec<_>>(),
            "rows": result.rows.iter().map(|row| {
                row.iter().map(|value| self.value_to_json(value)).collect::<Vec<_>>()
            }).collect::<Vec<_>>(),
            "total_rows": result.total_rows,
            "execution_time_ms": result.execution_time_ms
        }))
    }

    /// コマンド実行処理
    async fn handle_execute_command(&self, args: JsonValue) -> Result<JsonValue, McpError> {
        let sql: String = args
            .get("sql")
            .and_then(|v| v.as_str())
            .ok_or_else(|| McpError::InvalidRequest("Missing 'sql' parameter".to_string()))?
            .to_string();

        let params: Vec<Value> = if let Some(params_json) = args.get("params") {
            self.parse_parameters(params_json)?
        } else {
            Vec::new()
        };

        // 接続プールから接続を取得
        let pool = self
            .get_active_pool()
            .await
            .map_err(|e| McpError::Internal(e.to_string()))?;

        let config = {
            let configs = self.configs.read().await;
            let active_id = self.active_engine.read().await;
            configs.get(active_id.as_ref().unwrap()).unwrap().clone()
        };

        let connection = pool
            .get_connection(&config)
            .await
            .map_err(|e| McpError::Internal(e.to_string()))?;

        // コマンド実行
        let result = connection
            .inner()
            .execute(&sql, &params)
            .await
            .map_err(|e| McpError::ToolNotFound(e.to_string()))?;

        // 結果をJSON形式で返す
        Ok(json!({
            "rows_affected": result.rows_affected,
            "last_insert_id": result.last_insert_id.map(|v| self.value_to_json(&v)),
            "execution_time_ms": result.execution_time_ms
        }))
    }

    /// スキーマ取得処理
    async fn handle_get_schema(&self, args: JsonValue) -> Result<JsonValue, McpError> {
        let _schema_name: Option<String> = args
            .get("schema_name")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        // 接続プールから接続を取得
        let pool = self
            .get_active_pool()
            .await
            .map_err(|e| McpError::Internal(e.to_string()))?;

        let config = {
            let configs = self.configs.read().await;
            let active_id = self.active_engine.read().await;
            configs.get(active_id.as_ref().unwrap()).unwrap().clone()
        };

        let connection = pool
            .get_connection(&config)
            .await
            .map_err(|e| McpError::Internal(e.to_string()))?;

        // スキーマ取得
        let schema = connection
            .inner()
            .get_schema()
            .await
            .map_err(|e| McpError::ToolNotFound(e.to_string()))?;

        // 結果をJSON形式で返す
        Ok(json!({
            "database_name": schema.database_name,
            "tables": schema.tables.iter().map(|table| json!({
                "name": table.name,
                "schema": table.schema,
                "columns": table.columns.iter().map(|col| json!({
                    "name": col.name,
                    "type": col.data_type,
                    "nullable": col.nullable,
                    "max_length": col.max_length
                })).collect::<Vec<_>>(),
                "primary_keys": table.primary_keys,
                "foreign_keys": table.foreign_keys.iter().map(|fk| json!({
                    "name": fk.name,
                    "column": fk.column,
                    "referenced_table": fk.referenced_table,
                    "referenced_column": fk.referenced_column
                })).collect::<Vec<_>>(),
                "indexes": table.indexes.iter().map(|idx| json!({
                    "name": idx.name,
                    "columns": idx.columns,
                    "is_unique": idx.is_unique,
                    "is_primary": idx.is_primary
                })).collect::<Vec<_>>()
            })).collect::<Vec<_>>(),
            "views": schema.views.iter().map(|view| json!({
                "name": view.name,
                "schema": view.schema,
                "definition": view.definition,
                "columns": view.columns.iter().map(|col| json!({
                    "name": col.name,
                    "type": col.data_type,
                    "nullable": col.nullable
                })).collect::<Vec<_>>()
            })).collect::<Vec<_>>(),
            "procedures": schema.procedures.iter().map(|proc| json!({
                "name": proc.name,
                "schema": proc.schema,
                "parameters": proc.parameters.iter().map(|param| json!({
                    "name": param.name,
                    "type": param.data_type,
                    "direction": format!("{:?}", param.direction)
                })).collect::<Vec<_>>(),
                "return_type": proc.return_type
            })).collect::<Vec<_>>()
        }))
    }

    /// トランザクション開始処理
    async fn handle_begin_transaction(&self, args: JsonValue) -> Result<JsonValue, McpError> {
        let _isolation_level: Option<String> = args
            .get("isolation_level")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        // 接続プールから接続を取得
        let pool = self
            .get_active_pool()
            .await
            .map_err(|e| McpError::Internal(e.to_string()))?;

        let config = {
            let configs = self.configs.read().await;
            let active_id = self.active_engine.read().await;
            configs.get(active_id.as_ref().unwrap()).unwrap().clone()
        };

        let connection = pool
            .get_connection(&config)
            .await
            .map_err(|e| McpError::Internal(e.to_string()))?;

        // トランザクション開始
        let transaction = connection
            .inner()
            .begin_transaction()
            .await
            .map_err(|e| McpError::ToolNotFound(e.to_string()))?;

        let transaction_info = transaction.transaction_info();

        // トランザクション情報を返す
        Ok(json!({
            "transaction_id": transaction_info.transaction_id,
            "isolation_level": format!("{}", transaction_info.isolation_level),
            "started_at": transaction_info.started_at,
            "is_read_only": transaction_info.is_read_only
        }))
    }

    /// パラメータをパース
    fn parse_parameters(&self, params_json: &JsonValue) -> Result<Vec<Value>, McpError> {
        if let Some(array) = params_json.as_array() {
            array.iter().map(|v| self.json_to_value(v)).collect()
        } else {
            Err(McpError::InvalidRequest(
                "Parameters must be an array".to_string(),
            ))
        }
    }

    /// JSONValueをValueに変換
    fn json_to_value(&self, json_val: &JsonValue) -> Result<Value, McpError> {
        match json_val {
            JsonValue::Null => Ok(Value::Null),
            JsonValue::Bool(b) => Ok(Value::Bool(*b)),
            JsonValue::Number(n) => {
                if let Some(i) = n.as_i64() {
                    Ok(Value::Int(i))
                } else if let Some(f) = n.as_f64() {
                    Ok(Value::Float(f))
                } else {
                    Err(McpError::InvalidRequest(
                        "Invalid number format".to_string(),
                    ))
                }
            }
            JsonValue::String(s) => Ok(Value::String(s.clone())),
            JsonValue::Array(_) | JsonValue::Object(_) => Ok(Value::Json(json_val.clone())),
        }
    }

    /// ValueをJSONValueに変換
    fn value_to_json(&self, value: &Value) -> JsonValue {
        match value {
            Value::Null => JsonValue::Null,
            Value::Bool(b) => JsonValue::Bool(*b),
            Value::Int(i) => JsonValue::Number((*i).into()),
            Value::Float(f) => {
                JsonValue::Number(serde_json::Number::from_f64(*f).unwrap_or_else(|| 0.into()))
            }
            Value::String(s) => JsonValue::String(s.clone()),
            Value::Binary(bytes) => {
                JsonValue::String(base64::prelude::BASE64_STANDARD.encode(bytes))
            }
            Value::Json(j) => j.clone(),
            Value::DateTime(dt) => JsonValue::String(dt.to_rfc3339()),
        }
    }
}

#[async_trait]
impl McpHandler for DatabaseHandler {
    async fn initialize(&self, _params: InitializeParams) -> Result<JsonValue, McpError> {
        Ok(json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {
                "tools": {}
            },
            "serverInfo": {
                "name": "database-handler",
                "version": "1.0.0"
            }
        }))
    }

    async fn list_tools(&self) -> Result<Vec<Tool>, McpError> {
        Ok(vec![
            Tool {
                name: "execute_query".to_string(),
                description: "Execute SELECT query and return results".to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "sql": {
                            "type": "string",
                            "description": "SQL query to execute (SELECT statements only)"
                        },
                        "params": {
                            "type": "array",
                            "description": "Query parameters for prepared statements",
                            "items": {
                                "oneOf": [
                                    {"type": "null"},
                                    {"type": "boolean"},
                                    {"type": "number"},
                                    {"type": "string"}
                                ]
                            }
                        },
                        "engine": {
                            "type": "string",
                            "description": "Database engine to use (optional, uses active engine if not specified)"
                        }
                    },
                    "required": ["sql"]
                }),
            },
            Tool {
                name: "execute_command".to_string(),
                description: "Execute INSERT/UPDATE/DELETE command".to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "sql": {
                            "type": "string",
                            "description": "SQL command to execute (INSERT/UPDATE/DELETE statements)"
                        },
                        "params": {
                            "type": "array",
                            "description": "Command parameters for prepared statements",
                            "items": {
                                "oneOf": [
                                    {"type": "null"},
                                    {"type": "boolean"},
                                    {"type": "number"},
                                    {"type": "string"}
                                ]
                            }
                        },
                        "engine": {
                            "type": "string",
                            "description": "Database engine to use (optional)"
                        }
                    },
                    "required": ["sql"]
                }),
            },
            Tool {
                name: "get_schema".to_string(),
                description: "Get database schema information".to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "schema_name": {
                            "type": "string",
                            "description": "Specific schema name (optional, returns all schemas if not specified)"
                        },
                        "engine": {
                            "type": "string",
                            "description": "Database engine to use (optional)"
                        }
                    }
                }),
            },
            Tool {
                name: "begin_transaction".to_string(),
                description: "Begin database transaction".to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "isolation_level": {
                            "type": "string",
                            "enum": ["READ_UNCOMMITTED", "READ_COMMITTED", "REPEATABLE_READ", "SERIALIZABLE"],
                            "description": "Transaction isolation level (optional, defaults to READ_COMMITTED)"
                        },
                        "engine": {
                            "type": "string",
                            "description": "Database engine to use (optional)"
                        }
                    }
                }),
            },
            Tool {
                name: "list_engines".to_string(),
                description: "List available database engines and their status".to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {}
                }),
            },
            Tool {
                name: "switch_engine".to_string(),
                description: "Switch active database engine".to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "engine_id": {
                            "type": "string",
                            "description": "ID of the engine to switch to"
                        }
                    },
                    "required": ["engine_id"]
                }),
            },
        ])
    }

    async fn call_tool(&self, params: ToolCallParams) -> Result<JsonValue, McpError> {
        let args = params.arguments.clone().unwrap_or_default();
        let args_json =
            serde_json::to_value(&args).map_err(|e| McpError::InvalidParams(e.to_string()))?;

        match params.name.as_str() {
            "execute_query" => self.handle_execute_query(args_json).await,
            "execute_command" => self.handle_execute_command(args_json).await,
            "get_schema" => self.handle_get_schema(args_json).await,
            "begin_transaction" => self.handle_begin_transaction(args_json).await,
            "list_engines" => {
                let engines = self.engines.read().await;
                let active_id = self.active_engine.read().await;

                Ok(json!({
                    "engines": engines.available_types().iter().map(|t| json!({
                        "type": format!("{}", t),
                        "is_active": active_id.as_ref().is_some_and(|id| id.contains(&format!("{}", t)))
                    })).collect::<Vec<_>>(),
                    "active_engine": active_id.clone()
                }))
            }
            "switch_engine" => {
                let args = params.arguments.clone().unwrap_or_default();
                let engine_id: String = args
                    .get("engine_id")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| {
                        McpError::InvalidRequest("Missing 'engine_id' parameter".to_string())
                    })?
                    .to_string();

                self.switch_engine(&engine_id)
                    .await
                    .map_err(|e| McpError::ToolNotFound(e.to_string()))?;

                Ok(json!({
                    "success": true,
                    "active_engine": engine_id
                }))
            }
            _ => Err(McpError::InvalidRequest(format!(
                "Unknown tool: {}",
                params.name
            ))),
        }
    }

    async fn list_resources(&self) -> Result<Vec<Resource>, McpError> {
        // データベーススキーマをリソースとして公開
        Ok(vec![Resource {
            uri: "db://schema".to_string(),
            name: "Database Schema".to_string(),
            description: Some("Complete database schema information".to_string()),
            mime_type: Some("application/json".to_string()),
        }])
    }

    async fn read_resource(&self, params: ResourceReadParams) -> Result<JsonValue, McpError> {
        match params.uri.as_str() {
            "db://schema" => self.handle_get_schema(json!({})).await,
            _ => Err(McpError::InvalidRequest(format!(
                "Unknown resource: {}",
                params.uri
            ))),
        }
    }
}
