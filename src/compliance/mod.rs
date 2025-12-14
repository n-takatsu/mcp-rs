//! GDPR/CCPA Compliance Engine
//!
//! このモジュールは、GDPR（EU一般データ保護規則）およびCCPA（カリフォルニア州消費者プライバシー法）
//! に準拠したコンプライアンス機能を提供します。
//!
//! ## 主要機能
//!
//! - **データ主体リクエスト処理**: 削除権、アクセス権、ポータビリティ権
//! - **同意管理**: 同意取得、撤回、バージョン管理
//! - **データライフサイクル管理**: 保持ポリシー、自動削除
//! - **監査証跡**: 全ての処理を記録し、法的監査に対応
//!
//! ## 使用例
//!
//! ```rust
//! use mcp_rs::compliance::{ComplianceEngine, DataSubjectRequest, RequestType};
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! // コンプライアンスエンジンの初期化
//! let engine = ComplianceEngine::new();
//!
//! // データ削除リクエストの処理
//! let request = DataSubjectRequest::new(
//!     "user@example.com",
//!     RequestType::Erasure,
//! );
//!
//! let result = engine.process_request(request).await?;
//! # Ok(())
//! # }
//! ```

pub mod consent_manager;
pub mod data_subject_requests;
pub mod engine;
pub mod lifecycle_manager;
pub mod types;

pub use engine::ComplianceEngine;
pub use types::*;
