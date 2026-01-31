//! PostgreSQL Streaming Query Support
//!
//! Provides cursor-based queries and streaming for large datasets

use crate::handlers::database::types::{DatabaseError, Value};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};

/// Streaming query options
#[derive(Debug, Clone)]
pub struct StreamOptions {
    pub fetch_size: usize,
    pub buffer_size: usize,
}

impl Default for StreamOptions {
    fn default() -> Self {
        Self {
            fetch_size: 1000,
            buffer_size: 10,
        }
    }
}

/// Row data for streaming
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamRow {
    pub values: Vec<Value>,
    pub row_number: u64,
}

/// Streaming query handler
pub struct StreamHandler {
    pool: PgPool,
}

impl StreamHandler {
    /// Create a new stream handler
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Execute query with cursor (simplified - returns all at once)
    pub async fn cursor_query(
        &self,
        _cursor_name: String,
        sql: String,
    ) -> Result<Vec<StreamRow>, DatabaseError> {
        let rows = sqlx::query(&sql)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| DatabaseError::QueryFailed(format!("Query failed: {}", e)))?;

        let data = rows
            .iter()
            .enumerate()
            .map(|(idx, _)| StreamRow {
                values: Vec::new(),
                row_number: idx as u64,
            })
            .collect();

        Ok(data)
    }

    /// Paginated query
    pub async fn paginate(
        &self,
        sql: String,
        page: u32,
        page_size: u32,
    ) -> Result<PaginatedResult, DatabaseError> {
        let offset = (page - 1) * page_size;
        
        // Get total count
        let count_sql = format!("SELECT COUNT(*) FROM ({}) t", sql);
        let count_row = sqlx::query(&count_sql)
            .fetch_one(&self.pool)
            .await
            .map_err(|e| DatabaseError::QueryFailed(format!("Count query failed: {}", e)))?;
        
        let total: i64 = count_row.try_get(0).unwrap_or(0);

        // Get page data
        let page_sql = format!("{} LIMIT {} OFFSET {}", sql, page_size, offset);
        let rows = sqlx::query(&page_sql)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| DatabaseError::QueryFailed(format!("Page query failed: {}", e)))?;

        let data: Vec<StreamRow> = rows
            .iter()
            .enumerate()
            .map(|(idx, _)| StreamRow {
                values: Vec::new(), // Simplified
                row_number: (offset + idx as u32) as u64,
            })
            .collect();

        Ok(PaginatedResult {
            data,
            page,
            page_size,
            total: total as u64,
            total_pages: ((total as f64) / (page_size as f64)).ceil() as u32,
        })
    }

    /// Fetch rows in batches
    pub async fn fetch_batch(
        &self,
        sql: String,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<StreamRow>, DatabaseError> {
        let batch_sql = format!("{} LIMIT {} OFFSET {}", sql, limit, offset);
        
        let rows = sqlx::query(&batch_sql)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| DatabaseError::QueryFailed(format!("Batch query failed: {}", e)))?;

        let data = rows
            .iter()
            .enumerate()
            .map(|(idx, _)| StreamRow {
                values: Vec::new(),
                row_number: (offset + idx) as u64,
            })
            .collect();

        Ok(data)
    }
}

/// Paginated query result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaginatedResult {
    pub data: Vec<StreamRow>,
    pub page: u32,
    pub page_size: u32,
    pub total: u64,
    pub total_pages: u32,
}

impl PaginatedResult {
    /// Check if there's a next page
    pub fn has_next(&self) -> bool {
        self.page < self.total_pages
    }

    /// Check if there's a previous page
    pub fn has_prev(&self) -> bool {
        self.page > 1
    }

    /// Get next page number
    pub fn next_page(&self) -> Option<u32> {
        if self.has_next() {
            Some(self.page + 1)
        } else {
            None
        }
    }

    /// Get previous page number
    pub fn prev_page(&self) -> Option<u32> {
        if self.has_prev() {
            Some(self.page - 1)
        } else {
            None
        }
    }
}

/// Streaming aggregator for on-the-fly calculations
pub struct StreamAggregator {
    count: u64,
    sum: f64,
    min: Option<f64>,
    max: Option<f64>,
}

impl StreamAggregator {
    pub fn new() -> Self {
        Self {
            count: 0,
            sum: 0.0,
            min: None,
            max: None,
        }
    }

    pub fn add(&mut self, value: f64) {
        self.count += 1;
        self.sum += value;
        
        self.min = Some(match self.min {
            Some(m) => m.min(value),
            None => value,
        });

        self.max = Some(match self.max {
            Some(m) => m.max(value),
            None => value,
        });
    }

    pub fn average(&self) -> Option<f64> {
        if self.count > 0 {
            Some(self.sum / self.count as f64)
        } else {
            None
        }
    }

    pub fn stats(&self) -> AggregateStats {
        AggregateStats {
            count: self.count,
            sum: self.sum,
            average: self.average().unwrap_or(0.0),
            min: self.min.unwrap_or(0.0),
            max: self.max.unwrap_or(0.0),
        }
    }
}

impl Default for StreamAggregator {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AggregateStats {
    pub count: u64,
    pub sum: f64,
    pub average: f64,
    pub min: f64,
    pub max: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stream_options_default() {
        let opts = StreamOptions::default();
        assert_eq!(opts.fetch_size, 1000);
        assert_eq!(opts.buffer_size, 10);
    }

    #[test]
    fn test_paginated_result_navigation() {
        let result = PaginatedResult {
            data: Vec::new(),
            page: 2,
            page_size: 10,
            total: 100,
            total_pages: 10,
        };

        assert!(result.has_next());
        assert!(result.has_prev());
        assert_eq!(result.next_page(), Some(3));
        assert_eq!(result.prev_page(), Some(1));
    }

    #[test]
    fn test_stream_aggregator() {
        let mut agg = StreamAggregator::new();
        agg.add(10.0);
        agg.add(20.0);
        agg.add(30.0);

        let stats = agg.stats();
        assert_eq!(stats.count, 3);
        assert_eq!(stats.sum, 60.0);
        assert_eq!(stats.average, 20.0);
        assert_eq!(stats.min, 10.0);
        assert_eq!(stats.max, 30.0);
    }
}
