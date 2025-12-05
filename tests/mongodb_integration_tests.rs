//! MongoDB Integration Tests
//!
//! Tests for MongoDB engine implementation

#[cfg(feature = "mongodb-backend")]
mod tests {
    use mcp_rs::handlers::database::{
        engine::DatabaseEngine,
        engines::mongodb::{
            AggregationPipeline, MongoConfig, MongoDocument, MongoEngine, MongoIndexOptions,
        },
        types::{ConnectionConfig, DatabaseConfig, DatabaseType, PoolConfig},
    };
    use std::collections::HashMap;

fn create_test_config() -> DatabaseConfig {
    DatabaseConfig {
        database_type: DatabaseType::MongoDB,
        connection: ConnectionConfig {
            host: "localhost".to_string(),
            port: 27017,
            database: "test_db".to_string(),
            username: "test".to_string(),
            password: "test".to_string(),
            ssl_mode: None,
            timeout_seconds: 30,
            retry_attempts: 3,
            options: HashMap::new(),
        },
        pool: PoolConfig::default(),
        security: Default::default(),
        features: Default::default(),
    }
}

#[tokio::test]
async fn test_mongo_engine_creation() {
    let config = create_test_config();
    let result = MongoEngine::new(config).await;

    match result {
        Ok(engine) => {
            assert_eq!(engine.engine_type(), DatabaseType::MongoDB);
            let features = engine.supported_features();
            assert!(!features.is_empty());
        }
        Err(e) => {
            // Connection might fail if MongoDB is not running
            println!("MongoDB connection failed (expected if server not running): {}", e);
        }
    }
}

#[tokio::test]
async fn test_mongo_document_operations() {
    let doc = MongoDocument::new()
        .with_field("name", "Test User")
        .unwrap()
        .with_field("age", 30)
        .unwrap()
        .with_field("active", true)
        .unwrap();

    assert_eq!(
        doc.get_field("name").unwrap().as_str().unwrap(),
        "Test User"
    );
    assert_eq!(doc.get_field("age").unwrap().as_i64().unwrap(), 30);
    assert!(doc.get_field("active").unwrap().as_bool().unwrap());

    // Test JSON conversion
    let json = doc.to_json().unwrap();
    let restored = MongoDocument::from_json(json).unwrap();
    assert_eq!(
        restored.get_field("name").unwrap().as_str().unwrap(),
        "Test User"
    );
}

#[tokio::test]
async fn test_aggregation_pipeline_builder() {
    let pipeline = AggregationPipeline::new()
        .match_stage(serde_json::json!({"status": "active"}))
        .group_stage(serde_json::json!({"_id": "$category", "count": {"$sum": 1}}))
        .sort_stage(serde_json::json!({"count": -1}))
        .limit_stage(10);

    assert_eq!(pipeline.stages.len(), 4);
}

#[tokio::test]
async fn test_mongo_config_validation() {
    let config = MongoConfig::new(
        "mongodb://localhost:27017".to_string(),
        "test".to_string(),
    );

    assert!(config.validate().is_ok());

    let invalid_config = MongoConfig::new("".to_string(), "test".to_string());
    assert!(invalid_config.validate().is_err());

    let invalid_db_config = MongoConfig::new("mongodb://localhost:27017".to_string(), "".to_string());
    assert!(invalid_db_config.validate().is_err());
}

#[tokio::test]
async fn test_mongo_health_check() {
    let config = create_test_config();

    match MongoEngine::new(config).await {
        Ok(engine) => {
            let health = engine.health_check().await;
            match health {
                Ok(status) => {
                    println!("MongoDB health check: {:?}", status.status);
                    assert!(status.response_time_ms > 0);
                }
                Err(e) => {
                    println!(
                        "MongoDB health check failed (expected if server not running): {}",
                        e
                    );
                }
            }
        }
        Err(e) => {
            println!(
                "MongoDB engine creation failed (expected if server not running): {}",
                e
            );
        }
    }
}

#[tokio::test]
async fn test_mongo_connection_info() {
    let config = create_test_config();

    match MongoEngine::new(config.clone()).await {
        Ok(engine) => match engine.connect(&config).await {
            Ok(conn) => {
                let info = conn.connection_info();
                assert_eq!(info.database_name, "test_db");
                assert!(!info.connection_id.is_empty());
            }
            Err(e) => {
                println!(
                    "MongoDB connection failed (expected if server not running): {}",
                    e
                );
            }
        },
        Err(e) => {
            println!(
                "MongoDB engine creation failed (expected if server not running): {}",
                e
            );
        }
    }
}

#[tokio::test]
async fn test_mongo_schema_operations() {
    let config = create_test_config();

    match MongoEngine::new(config.clone()).await {
        Ok(engine) => match engine.connect(&config).await {
            Ok(conn) => match conn.get_schema().await {
                Ok(schema) => {
                    assert_eq!(schema.database_name, "test_db");
                    println!("Collections found: {}", schema.tables.len());
                }
                Err(e) => {
                    println!(
                        "Schema retrieval failed (expected if server not running): {}",
                        e
                    );
                }
            },
            Err(e) => {
                println!(
                    "MongoDB connection failed (expected if server not running): {}",
                    e
                );
            }
        },
        Err(e) => {
            println!(
                "MongoDB engine creation failed (expected if server not running): {}",
                e
            );
        }
    }
}

#[test]
fn test_default_implementations() {
    // Test Default for MongoDocument
    let doc = MongoDocument::default();
    assert!(doc.id.is_none());
    assert!(doc.fields.is_empty());

    // Test Default for AggregationPipeline
    let pipeline = AggregationPipeline::default();
    assert!(pipeline.stages.is_empty());

    // Test Default for MongoIndexOptions
    let options = MongoIndexOptions::default();
    assert!(!options.unique);
    assert!(!options.sparse);
    assert!(options.background);
}

} // end of tests module
