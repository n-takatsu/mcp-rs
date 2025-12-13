//! LLM統合の型定義

use serde::{Deserialize, Serialize};

/// メッセージのロール
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    /// システムメッセージ
    System,
    /// ユーザーメッセージ
    User,
    /// アシスタント（AI）メッセージ
    Assistant,
}

/// チャットメッセージ
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    /// メッセージのロール
    pub role: Role,
    /// メッセージ内容
    pub content: String,
}

impl Message {
    /// 新しいシステムメッセージを作成
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: Role::System,
            content: content.into(),
        }
    }

    /// 新しいユーザーメッセージを作成
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: Role::User,
            content: content.into(),
        }
    }

    /// 新しいアシスタントメッセージを作成
    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: Role::Assistant,
            content: content.into(),
        }
    }
}

/// LLMリクエスト
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmRequest {
    /// メッセージ履歴
    pub messages: Vec<Message>,
    /// 使用するモデル（オプション）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// 温度パラメータ（0.0-2.0）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    /// 最大トークン数
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<usize>,
    /// Top-pサンプリング
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,
    /// ストリーミング有効化
    #[serde(default)]
    pub stream: bool,
}

impl LlmRequest {
    /// 新しいリクエストを作成
    pub fn new(messages: Vec<Message>) -> Self {
        Self {
            messages,
            model: None,
            temperature: None,
            max_tokens: None,
            top_p: None,
            stream: false,
        }
    }

    /// モデルを設定
    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = Some(model.into());
        self
    }

    /// 温度を設定
    pub fn with_temperature(mut self, temperature: f32) -> Self {
        self.temperature = Some(temperature);
        self
    }

    /// 最大トークン数を設定
    pub fn with_max_tokens(mut self, max_tokens: usize) -> Self {
        self.max_tokens = Some(max_tokens);
        self
    }

    /// ストリーミングを有効化
    pub fn with_streaming(mut self, stream: bool) -> Self {
        self.stream = stream;
        self
    }
}

/// LLMレスポンス
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmResponse {
    /// 生成されたテキスト
    pub content: String,
    /// 使用されたモデル
    pub model: String,
    /// 使用トークン数
    pub usage: TokenUsage,
    /// レスポンスID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// 完了理由
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finish_reason: Option<String>,
}

/// トークン使用量
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TokenUsage {
    /// プロンプトトークン数
    pub prompt_tokens: usize,
    /// 完了トークン数
    pub completion_tokens: usize,
    /// 合計トークン数
    pub total_tokens: usize,
}

impl TokenUsage {
    /// 新しいトークン使用量を作成
    pub fn new(prompt_tokens: usize, completion_tokens: usize) -> Self {
        Self {
            prompt_tokens,
            completion_tokens,
            total_tokens: prompt_tokens + completion_tokens,
        }
    }
}

/// ストリーミングチャンク
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamChunk {
    /// チャンクの内容
    pub content: String,
    /// 完了フラグ
    pub done: bool,
    /// 完了理由（完了時のみ）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finish_reason: Option<String>,
}
