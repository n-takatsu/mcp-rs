//! LLM統合モジュール
//! OpenAI/Anthropic APIとWebSocketストリーミングの統合を提供

use crate::error::{Error, Result};
use async_trait::async_trait;
use futures::stream::StreamExt;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

/// LLMプロバイダー種別
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LlmProvider {
    /// OpenAI (GPT-4, GPT-3.5等)
    OpenAI,
    /// Anthropic (Claude等)
    Anthropic,
}

/// LLM設定
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmConfig {
    /// プロバイダー
    pub provider: LlmProvider,
    /// APIキー
    pub api_key: String,
    /// モデル名
    pub model: String,
    /// 最大トークン数
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<usize>,
    /// 温度パラメータ (0.0-2.0)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    /// リトライ最大回数
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,
    /// リトライ初期待機時間(ms)
    #[serde(default = "default_initial_retry_delay_ms")]
    pub initial_retry_delay_ms: u64,
}

fn default_max_retries() -> u32 {
    3
}

fn default_initial_retry_delay_ms() -> u64 {
    1000
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            provider: LlmProvider::OpenAI,
            api_key: String::new(),
            model: "gpt-4".to_string(),
            max_tokens: Some(2048),
            temperature: Some(0.7),
            max_retries: default_max_retries(),
            initial_retry_delay_ms: default_initial_retry_delay_ms(),
        }
    }
}

/// ストリーミングチャンク
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamChunk {
    /// チャンクID
    pub id: String,
    /// テキストコンテンツ
    pub content: String,
    /// ストリーム完了フラグ
    pub done: bool,
    /// エラーメッセージ (あれば)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// LLMブリッジトレイト
#[async_trait]
pub trait LlmBridge: Send + Sync {
    /// ストリーミング補完を実行
    ///
    /// # 引数
    /// * `prompt` - プロンプトテキスト
    /// * `sender` - チャンク送信チャネル
    ///
    /// # 戻り値
    /// 成功時はOk(())、失敗時はError
    async fn stream_completion(
        &self,
        prompt: String,
        sender: mpsc::UnboundedSender<StreamChunk>,
    ) -> Result<()>;

    /// ストリームをキャンセル
    ///
    /// # 引数
    /// * `stream_id` - ストリームID
    ///
    /// # 戻り値
    /// 成功時はOk(())、失敗時はError
    async fn cancel_stream(&self, stream_id: String) -> Result<()>;
}

/// OpenAI LLMブリッジ実装
pub struct OpenAiBridge {
    config: LlmConfig,
    client: reqwest::Client,
}

impl OpenAiBridge {
    /// 新しいOpenAIブリッジを作成
    pub fn new(config: LlmConfig) -> Result<Self> {
        if config.api_key.is_empty() {
            return Err(Error::Configuration(
                "OpenAI API key is required".to_string(),
            ));
        }

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(300))
            .build()
            .map_err(|e| Error::Connection(format!("Failed to create HTTP client: {}", e)))?;

        Ok(Self { config, client })
    }

    /// エクスポネンシャルバックオフでリトライ
    async fn retry_with_backoff<F, Fut, T>(&self, mut operation: F) -> Result<T>
    where
        F: FnMut() -> Fut,
        Fut: std::future::Future<Output = Result<T>>,
    {
        let mut delay_ms = self.config.initial_retry_delay_ms;

        for attempt in 0..=self.config.max_retries {
            match operation().await {
                Ok(result) => return Ok(result),
                Err(e) => {
                    if attempt == self.config.max_retries {
                        return Err(e);
                    }

                    warn!(
                        "Attempt {} failed: {}, retrying in {}ms",
                        attempt + 1,
                        e,
                        delay_ms
                    );

                    tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
                    delay_ms *= 2; // エクスポネンシャルバックオフ
                }
            }
        }

        unreachable!()
    }
}

