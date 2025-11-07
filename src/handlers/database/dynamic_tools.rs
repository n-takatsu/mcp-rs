//! MCP Tools for Dynamic Database Switching
//!
//! 動的データベース切り替え機能のMCPツールインターフェース

use super::dynamic_engine::{
    DynamicEngineManager, EngineInfo, EngineMetrics, SwitchEvent, SwitchPolicy, SwitchResult,
    SwitchStrategy, TriggerCondition,
};
use super::types::{DatabaseError, DatabaseType};
use crate::mcp::{McpError, Tool, ToolCallParams};
use async_trait::async_trait;
use serde_json::{json, Value as JsonValue};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, error, info};

/// 動的データベース切り替えMCPツール
pub struct DynamicDatabaseTools {
    dynamic_manager: Arc<DynamicEngineManager>,
}

impl DynamicDatabaseTools {
    /// 新しいDynamicDatabaseToolsを作成
    pub fn new(dynamic_manager: Arc<DynamicEngineManager>) -> Self {
        Self { dynamic_manager }
    }

    /// 動的切り替えツール一覧を取得
    pub fn get_tools() -> Vec<Tool> {
        vec![
            Tool {
                name: "switch_database_engine".to_string(),
                description: "Dynamically switch the active database engine with zero downtime"
                    .to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "target_engine": {
                            "type": "string",
                            "enum": ["postgresql", "mysql", "redis", "mongodb", "sqlite"],
                            "description": "Target database engine to switch to"
                        },
                        "strategy": {
                            "type": "string",
                            "enum": ["graceful", "immediate", "rolling", "canary"],
                            "default": "graceful",
                            "description": "Switch strategy to use"
                        },
                        "force": {
                            "type": "boolean",
                            "default": false,
                            "description": "Force switch even if unsafe conditions detected"
                        },
                        "drain_timeout_seconds": {
                            "type": "integer",
                            "minimum": 0,
                            "maximum": 3600,
                            "default": 30,
                            "description": "Time to wait for existing connections to drain"
                        }
                    },
                    "required": ["target_engine"]
                }),
            },
            Tool {
                name: "list_available_engines".to_string(),
                description: "List all available database engines and their current status"
                    .to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {},
                    "additionalProperties": false
                }),
            },
            Tool {
                name: "get_engine_metrics".to_string(),
                description: "Get real-time performance metrics for a specific database engine"
                    .to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "engine_id": {
                            "type": "string",
                            "description": "ID of the engine to get metrics for"
                        }
                    },
                    "required": ["engine_id"]
                }),
            },
            Tool {
                name: "configure_switch_policy".to_string(),
                description:
                    "Configure automatic switching policies based on performance thresholds"
                        .to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "policy_name": {
                            "type": "string",
                            "description": "Name of the switch policy"
                        },
                        "trigger_type": {
                            "type": "string",
                            "enum": ["performance", "load", "error_rate", "manual", "scheduled"],
                            "description": "Type of trigger condition"
                        },
                        "target_engine": {
                            "type": "string",
                            "description": "Engine to switch to when policy triggers"
                        },
                        "strategy": {
                            "type": "string",
                            "enum": ["graceful", "immediate", "rolling", "canary"],
                            "default": "graceful"
                        },
                        "priority": {
                            "type": "integer",
                            "minimum": 1,
                            "maximum": 10,
                            "default": 5,
                            "description": "Policy priority (1=highest, 10=lowest)"
                        },
                        "enabled": {
                            "type": "boolean",
                            "default": true,
                            "description": "Whether the policy is enabled"
                        },
                        "threshold_config": {
                            "type": "object",
                            "description": "Threshold configuration based on trigger type",
                            "properties": {
                                "response_time_ms": { "type": "integer", "minimum": 1 },
                                "cpu_threshold": { "type": "integer", "minimum": 1, "maximum": 100 },
                                "memory_threshold": { "type": "integer", "minimum": 1, "maximum": 100 },
                                "error_rate_threshold": { "type": "number", "minimum": 0.0, "maximum": 1.0 },
                                "window_duration_seconds": { "type": "integer", "minimum": 1 }
                            }
                        }
                    },
                    "required": ["policy_name", "trigger_type", "target_engine"]
                }),
            },
            Tool {
                name: "trigger_manual_switch".to_string(),
                description: "Manually trigger a database engine switch immediately".to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "target_engine": {
                            "type": "string",
                            "enum": ["postgresql", "mysql", "redis", "mongodb", "sqlite"],
                            "description": "Target database engine to switch to"
                        },
                        "reason": {
                            "type": "string",
                            "description": "Reason for manual switch"
                        }
                    },
                    "required": ["target_engine", "reason"]
                }),
            },
            Tool {
                name: "get_switch_history".to_string(),
                description: "Get the history of database engine switches".to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "limit": {
                            "type": "integer",
                            "minimum": 1,
                            "maximum": 100,
                            "default": 20,
                            "description": "Maximum number of history entries to return"
                        }
                    }
                }),
            },
            Tool {
                name: "cancel_pending_switch".to_string(),
                description: "Cancel a pending database engine switch operation".to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "switch_id": {
                            "type": "string",
                            "description": "ID of the switch operation to cancel"
                        }
                    },
                    "required": ["switch_id"]
                }),
            },
            Tool {
                name: "validate_switch_readiness".to_string(),
                description: "Validate if a target engine is ready for switching".to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "target_engine": {
                            "type": "string",
                            "enum": ["postgresql", "mysql", "redis", "mongodb", "sqlite"],
                            "description": "Target database engine to validate"
                        }
                    },
                    "required": ["target_engine"]
                }),
            },
            Tool {
                name: "get_current_active_engine".to_string(),
                description: "Get information about the currently active database engine"
                    .to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {},
                    "additionalProperties": false
                }),
            },
        ]
    }

    /// ツール呼び出しの処理
    pub async fn handle_tool_call(
        &self,
        tool_name: &str,
        params: &ToolCallParams,
    ) -> Result<JsonValue, McpError> {
        debug!("Handling dynamic database tool call: {}", tool_name);

        match tool_name {
            "switch_database_engine" => self.handle_switch_engine(params).await,
            "list_available_engines" => self.handle_list_engines().await,
            "get_engine_metrics" => self.handle_get_metrics(params).await,
            "configure_switch_policy" => self.handle_configure_policy(params).await,
            "trigger_manual_switch" => self.handle_manual_switch(params).await,
            "get_switch_history" => self.handle_get_history(params).await,
            "cancel_pending_switch" => self.handle_cancel_switch(params).await,
            "validate_switch_readiness" => self.handle_validate_readiness(params).await,
            "get_current_active_engine" => self.handle_get_active_engine().await,
            _ => Err(McpError::InvalidRequest(format!(
                "Unknown dynamic database tool: {}",
                tool_name
            ))),
        }
    }

    /// データベースエンジン切り替え
    async fn handle_switch_engine(&self, params: &ToolCallParams) -> Result<JsonValue, McpError> {
        let args = params
            .arguments
            .as_ref()
            .ok_or_else(|| McpError::InvalidRequest("Missing arguments".to_string()))?;

        let target_engine = args
            .get("target_engine")
            .and_then(|v| v.as_str())
            .ok_or_else(|| McpError::InvalidRequest("target_engine is required".to_string()))?;

        let strategy_str = args
            .get("strategy")
            .and_then(|v| v.as_str())
            .unwrap_or("graceful");

        let force = args.get("force").and_then(|v| v.as_bool()).unwrap_or(false);

        let drain_timeout_seconds = args
            .get("drain_timeout_seconds")
            .and_then(|v| v.as_u64())
            .unwrap_or(30);

        // 戦略の構築
        let strategy = match strategy_str {
            "graceful" => SwitchStrategy::Graceful {
                drain_timeout: Duration::from_secs(drain_timeout_seconds),
                max_pending_transactions: 1000,
            },
            "immediate" => SwitchStrategy::Immediate {
                force_transaction_abort: force,
            },
            _ => {
                return Err(McpError::InvalidRequest(format!(
                    "Unsupported switch strategy: {}",
                    strategy_str
                )));
            }
        };

        // 切り替え実行
        match self
            .dynamic_manager
            .switch_to_engine(target_engine, strategy)
            .await
        {
            Ok(result) => {
                info!("Database engine switch successful: {}", target_engine);
                Ok(json!({
                    "success": true,
                    "target_engine": target_engine,
                    "switch_result": {
                        "success": result.success,
                        "switch_duration_ms": result.switch_duration_ms,
                        "affected_transactions": result.affected_transactions,
                        "downtime_ms": result.downtime_ms,
                        "message": result.message
                    }
                }))
            }
            Err(e) => {
                error!("Database engine switch failed: {}", e);
                Err(McpError::Internal(format!("Switch failed: {}", e)))
            }
        }
    }

    /// 利用可能エンジン一覧
    async fn handle_list_engines(&self) -> Result<JsonValue, McpError> {
        let engines = self.dynamic_manager.list_available_engines().await;
        let active_engine = self.dynamic_manager.get_active_engine().await;

        Ok(json!({
            "engines": engines.iter().map(|engine| {
                json!({
                    "id": engine.id,
                    "type": engine.engine_type,
                    "role": engine.role,
                    "status": engine.status,
                    "added_at": engine.added_at,
                    "is_active": active_engine.as_ref()
                        .map(|(id, _)| id == &engine.id)
                        .unwrap_or(false)
                })
            }).collect::<Vec<_>>(),
            "active_engine": active_engine.map(|(id, _)| id)
        }))
    }

    /// エンジンメトリクス取得
    async fn handle_get_metrics(&self, params: &ToolCallParams) -> Result<JsonValue, McpError> {
        let args = params
            .arguments
            .as_ref()
            .ok_or_else(|| McpError::InvalidRequest("Missing arguments".to_string()))?;

        let engine_id = args
            .get("engine_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| McpError::InvalidRequest("engine_id is required".to_string()))?;

        match self.dynamic_manager.get_engine_metrics(engine_id).await {
            Some(metrics) => Ok(json!({
                "engine_id": engine_id,
                "metrics": {
                    "response_time_ms": metrics.response_time_ms,
                    "cpu_usage_percent": metrics.cpu_usage_percent,
                    "memory_usage_percent": metrics.memory_usage_percent,
                    "active_connections": metrics.active_connections,
                    "query_rate_per_second": metrics.query_rate_per_second,
                    "error_rate_percent": metrics.error_rate_percent,
                    "availability_percent": metrics.availability_percent,
                    "last_updated": metrics.last_updated
                }
            })),
            None => Err(McpError::InvalidRequest(format!(
                "Engine not found or no metrics available: {}",
                engine_id
            ))),
        }
    }

    /// 切り替えポリシー設定
    async fn handle_configure_policy(
        &self,
        params: &ToolCallParams,
    ) -> Result<JsonValue, McpError> {
        let args = params
            .arguments
            .as_ref()
            .ok_or_else(|| McpError::InvalidRequest("Missing arguments".to_string()))?;

        let policy_name = args
            .get("policy_name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| McpError::InvalidRequest("policy_name is required".to_string()))?;

        let trigger_type = args
            .get("trigger_type")
            .and_then(|v| v.as_str())
            .ok_or_else(|| McpError::InvalidRequest("trigger_type is required".to_string()))?;

        let target_engine = args
            .get("target_engine")
            .and_then(|v| v.as_str())
            .ok_or_else(|| McpError::InvalidRequest("target_engine is required".to_string()))?;

        let strategy_str = args
            .get("strategy")
            .and_then(|v| v.as_str())
            .unwrap_or("graceful");

        let priority = args.get("priority").and_then(|v| v.as_u64()).unwrap_or(5) as u8;

        let enabled = args
            .get("enabled")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        // トリガー条件の構築
        let trigger = match trigger_type {
            "performance" => {
                let threshold_config = args.get("threshold_config");
                let response_time_ms = threshold_config
                    .and_then(|c| c.get("response_time_ms"))
                    .and_then(|v| v.as_u64())
                    .unwrap_or(1000);
                let window_duration_seconds = threshold_config
                    .and_then(|c| c.get("window_duration_seconds"))
                    .and_then(|v| v.as_u64())
                    .unwrap_or(60);

                TriggerCondition::PerformanceDegradation {
                    response_time_threshold_ms: response_time_ms,
                    window_duration: Duration::from_secs(window_duration_seconds),
                }
            }
            "load" => {
                let threshold_config = args.get("threshold_config");
                let cpu_threshold = threshold_config
                    .and_then(|c| c.get("cpu_threshold"))
                    .and_then(|v| v.as_u64())
                    .unwrap_or(80) as u8;
                let memory_threshold = threshold_config
                    .and_then(|c| c.get("memory_threshold"))
                    .and_then(|v| v.as_u64())
                    .unwrap_or(80) as u8;

                TriggerCondition::LoadThreshold {
                    cpu_threshold,
                    memory_threshold,
                    connection_threshold: 90,
                }
            }
            "manual" => TriggerCondition::Manual,
            _ => {
                return Err(McpError::InvalidRequest(format!(
                    "Unsupported trigger type: {}",
                    trigger_type
                )));
            }
        };

        // 戦略の構築
        let strategy = match strategy_str {
            "graceful" => SwitchStrategy::Graceful {
                drain_timeout: Duration::from_secs(30),
                max_pending_transactions: 1000,
            },
            "immediate" => SwitchStrategy::Immediate {
                force_transaction_abort: false,
            },
            _ => {
                return Err(McpError::InvalidRequest(format!(
                    "Unsupported switch strategy: {}",
                    strategy_str
                )));
            }
        };

        // ポリシーの作成と追加
        let policy = SwitchPolicy {
            name: policy_name.to_string(),
            trigger,
            target_engine: target_engine.to_string(),
            strategy,
            priority,
            enabled,
        };

        match self.dynamic_manager.add_switch_policy(policy).await {
            Ok(()) => Ok(json!({
                "success": true,
                "message": format!("Switch policy '{}' configured successfully", policy_name)
            })),
            Err(e) => Err(McpError::Internal(format!(
                "Failed to configure policy: {}",
                e
            ))),
        }
    }

    /// 手動切り替えトリガー
    async fn handle_manual_switch(&self, params: &ToolCallParams) -> Result<JsonValue, McpError> {
        let args = params
            .arguments
            .as_ref()
            .ok_or_else(|| McpError::InvalidRequest("Missing arguments".to_string()))?;

        let target_engine = args
            .get("target_engine")
            .and_then(|v| v.as_str())
            .ok_or_else(|| McpError::InvalidRequest("target_engine is required".to_string()))?;

        let reason = args
            .get("reason")
            .and_then(|v| v.as_str())
            .ok_or_else(|| McpError::InvalidRequest("reason is required".to_string()))?;

        info!(
            "Manual switch triggered: {} -> {}, reason: {}",
            "current", target_engine, reason
        );

        // 即座切り替え戦略で実行
        let strategy = SwitchStrategy::Immediate {
            force_transaction_abort: false,
        };

        match self
            .dynamic_manager
            .switch_to_engine(target_engine, strategy)
            .await
        {
            Ok(result) => Ok(json!({
                "success": true,
                "target_engine": target_engine,
                "reason": reason,
                "switch_result": result
            })),
            Err(e) => Err(McpError::Internal(format!("Manual switch failed: {}", e))),
        }
    }

    /// 切り替え履歴取得
    async fn handle_get_history(&self, params: &ToolCallParams) -> Result<JsonValue, McpError> {
        let empty_args = HashMap::new();
        let args = params.arguments.as_ref().unwrap_or(&empty_args);

        let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(20) as usize;

        let history = self.dynamic_manager.get_switch_history(limit).await;

        Ok(json!({
            "history": history.iter().map(|event| {
                json!({
                    "id": event.id,
                    "target_engine": event.target_engine,
                    "strategy": event.strategy,
                    "start_time": event.start_time,
                    "end_time": event.end_time,
                    "success": event.success,
                    "result": event.result
                })
            }).collect::<Vec<_>>()
        }))
    }

    /// 進行中の切り替えをキャンセル
    async fn handle_cancel_switch(&self, _params: &ToolCallParams) -> Result<JsonValue, McpError> {
        // TODO: 切り替えキャンセル機能の実装
        Err(McpError::Internal(
            "Switch cancellation not yet implemented".to_string(),
        ))
    }

    /// 切り替え準備状況の検証
    async fn handle_validate_readiness(
        &self,
        params: &ToolCallParams,
    ) -> Result<JsonValue, McpError> {
        let args = params
            .arguments
            .as_ref()
            .ok_or_else(|| McpError::InvalidRequest("Missing arguments".to_string()))?;

        let target_engine = args
            .get("target_engine")
            .and_then(|v| v.as_str())
            .ok_or_else(|| McpError::InvalidRequest("target_engine is required".to_string()))?;

        // TODO: より詳細な準備状況チェック
        let engines = self.dynamic_manager.list_available_engines().await;
        let target_exists = engines.iter().any(|e| e.id == target_engine);

        if !target_exists {
            return Ok(json!({
                "ready": false,
                "reason": "Target engine not found",
                "checks": {
                    "engine_exists": false,
                    "engine_healthy": false,
                    "connections_available": false
                }
            }));
        }

        // 基本的な健全性チェック
        let metrics = self.dynamic_manager.get_engine_metrics(target_engine).await;
        let is_healthy = metrics
            .as_ref()
            .map(|m| m.availability_percent >= 95.0)
            .unwrap_or(false);

        Ok(json!({
            "ready": is_healthy,
            "target_engine": target_engine,
            "checks": {
                "engine_exists": true,
                "engine_healthy": is_healthy,
                "connections_available": true // TODO: 実際のコネクション数チェック
            },
            "metrics": metrics
        }))
    }

    /// 現在のアクティブエンジン取得
    async fn handle_get_active_engine(&self) -> Result<JsonValue, McpError> {
        match self.dynamic_manager.get_active_engine().await {
            Some((engine_id, engine)) => {
                let metrics = self.dynamic_manager.get_engine_metrics(&engine_id).await;

                Ok(json!({
                    "active_engine": {
                        "id": engine_id,
                        "type": engine.engine_type(),
                        "metrics": metrics
                    }
                }))
            }
            None => Ok(json!({
                "active_engine": null,
                "message": "No active engine configured"
            })),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handlers::database::pool::PoolManager;

    #[tokio::test]
    async fn test_dynamic_tools_creation() {
        let pool_manager = Arc::new(PoolManager::new());
        let dynamic_manager = Arc::new(DynamicEngineManager::new(pool_manager).await.unwrap());
        let _tools = DynamicDatabaseTools::new(dynamic_manager);

        // ツール作成が成功することを確認
        assert!(!DynamicDatabaseTools::get_tools().is_empty());
    }

    #[test]
    fn test_tool_definitions() {
        let tools = DynamicDatabaseTools::get_tools();

        // 期待されるツール数
        assert_eq!(tools.len(), 9);

        // 主要ツールの存在確認
        let tool_names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();
        assert!(tool_names.contains(&"switch_database_engine"));
        assert!(tool_names.contains(&"list_available_engines"));
        assert!(tool_names.contains(&"get_engine_metrics"));
        assert!(tool_names.contains(&"configure_switch_policy"));
    }

    #[tokio::test]
    async fn test_list_engines_empty() {
        let pool_manager = Arc::new(PoolManager::new());
        let dynamic_manager = Arc::new(DynamicEngineManager::new(pool_manager).await.unwrap());
        let tools = DynamicDatabaseTools::new(dynamic_manager);

        let _params = ToolCallParams {
            name: "list_available_engines".to_string(),
            arguments: Some(HashMap::new()),
        };

        let result = tools.handle_list_engines().await.unwrap();
        let engines = result.get("engines").unwrap().as_array().unwrap();
        assert!(engines.is_empty());
    }
}
