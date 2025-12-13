//! ストリーミングレスポンス処理

use crate::llm::{
    error::{LlmError, LlmResult},
    types::StreamChunk,
};
use futures::Stream;
use std::pin::Pin;

/// ストリームヘルパー
pub struct StreamHelper;

impl StreamHelper {
    /// ストリームを文字列に収集
    pub async fn collect_stream(
        mut stream: Pin<Box<dyn Stream<Item = LlmResult<StreamChunk>> + Send>>,
    ) -> LlmResult<String> {
        use futures::StreamExt;

        let mut content = String::new();

        while let Some(result) = stream.next().await {
            let chunk = result?;
            content.push_str(&chunk.content);

            if chunk.done {
                break;
            }
        }

        Ok(content)
    }

    /// ストリームをコールバック付きで処理
    pub async fn process_stream<F>(
        mut stream: Pin<Box<dyn Stream<Item = LlmResult<StreamChunk>> + Send>>,
        mut callback: F,
    ) -> LlmResult<String>
    where
        F: FnMut(&str),
    {
        use futures::StreamExt;

        let mut content = String::new();

        while let Some(result) = stream.next().await {
            let chunk = result?;
            callback(&chunk.content);
            content.push_str(&chunk.content);

            if chunk.done {
                break;
            }
        }

        Ok(content)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::stream;

    #[tokio::test]
    async fn test_collect_stream() {
        let chunks = vec![
            Ok(StreamChunk {
                content: "Hello".to_string(),
                done: false,
                finish_reason: None,
            }),
            Ok(StreamChunk {
                content: " World".to_string(),
                done: true,
                finish_reason: Some("stop".to_string()),
            }),
        ];

        let stream = Box::pin(stream::iter(chunks));
        let result = StreamHelper::collect_stream(stream).await.unwrap();
        assert_eq!(result, "Hello World");
    }
}