#[async_trait]
impl LlmBridge for OpenAiBridge {
    async fn stream_completion(
        &self,
        prompt: String,
        sender: mpsc::UnboundedSender<StreamChunk>,
    ) -> Result<()> {
        info!("Starting OpenAI streaming completion");

        let stream_id = uuid::Uuid::new_v4().to_string();

        // リトライロジック付きでAPIリクエスト実行
        self.retry_with_backoff(|| async {
            let request_body = serde_json::json!({
                "model": self.config.model,
                "messages": [{
                    "role": "user",
                    "content": prompt
                }],
                "stream": true,
                "max_tokens": self.config.max_tokens,
                "temperature": self.config.temperature,
            });

            let response = self
                .client
                .post("https://api.openai.com/v1/chat/completions")
                .header("Authorization", format!("Bearer {}", self.config.api_key))
                .header("Content-Type", "application/json")
                .json(&request_body)
                .send()
                .await
                .map_err(|e| Error::Connection(format!("OpenAI API request failed: {}", e)))?;

            if !response.status().is_success() {
                let status = response.status();
                let error_text = response
                    .text()
                    .await
                    .unwrap_or_else(|_| "Unknown error".to_string());
                return Err(Error::Server(format!(
                    "OpenAI API error {}: {}",
                    status, error_text
                )));
            }

            let mut stream = response.bytes_stream();
            let mut buffer = Vec::new();

            while let Some(chunk_result) = stream.next().await {
                let chunk = chunk_result.map_err(|e| {
                    Error::Connection(format!("Failed to read stream chunk: {}", e))
                })?;

                buffer.extend_from_slice(&chunk);

                // SSEフォーマットをパース (data: {json}\n\n)
                if let Some(pos) = buffer.windows(2).position(|w| w == b"\n\n") {
                    let line = String::from_utf8_lossy(&buffer[..pos]);

                    for data_line in line.lines() {
                        if let Some(json_str) = data_line.strip_prefix("data: ") {
                            if json_str.trim() == "[DONE]" {
                                // ストリーム完了
                                let done_chunk = StreamChunk {
                                    id: stream_id.clone(),
                                    content: String::new(),
                                    done: true,
                                    error: None,
                                };
                                sender.send(done_chunk).map_err(|e| {
                                    Error::Connection(format!("Failed to send chunk: {}", e))
                                })?;
                                debug!("OpenAI stream completed");
                                return Ok(());
                            }

                            // JSONパース
                            if let Ok(json) = serde_json::from_str::<serde_json::Value>(json_str) {
                                if let Some(choices) =
                                    json.get("choices").and_then(|v| v.as_array())
                                {
                                    if let Some(delta) =
                                        choices.first().and_then(|c| c.get("delta"))
                                    {
                                        if let Some(content) =
                                            delta.get("content").and_then(|v| v.as_str())
                                        {
                                            let chunk = StreamChunk {
                                                id: stream_id.clone(),
                                                content: content.to_string(),
                                                done: false,
                                                error: None,
                                            };

                                            sender.send(chunk).map_err(|e| {
                                                Error::Connection(format!(
                                                    "Failed to send chunk: {}",
                                                    e
                                                ))
                                            })?;
                                        }
                                    }
                                }
                            }
                        }
                    }

                    buffer.drain(..=pos + 1);
                }
            }

            Ok(())
        })
        .await
    }

    async fn cancel_stream(&self, stream_id: String) -> Result<()> {
        debug!("Canceling OpenAI stream: {}", stream_id);
        // OpenAI APIはストリームキャンセルをサポートしていないため、
        // クライアント側で接続を切断することで対応
        // 実装は将来的に拡張可能
        Ok(())
    }
}

/// Anthropic LLMブリッジ実装
pub struct AnthropicBridge {
    config: LlmConfig,
    client: reqwest::Client,
}

impl AnthropicBridge {
    /// 新しいAnthropicブリッジを作成
    pub fn new(config: LlmConfig) -> Result<Self> {
        if config.api_key.is_empty() {
            return Err(Error::Configuration(
                "Anthropic API key is required".to_string(),
            ));
        }

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(300))
            .build()
            .map_err(|e| Error::Connection(format!("Failed to create HTTP client: {}", e)))?;

        Ok(Self { config, client })
    }

    /// エクスポネンシャルバックオフでリトライ
    async fn retry_with_backoff<F, Fut, T>(&self, mut operation: F) -> Result<T>
    where
        F: FnMut() -> Fut,
        Fut: std::future::Future<Output = Result<T>>,
    {
        let mut delay_ms = self.config.initial_retry_delay_ms;

        for attempt in 0..=self.config.max_retries {
            match operation().await {
                Ok(result) => return Ok(result),
                Err(e) => {
                    if attempt == self.config.max_retries {
                        return Err(e);
                    }

                    warn!(
                        "Attempt {} failed: {}, retrying in {}ms",
                        attempt + 1,
                        e,
                        delay_ms
                    );

                    tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
                    delay_ms *= 2;
                }
            }
        }

        unreachable!()
    }
}

