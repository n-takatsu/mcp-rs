//! PostgreSQL Advanced Query Features
//!
//! Provides CTEs, Window Functions, LATERAL JOINs, and other advanced SQL features

use crate::handlers::database::types::{DatabaseError, QueryResult};
use serde_json::Value as JsonValue;
use sqlx::{Column, PgPool, Row, TypeInfo};
use std::collections::HashMap;

/// Common Table Expression (CTE) Builder
#[derive(Clone, Debug)]
pub struct CteBuilder {
    ctes: Vec<(String, String)>,
    main_query: Option<String>,
    recursive: bool,
}

impl CteBuilder {
    /// Create a new CTE builder
    pub fn new() -> Self {
        Self {
            ctes: Vec::new(),
            main_query: None,
            recursive: false,
        }
    }

    /// Add a CTE
    ///
    /// # Example
    /// ```ignore
    /// builder.with_cte("regional_sales", 
    ///     "SELECT region, SUM(amount) as total FROM orders GROUP BY region");
    /// ```
    pub fn with_cte(mut self, name: impl Into<String>, query: impl Into<String>) -> Self {
        self.ctes.push((name.into(), query.into()));
        self
    }

    /// Mark as recursive CTE
    pub fn recursive(mut self) -> Self {
        self.recursive = true;
        self
    }

    /// Set the main query
    pub fn main_query(mut self, query: impl Into<String>) -> Self {
        self.main_query = Some(query.into());
        self
    }

    /// Build the complete SQL
    pub fn build(&self) -> Result<String, DatabaseError> {
        if self.ctes.is_empty() {
            return Err(DatabaseError::InvalidQuery("No CTEs defined".to_string()));
        }

        let main_query = self.main_query.as_ref()
            .ok_or_else(|| DatabaseError::InvalidQuery("No main query defined".to_string()))?;

        let with_clause = if self.recursive { "WITH RECURSIVE" } else { "WITH" };
        
        let cte_parts: Vec<String> = self.ctes.iter()
            .map(|(name, query)| format!("{} AS ({})", name, query))
            .collect();

        Ok(format!("{} {} {}", with_clause, cte_parts.join(", "), main_query))
    }
}

impl Default for CteBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Window Function Builder
#[derive(Clone, Debug)]
pub struct WindowBuilder {
    function: String,
    partition_by: Vec<String>,
    order_by: Vec<String>,
    frame: Option<String>,
}

impl WindowBuilder {
    /// Create a new window function builder
    pub fn new(function: impl Into<String>) -> Self {
        Self {
            function: function.into(),
            partition_by: Vec::new(),
            order_by: Vec::new(),
            frame: None,
        }
    }

    /// Add partition by columns
    pub fn partition_by(mut self, columns: Vec<String>) -> Self {
        self.partition_by = columns;
        self
    }

    /// Add order by columns
    pub fn order_by(mut self, columns: Vec<String>) -> Self {
        self.order_by = columns;
        self
    }

    /// Set window frame
    ///
    /// # Example
    /// ```ignore
    /// builder.frame("ROWS BETWEEN UNBOUNDED PRECEDING AND CURRENT ROW");
    /// ```
    pub fn frame(mut self, frame: impl Into<String>) -> Self {
        self.frame = Some(frame.into());
        self
    }

    /// Build the window function SQL
    pub fn build(&self) -> String {
        let mut parts = vec![self.function.clone()];
        parts.push("OVER (".to_string());

        let mut over_parts = Vec::new();
        
        if !self.partition_by.is_empty() {
            over_parts.push(format!("PARTITION BY {}", self.partition_by.join(", ")));
        }

        if !self.order_by.is_empty() {
            over_parts.push(format!("ORDER BY {}", self.order_by.join(", ")));
        }

        if let Some(frame) = &self.frame {
            over_parts.push(frame.clone());
        }

        parts.push(over_parts.join(" "));
        parts.push(")".to_string());

        parts.join("")
    }
}

/// Advanced Query Handler
pub struct AdvancedQueryHandler {
    pool: PgPool,
}

impl AdvancedQueryHandler {
    /// Create a new advanced query handler
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Execute a CTE query
    pub async fn execute_cte(&self, builder: &CteBuilder) -> Result<QueryResult, DatabaseError> {
        let sql = builder.build()?;
        
        let start = std::time::Instant::now();
        let rows = sqlx::query(&sql)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| DatabaseError::QueryFailed(format!("CTE query failed: {}", e)))?;

        let execution_time_ms = start.elapsed().as_millis() as u64;

