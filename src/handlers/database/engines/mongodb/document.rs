//! MongoDB Document Types
//!
//! MongoDB document, aggregation, and index types

use crate::handlers::database::types::DatabaseError;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value as JsonValue};

/// MongoDB Document
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MongoDocument {
    /// Document ID (ObjectId)
    pub id: Option<String>,
    /// Field-value map
    pub fields: Map<String, JsonValue>,
}

impl Default for MongoDocument {
    fn default() -> Self {
        Self::new()
    }
}

impl MongoDocument {
    /// Create new empty document
    pub fn new() -> Self {
        Self {
            id: None,
            fields: Map::new(),
        }
    }

    /// Set document ID
    pub fn with_id(mut self, id: String) -> Self {
        self.id = Some(id);
        self
    }

    /// Add a field
    pub fn with_field<T: Serialize>(mut self, key: &str, value: T) -> Result<Self, DatabaseError> {
        let json_value = serde_json::to_value(value)
            .map_err(|e| DatabaseError::SerializationError(e.to_string()))?;
        self.fields.insert(key.to_string(), json_value);
        Ok(self)
    }

    /// Get a field value
    pub fn get_field(&self, key: &str) -> Option<&JsonValue> {
        self.fields.get(key)
    }

    /// Convert to JSON
    pub fn to_json(&self) -> Result<JsonValue, DatabaseError> {
        let mut doc = self.fields.clone();
        if let Some(ref id) = self.id {
            doc.insert("_id".to_string(), JsonValue::String(id.clone()));
        }
        Ok(JsonValue::Object(doc))
    }

    /// Create from JSON
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

/// MongoDB Aggregation Pipeline
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AggregationPipeline {
    /// Pipeline stages
    pub stages: Vec<AggregationStage>,
}

impl Default for AggregationPipeline {
    fn default() -> Self {
        Self::new()
    }
}

impl AggregationPipeline {
    /// Create new pipeline
    pub fn new() -> Self {
        Self { stages: Vec::new() }
    }

    /// Add match stage
    pub fn match_stage(mut self, filter: JsonValue) -> Self {
        self.stages.push(AggregationStage::Match(filter));
        self
    }

    /// Add group stage
    pub fn group_stage(mut self, group_spec: JsonValue) -> Self {
        self.stages.push(AggregationStage::Group(group_spec));
        self
    }

    /// Add sort stage
    pub fn sort_stage(mut self, sort_spec: JsonValue) -> Self {
        self.stages.push(AggregationStage::Sort(sort_spec));
        self
    }

    /// Add limit stage
    pub fn limit_stage(mut self, limit: i64) -> Self {
        self.stages.push(AggregationStage::Limit(limit));
        self
    }

    /// Add skip stage
    pub fn skip_stage(mut self, skip: i64) -> Self {
        self.stages.push(AggregationStage::Skip(skip));
        self
    }
}

/// Aggregation Pipeline Stage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AggregationStage {
    /// $match - Filter documents
    Match(JsonValue),
    /// $group - Group by field
    Group(JsonValue),
    /// $sort - Sort results
    Sort(JsonValue),
    /// $limit - Limit results
    Limit(i64),
    /// $skip - Skip documents
    Skip(i64),
    /// $project - Project fields
    Project(JsonValue),
    /// $lookup - Join collections
    Lookup(JsonValue),
    /// $unwind - Unwind array
    Unwind(JsonValue),
}

/// MongoDB Index
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MongoIndex {
    /// Index name
    pub name: String,
    /// Index keys
    pub keys: JsonValue,
    /// Index options
    pub options: MongoIndexOptions,
}

/// MongoDB Index Options
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MongoIndexOptions {
    /// Unique constraint
    pub unique: bool,
    /// Sparse index
    pub sparse: bool,
    /// TTL in seconds
    pub expire_after_seconds: Option<i64>,
    /// Partial filter expression
    pub partial_filter_expression: Option<JsonValue>,
    /// Background creation
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

/// MongoDB Operation Result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MongoResult {
    /// Success flag
    pub success: bool,
    /// Number of affected documents
    pub affected_count: i64,
    /// Inserted IDs
    pub inserted_ids: Vec<String>,
    /// Number of modified documents
    pub modified_count: i64,
    /// Number of deleted documents
    pub deleted_count: i64,
    /// Error message if any
    pub error_message: Option<String>,
}

/// MongoDB Statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MongoStats {
    /// Database size in bytes
    pub database_size_bytes: i64,
    /// Number of collections
    pub collection_count: i32,
    /// Number of indexes
    pub index_count: i32,
    /// Number of documents
    pub document_count: i64,
    /// Average document size
    pub avg_document_size: f64,
    /// Storage engine
    pub storage_engine: String,
}
