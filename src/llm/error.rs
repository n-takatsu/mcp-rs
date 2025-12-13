//! LLM統合のエラー型定義

use thiserror::Error;

/// LLM統合システムのエラー型
#[derive(Error, Debug)]
pub enum LlmError {
    /// API呼び出しエラー
    #[error("API error: {0}")]
    ApiError(String),

    /// 認証エラー
    #[error("Authentication error: {0}")]
    AuthError(String),

    /// レート制限エラー
    #[error("Rate limit exceeded: {0}")]
    RateLimitError(String),

    /// 無効なリクエスト
    #[error("Invalid request: {0}")]
    InvalidRequest(String),

    /// ストリーミングエラー
    #[error("Streaming error: {0}")]
    StreamingError(String),

    /// トークン制限超過
    #[error("Token limit exceeded: requested {requested}, max {max}")]
    TokenLimitExceeded { requested: usize, max: usize },

    /// プロバイダー未対応
    #[error("Unsupported provider: {0}")]
    UnsupportedProvider(String),

    /// 設定エラー
    #[error("Configuration error: {0}")]
    ConfigError(String),

    /// ネットワークエラー
    #[error("Network error: {0}")]
    NetworkError(String),

    /// タイムアウト
    #[error("Request timeout after {0}s")]
    Timeout(u64),

    /// JSONパースエラー
    #[error("JSON parse error: {0}")]
    JsonError(#[from] serde_json::Error),

    /// HTTPエラー
    #[error("HTTP error: {0}")]
    HttpError(String),

    /// その他のエラー
    #[error("Internal error: {0}")]
    Internal(String),
}

/// LLM統合システムの結果型
pub type LlmResult<T> = Result<T, LlmError>;