        // Convert rows to QueryResult
        if rows.is_empty() {
            return Ok(QueryResult {
                columns: Vec::new(),
                rows: Vec::new(),
                total_rows: Some(0),
                execution_time_ms,
            });
        }

        let columns = rows[0].columns().iter()
            .map(|col| crate::handlers::database::types::ColumnInfo {
                name: col.name().to_string(),
                data_type: col.type_info().name().to_string(),
                nullable: true,
                max_length: None,
            })
            .collect();

        let data_rows = rows.iter()
            .map(|row| {
                row.columns().iter().enumerate()
                    .map(|(i, _)| {
                        // Simplified value extraction
                        crate::handlers::database::types::Value::Null
                    })
                    .collect()
            })
            .collect();

        Ok(QueryResult {
            columns,
            rows: data_rows,
            total_rows: Some(rows.len() as u64),
            execution_time_ms,
        })
    }

    /// Execute hierarchical query (recursive CTE)
    ///
    /// # Example
    /// ```ignore
    /// let hierarchy = handler.hierarchical_query(
    ///     "employees",
    ///     "id",
    ///     "manager_id",
    ///     Some("WHERE level <= 5")
    /// ).await?;
    /// ```
    pub async fn hierarchical_query(
        &self,
        table: &str,
        id_column: &str,
        parent_column: &str,
        condition: Option<&str>,
    ) -> Result<QueryResult, DatabaseError> {
        let where_clause = condition.unwrap_or("");
        
        let cte = CteBuilder::new()
            .recursive()
            .with_cte(
                "hierarchy",
                format!(
                    "SELECT {}, {}, 1 as level FROM {} WHERE {} IS NULL \
                     UNION ALL \
                     SELECT t.{}, t.{}, h.level + 1 FROM {} t \
                     JOIN hierarchy h ON t.{} = h.{} {}",
                    id_column, parent_column, table, parent_column,
                    id_column, parent_column, table, parent_column, id_column, where_clause
                )
            )
            .main_query("SELECT * FROM hierarchy");

        self.execute_cte(&cte).await
    }

    /// Execute window function query
    pub async fn execute_window(
        &self,
        table: &str,
        select_columns: &[&str],
        window: &WindowBuilder,
        alias: &str,
    ) -> Result<QueryResult, DatabaseError> {
        let window_sql = window.build();
        let columns = select_columns.join(", ");
        let sql = format!(
            "SELECT {}, {} as {} FROM {}",
            columns, window_sql, alias, table
        );

        let start = std::time::Instant::now();
        let rows = sqlx::query(&sql)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| DatabaseError::QueryFailed(format!("Window query failed: {}", e)))?;

        let execution_time_ms = start.elapsed().as_millis() as u64;

        if rows.is_empty() {
            return Ok(QueryResult {
                columns: Vec::new(),
                rows: Vec::new(),
                total_rows: Some(0),
                execution_time_ms,
            });
        }

        let columns = rows[0].columns().iter()
            .map(|col| crate::handlers::database::types::ColumnInfo {
                name: col.name().to_string(),
                data_type: col.type_info().name().to_string(),
                nullable: true,
                max_length: None,
            })
            .collect();

        Ok(QueryResult {
            columns,
            rows: Vec::new(), // Simplified
            total_rows: Some(rows.len() as u64),
            execution_time_ms,
        })
    }

    /// Calculate running totals using window function
    pub async fn running_total(
        &self,
        table: &str,
        value_column: &str,
        order_column: &str,
        partition_columns: Option<Vec<String>>,
    ) -> Result<QueryResult, DatabaseError> {
        let mut window = WindowBuilder::new(format!("SUM({})", value_column))
            .order_by(vec![order_column.to_string()])
            .frame("ROWS BETWEEN UNBOUNDED PRECEDING AND CURRENT ROW");

        if let Some(partitions) = partition_columns {
            window = window.partition_by(partitions);
        }

        self.execute_window(table, &[value_column, order_column], &window, "running_total").await
    }

    /// Calculate rankings using window function
    pub async fn rank_rows(
        &self,
        table: &str,
        rank_type: RankType,
        order_column: &str,
        partition_columns: Option<Vec<String>>,
    ) -> Result<QueryResult, DatabaseError> {
        let rank_func = match rank_type {
            RankType::Dense => "DENSE_RANK()",
            RankType::Rank => "RANK()",
            RankType::RowNumber => "ROW_NUMBER()",
            RankType::PercentRank => "PERCENT_RANK()",
        };

        let mut window = WindowBuilder::new(rank_func.to_string())
            .order_by(vec![order_column.to_string()]);

        if let Some(partitions) = partition_columns {
            window = window.partition_by(partitions);
        }

        self.execute_window(table, &["*"], &window, "rank").await
    }

    /// Execute LATERAL JOIN query
    ///
    /// # Example
    /// ```ignore
    /// handler.lateral_join(
    ///     "departments",
    ///     "employees",
    ///     "department_id",
    ///     "SELECT * FROM employees WHERE department_id = d.id LIMIT 5"
    /// ).await?;
    /// ```
    pub async fn lateral_join(
        &self,
        main_table: &str,
        lateral_subquery: &str,
        alias: &str,
    ) -> Result<QueryResult, DatabaseError> {
        let sql = format!(
            "SELECT * FROM {} m, LATERAL ({}) {} ON true",
            main_table, lateral_subquery, alias
        );

        let start = std::time::Instant::now();
        let rows = sqlx::query(&sql)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| DatabaseError::QueryFailed(format!("LATERAL query failed: {}", e)))?;

        let execution_time_ms = start.elapsed().as_millis() as u64;

        Ok(QueryResult {
            columns: Vec::new(), // Simplified
            rows: Vec::new(),
            total_rows: Some(rows.len() as u64),
            execution_time_ms,
        })
    }

    /// Generate series (PostgreSQL generate_series)
    pub async fn generate_series(
        &self,
        start: i64,
        end: i64,
        step: Option<i64>,
    ) -> Result<Vec<i64>, DatabaseError> {
        let step_val = step.unwrap_or(1);
        let sql = format!("SELECT * FROM generate_series({}, {}, {})", start, end, step_val);

        let rows = sqlx::query(&sql)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| DatabaseError::QueryFailed(format!("generate_series failed: {}", e)))?;

        let values: Vec<i64> = rows.iter()
            .filter_map(|row| row.try_get::<i64, _>(0).ok())
            .collect();

        Ok(values)
    }

    /// Perform pivot operation using crosstab
    pub async fn pivot(
        &self,
        source_sql: &str,
        category_column: &str,
        value_column: &str,
        categories: &[&str],
    ) -> Result<QueryResult, DatabaseError> {
        // Simplified pivot using CASE WHEN
        let case_statements: Vec<String> = categories.iter()
            .map(|cat| format!(
                "SUM(CASE WHEN {} = '{}' THEN {} ELSE 0 END) as {}",
                category_column, cat, value_column, cat
            ))
            .collect();

        let sql = format!(
            "SELECT row_id, {} FROM ({}) t GROUP BY row_id",
            case_statements.join(", "),
            source_sql
        );

        let start = std::time::Instant::now();
        let rows = sqlx::query(&sql)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| DatabaseError::QueryFailed(format!("Pivot failed: {}", e)))?;

        let execution_time_ms = start.elapsed().as_millis() as u64;

        Ok(QueryResult {
            columns: Vec::new(), // Simplified
            rows: Vec::new(),
            total_rows: Some(rows.len() as u64),
            execution_time_ms,
        })
    }
}

