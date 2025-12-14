//! Intrusion Detection System (IDS)
//!
//! 包括的な侵入検知システムで、高度な攻撃の早期検知を実現します。
//!
//! ## 主要機能
//!
//! - **シグネチャベース検知**: 既知の攻撃パターンのマッチング
//! - **振る舞いベース検知**: ベースライン学習と異常行動検出
//! - **ネットワーク監視**: リクエストパターン分析とDDoS検知
//! - **アラート管理**: 重要度別アラート生成と通知
//! - **リアルタイム分析**: 低レイテンシ検知（<1秒）
//!
//! ## 使用例
//!
//! ```rust,no_run
//! use mcp_rs::security::ids::{IntrusionDetectionSystem, IDSConfig, RequestData};
//! use std::collections::HashMap;
//! use chrono::Utc;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let ids = IntrusionDetectionSystem::new(IDSConfig::default()).await?;
//!
//! let request_data = RequestData {
//!     request_id: "req-123".to_string(),
//!     method: "GET".to_string(),
//!     path: "/api/data".to_string(),
//!     query_params: HashMap::new(),
//!     headers: HashMap::new(),
//!     body: None,
//!     source_ip: Some("192.168.1.1".parse().unwrap()),
//!     timestamp: Utc::now(),
//! };
//!
//! let result = ids.analyze_request(&request_data).await?;
//! if result.is_intrusion {
//!     ids.generate_alert(result).await?;
//! }
//! # Ok(())
//! # }
//! ```

pub mod alerts;
pub mod behavioral;
pub mod config;
pub mod detector;
pub mod network;
pub mod signature;
pub mod types;

#[cfg(feature = "ml-anomaly-detection")]
pub mod ml;

pub use config::{IDSConfig, IDSStats};
pub use detector::IntrusionDetectionSystem;
pub use types::{
    AttackDetails, DetectionResult, DetectionType, GeoLocation, RecommendedAction, RequestData,
    Severity, SourceInfo,
};