#[async_trait]
impl LlmBridge for AnthropicBridge {
    async fn stream_completion(
        &self,
        prompt: String,
        sender: mpsc::UnboundedSender<StreamChunk>,
    ) -> Result<()> {
        info!("Starting Anthropic streaming completion");

        let stream_id = uuid::Uuid::new_v4().to_string();

        // リトライロジック付きでAPIリクエスト実行
        self.retry_with_backoff(|| async {
            let request_body = serde_json::json!({
                "model": self.config.model,
                "messages": [{
                    "role": "user",
                    "content": prompt
                }],
                "stream": true,
                "max_tokens": self.config.max_tokens.unwrap_or(2048),
            });

            let response = self
                .client
                .post("https://api.anthropic.com/v1/messages")
                .header("x-api-key", &self.config.api_key)
                .header("anthropic-version", "2023-06-01")
                .header("Content-Type", "application/json")
                .json(&request_body)
                .send()
                .await
                .map_err(|e| Error::Connection(format!("Anthropic API request failed: {}", e)))?;

            if !response.status().is_success() {
                let status = response.status();
                let error_text = response
                    .text()
                    .await
                    .unwrap_or_else(|_| "Unknown error".to_string());
                return Err(Error::Server(format!(
                    "Anthropic API error {}: {}",
                    status, error_text
                )));
            }

            let mut stream = response.bytes_stream();
            let mut buffer = Vec::new();

            while let Some(chunk_result) = stream.next().await {
                let chunk = chunk_result.map_err(|e| {
                    Error::Connection(format!("Failed to read stream chunk: {}", e))
                })?;

                buffer.extend_from_slice(&chunk);

                // SSEフォーマットをパース
                if let Some(pos) = buffer.windows(2).position(|w| w == b"\n\n") {
                    let line = String::from_utf8_lossy(&buffer[..pos]);

                    for data_line in line.lines() {
                        if let Some(json_str) = data_line.strip_prefix("data: ") {
                            if let Ok(json) = serde_json::from_str::<serde_json::Value>(json_str) {
                                // イベントタイプをチェック
                                if let Some(event_type) = json.get("type").and_then(|v| v.as_str())
                                {
                                    match event_type {
                                        "content_block_delta" => {
                                            if let Some(delta) = json.get("delta") {
                                                if let Some(text) =
                                                    delta.get("text").and_then(|v| v.as_str())
                                                {
                                                    let chunk = StreamChunk {
                                                        id: stream_id.clone(),
                                                        content: text.to_string(),
                                                        done: false,
                                                        error: None,
                                                    };

                                                    sender.send(chunk).map_err(|e| {
                                                        Error::Connection(format!(
                                                            "Failed to send chunk: {}",
                                                            e
                                                        ))
                                                    })?;
                                                }
                                            }
                                        }
                                        "message_stop" => {
                                            // ストリーム完了
                                            let done_chunk = StreamChunk {
                                                id: stream_id.clone(),
                                                content: String::new(),
                                                done: true,
                                                error: None,
                                            };
                                            sender.send(done_chunk).map_err(|e| {
                                                Error::Connection(format!(
                                                    "Failed to send chunk: {}",
                                                    e
                                                ))
                                            })?;
                                            debug!("Anthropic stream completed");
                                            return Ok(());
                                        }
                                        "error" => {
                                            if let Some(error) = json.get("error") {
                                                let error_msg = error.to_string();
                                                error!("Anthropic stream error: {}", error_msg);
                                                let error_chunk = StreamChunk {
                                                    id: stream_id.clone(),
                                                    content: String::new(),
                                                    done: true,
                                                    error: Some(error_msg),
                                                };
                                                sender.send(error_chunk).map_err(|e| {
                                                    Error::Connection(format!(
                                                        "Failed to send error chunk: {}",
                                                        e
                                                    ))
                                                })?;
                                                return Err(Error::Server(format!(
                                                    "Anthropic API error: {}",
                                                    error
                                                )));
                                            }
                                        }
                                        _ => {
                                            // その他のイベントタイプは無視
                                            debug!("Ignoring event type: {}", event_type);
                                        }
                                    }
                                }
                            }
                        }
                    }

                    buffer.drain(..=pos + 1);
                }
            }

            Ok(())
        })
        .await
    }