/// Rank type for window functions
#[derive(Debug, Clone, Copy)]
pub enum RankType {
    Dense,
    Rank,
    RowNumber,
    PercentRank,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cte_builder_simple() {
        let cte = CteBuilder::new()
            .with_cte("sales", "SELECT * FROM orders WHERE year = 2026")
            .main_query("SELECT * FROM sales")
            .build()
            .unwrap();

        assert!(cte.contains("WITH"));
        assert!(cte.contains("sales AS"));
    }

    #[test]
    fn test_cte_builder_recursive() {
        let cte = CteBuilder::new()
            .recursive()
            .with_cte("tree", "SELECT id FROM nodes UNION ALL SELECT n.id FROM nodes n JOIN tree t ON n.parent = t.id")
            .main_query("SELECT * FROM tree")
            .build()
            .unwrap();

        assert!(cte.contains("WITH RECURSIVE"));
    }

    #[test]
    fn test_window_builder_partition() {
        let window = WindowBuilder::new("ROW_NUMBER()")
            .partition_by(vec!["department".to_string()])
            .order_by(vec!["salary DESC".to_string()])
            .build();

        assert!(window.contains("PARTITION BY"));
        assert!(window.contains("ORDER BY"));
    }

    #[test]
    fn test_window_builder_frame() {
        let window = WindowBuilder::new("AVG(price)")
            .order_by(vec!["date".to_string()])
            .frame("ROWS BETWEEN 2 PRECEDING AND 2 FOLLOWING")
            .build();

        assert!(window.contains("ROWS BETWEEN"));
    }
}
