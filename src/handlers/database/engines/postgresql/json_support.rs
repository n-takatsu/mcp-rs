//! PostgreSQL JSON/JSONB Support
//!
//! Provides PostgreSQL-specific JSON and JSONB data type handling
//! with query operators and manipulation functions.

use crate::handlers::database::types::{DatabaseError, Value};
use serde_json::{json, Value as JsonValue};

/// PostgreSQL JSON Support
///
/// Handles JSON and JSONB operations specific to PostgreSQL
pub struct PostgreSqlJsonSupport;

impl PostgreSqlJsonSupport {
    /// Create a JSON value from Rust value
    pub fn to_json(&self, value: &Value) -> Result<JsonValue, DatabaseError> {
        match value {
            Value::Null => Ok(JsonValue::Null),
            Value::Bool(b) => Ok(JsonValue::Bool(*b)),
            Value::Int(i) => Ok(json!(i)),
            Value::Float(f) => Ok(json!(f)),
            Value::String(s) => Ok(json!(s)),
            _ => Err(DatabaseError::ValidationError(
                "Unsupported value type for JSON conversion".to_string(),
            )),
        }
    }

    /// Create a Value from JSON
    pub fn from_json(&self, json: &JsonValue) -> Result<Value, DatabaseError> {
        match json {
            JsonValue::Null => Ok(Value::Null),
            JsonValue::Bool(b) => Ok(Value::Bool(*b)),
            JsonValue::Number(n) => {
                if let Some(i) = n.as_i64() {
                    Ok(Value::Int(i))
                } else if let Some(f) = n.as_f64() {
                    Ok(Value::Float(f))
                } else {
                    Err(DatabaseError::ValidationError(
                        "Invalid number in JSON".to_string(),
                    ))
                }
            }
            JsonValue::String(s) => Ok(Value::String(s.clone())),
            JsonValue::Array(_) | JsonValue::Object(_) => {
                // For complex JSON, store as string
                Ok(Value::String(json.to_string()))
            }
        }
    }

    /// Check if JSON contains key (PostgreSQL @> operator)
    pub fn contains(&self, json: &JsonValue, key: &str) -> bool {
        match json {
            JsonValue::Object(obj) => obj.contains_key(key),
            _ => false,
        }
    }

    /// Check if JSON is contained in another (PostgreSQL <@ operator)
    pub fn is_contained_in(&self, json: &JsonValue, container: &JsonValue) -> bool {
        // Simplified implementation
        json == container
    }

    /// Get JSON value at path (PostgreSQL -> operator)
    pub fn get_path(&self, json: &JsonValue, path: &str) -> Result<JsonValue, DatabaseError> {
        let keys: Vec<&str> = path.split('.').collect();
        let mut current = json.clone();

        for key in keys {
            if key.is_empty() {
                continue;
            }

            // Try as object key
            if let JsonValue::Object(obj) = &current {
                if let Some(next) = obj.get(key) {
                    current = next.clone();
                } else {
                    return Ok(JsonValue::Null);
                }
            } else {
                return Ok(JsonValue::Null);
            }
        }

        Ok(current)
    }

    /// Set JSON value at path
    pub fn set_path(
        &self,
        json: &mut JsonValue,
        path: &str,
        value: JsonValue,
    ) -> Result<(), DatabaseError> {
        let keys: Vec<&str> = path.split('.').collect();

        if keys.is_empty() {
            *json = value;
            return Ok(());
        }

        // Ensure json is an object
        if !json.is_object() {
            *json = json!({});
        }

        // Navigate and set
        let mut current = json;
        for (i, key) in keys.iter().enumerate() {
            if key.is_empty() {
                continue;
            }

            if i == keys.len() - 1 {
                // Last key - set value
                current[key] = value.clone();
            } else {
                // Intermediate key - ensure object exists
                if !current[key].is_object() {
                    current[key] = json!({});
                }
                current = &mut current[key];
            }
        }

        Ok(())
    }

    /// Merge two JSON objects
    pub fn merge(&self, base: &JsonValue, update: &JsonValue) -> Result<JsonValue, DatabaseError> {
        if !base.is_object() || !update.is_object() {
            return Err(DatabaseError::ValidationError(
                "Both values must be JSON objects".to_string(),
            ));
        }

        let mut result = base.clone();

        if let JsonValue::Object(base_obj) = &mut result {
            if let JsonValue::Object(update_obj) = update {
                for (key, value) in update_obj {
                    base_obj.insert(key.clone(), value.clone());
                }
            }
        }

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_json_value_conversion() {
        let support = PostgreSqlJsonSupport;

        let int_val = Value::Int(42);
        let json = support.to_json(&int_val).unwrap();
        assert_eq!(json, json!(42));

        let bool_val = Value::Bool(true);
        let json = support.to_json(&bool_val).unwrap();
        assert_eq!(json, json!(true));
    }

    #[test]
    fn test_from_json_conversion() {
        let support = PostgreSqlJsonSupport;

        let json = json!(42);
        let val = support.from_json(&json).unwrap();
        assert!(matches!(val, Value::Int(42)));

        let json = json!("hello");
        let val = support.from_json(&json).unwrap();
        assert!(matches!(val, Value::String(ref s) if s == "hello"));
    }

    #[test]
    fn test_json_contains() {
        let support = PostgreSqlJsonSupport;
        let json = json!({
            "name": "John",
            "age": 30,
            "email": "john@example.com"
        });

        assert!(support.contains(&json, "name"));
        assert!(support.contains(&json, "age"));
        assert!(!support.contains(&json, "phone"));
    }

    #[test]
    fn test_json_get_path() {
        let support = PostgreSqlJsonSupport;
        let json = json!({
            "user": {
                "name": "John",
                "profile": {
                    "age": 30
                }
            }
        });

        let result = support.get_path(&json, "user.name").unwrap();
        assert_eq!(result, json!("John"));

        let result = support.get_path(&json, "user.profile.age").unwrap();
        assert_eq!(result, json!(30));
    }

    #[test]
    fn test_json_set_path() {
        let support = PostgreSqlJsonSupport;
        let mut json = json!({
            "name": "John"
        });

        support.set_path(&mut json, "age", json!(30)).unwrap();
        assert_eq!(json["age"], json!(30));
    }

    #[test]
    fn test_json_merge() {
        let support = PostgreSqlJsonSupport;
        let base = json!({
            "name": "John",
            "age": 30
        });

        let update = json!({
            "age": 31,
            "email": "john@example.com"
        });

        let result = support.merge(&base, &update).unwrap();
        assert_eq!(result["name"], json!("John"));
        assert_eq!(result["age"], json!(31));
        assert_eq!(result["email"], json!("john@example.com"));
    }
}
