//! LLM統合システム
//!
//! このモジュールは、複数のLLMプロバイダー（OpenAI、ローカルモデル）との
//! 統合機能を提供します。

pub mod client;
pub mod config;
pub mod error;
pub mod providers;
pub mod streaming;
pub mod types;

pub use client::LlmClient;
pub use config::{LlmConfig, LlmProvider};
pub use error::{LlmError, LlmResult};
pub use types::{LlmRequest, LlmResponse, Message, Role};