    async fn cancel_stream(&self, stream_id: String) -> Result<()> {
        debug!("Canceling Anthropic stream: {}", stream_id);
        // Anthropic APIもストリームキャンセルをサポートしていないため、
        // クライアント側で接続を切断することで対応
        Ok(())
    }
}

/// LLMブリッジファクトリー
pub struct LlmBridgeFactory;

impl LlmBridgeFactory {
    /// 設定からLLMブリッジを作成
    pub fn create(config: LlmConfig) -> Result<Arc<dyn LlmBridge>> {
        match config.provider {
            LlmProvider::OpenAI => {
                let bridge = OpenAiBridge::new(config)?;
                Ok(Arc::new(bridge))
            }
            LlmProvider::Anthropic => {
                let bridge = AnthropicBridge::new(config)?;
                Ok(Arc::new(bridge))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_llm_config_default() {
        let config = LlmConfig::default();
        assert_eq!(config.provider, LlmProvider::OpenAI);
        assert_eq!(config.model, "gpt-4");
        assert_eq!(config.max_tokens, Some(2048));
        assert_eq!(config.temperature, Some(0.7));
        assert_eq!(config.max_retries, 3);
        assert_eq!(config.initial_retry_delay_ms, 1000);
    }

    #[test]
    fn test_stream_chunk_serialization() {
        let chunk = StreamChunk {
            id: "test-123".to_string(),
            content: "Hello, world!".to_string(),
            done: false,
            error: None,
        };

        let json = serde_json::to_string(&chunk).unwrap();
        let deserialized: StreamChunk = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.id, chunk.id);
        assert_eq!(deserialized.content, chunk.content);
        assert_eq!(deserialized.done, chunk.done);
        assert_eq!(deserialized.error, chunk.error);
    }

    #[test]
    fn test_openai_bridge_creation() {
        let config = LlmConfig {
            provider: LlmProvider::OpenAI,
            api_key: "test-key".to_string(),
            model: "gpt-4".to_string(),
            max_tokens: Some(1024),
            temperature: Some(0.5),
            max_retries: 3,
            initial_retry_delay_ms: 1000,
        };

        let result = OpenAiBridge::new(config);
        assert!(result.is_ok());
    }

    #[test]
    fn test_openai_bridge_requires_api_key() {
        let config = LlmConfig {
            provider: LlmProvider::OpenAI,
            api_key: String::new(),
            model: "gpt-4".to_string(),
            max_tokens: Some(1024),
            temperature: Some(0.5),
            max_retries: 3,
            initial_retry_delay_ms: 1000,
        };

        let result = OpenAiBridge::new(config);
        assert!(result.is_err());
    }

    #[test]
    fn test_anthropic_bridge_creation() {
        let config = LlmConfig {
            provider: LlmProvider::Anthropic,
            api_key: "test-key".to_string(),
            model: "claude-3-opus-20240229".to_string(),
            max_tokens: Some(2048),
            temperature: Some(0.7),
            max_retries: 3,
            initial_retry_delay_ms: 1000,
        };

        let result = AnthropicBridge::new(config);
        assert!(result.is_ok());
    }

    #[test]
    fn test_anthropic_bridge_requires_api_key() {
        let config = LlmConfig {
            provider: LlmProvider::Anthropic,
            api_key: String::new(),
            model: "claude-3-opus-20240229".to_string(),
            max_tokens: Some(2048),
            temperature: Some(0.7),
            max_retries: 3,
            initial_retry_delay_ms: 1000,
        };

        let result = AnthropicBridge::new(config);
        assert!(result.is_err());
    }

    #[test]
    fn test_factory_creates_openai_bridge() {
        let config = LlmConfig {
            provider: LlmProvider::OpenAI,
            api_key: "test-key".to_string(),
            model: "gpt-4".to_string(),
            max_tokens: Some(1024),
            temperature: Some(0.5),
            max_retries: 3,
            initial_retry_delay_ms: 1000,
        };

        let result = LlmBridgeFactory::create(config);
        assert!(result.is_ok());
    }

    #[test]
    fn test_factory_creates_anthropic_bridge() {
        let config = LlmConfig {
            provider: LlmProvider::Anthropic,
            api_key: "test-key".to_string(),
            model: "claude-3-opus-20240229".to_string(),
            max_tokens: Some(2048),
            temperature: Some(0.7),
            max_retries: 3,
            initial_retry_delay_ms: 1000,
        };

        let result = LlmBridgeFactory::create(config);
        assert!(result.is_ok());
    }
}
