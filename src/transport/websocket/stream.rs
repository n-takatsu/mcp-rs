//! WebSocket Streaming Transport

use super::connection::WebSocketConnection;
use super::types::*;
pub use super::types::{StreamConfig, StreamProgress};
use crate::error::{Error, Result};
use std::sync::Arc;
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::sync::RwLock;

/// ストリーミングトランスポート
pub struct StreamingTransport {
    connection: WebSocketConnection,
    config: StreamConfig,
    progress: Arc<RwLock<StreamProgress>>,
}

impl StreamingTransport {
    /// 新しいストリーミングトランスポートを作成
    pub fn new(connection: WebSocketConnection, config: StreamConfig) -> Self {
        let progress = StreamProgress {
            total_bytes: 0,
            transferred_bytes: 0,
            transfer_rate: 0.0,
            estimated_time_remaining: None,
        };

        Self {
            connection,
            config,
            progress: Arc::new(RwLock::new(progress)),
        }
    }

    /// データストリームを送信
    pub async fn send_stream<R: AsyncRead + Unpin>(
        &mut self,
        mut reader: R,
        total_size: Option<u64>,
    ) -> Result<u64> {
        let mut buffer = vec![0u8; self.config.chunk_size];
        let mut total_sent = 0u64;
        let start_time = std::time::Instant::now();

        // プログレス初期化
        {
            let mut progress = self.progress.write().await;
            progress.total_bytes = total_size.unwrap_or(0);
            progress.transferred_bytes = 0;
        }

        loop {
            let n = reader
                .read(&mut buffer)
                .await
                .map_err(|e| Error::StreamError(format!("Failed to read from stream: {}", e)))?;

            if n == 0 {
                break; // EOF
            }

            // データ圧縮（オプション）
            let data = if self.config.compression_enabled {
                compress_data(&buffer[..n])?
            } else {
                buffer[..n].to_vec()
            };

            // メッセージ送信
            self.connection.send(WebSocketMessage::Binary(data)).await?;

            total_sent += n as u64;

            // プログレス更新
            {
                let mut progress = self.progress.write().await;
                progress.transferred_bytes = total_sent;

                let elapsed = start_time.elapsed().as_secs_f64();
                if elapsed > 0.0 {
                    progress.transfer_rate = total_sent as f64 / elapsed;

                    if let Some(total) = total_size {
                        if progress.transfer_rate > 0.0 {
                            let remaining_bytes = total.saturating_sub(total_sent);
                            let eta_secs = remaining_bytes as f64 / progress.transfer_rate;
                            progress.estimated_time_remaining = Some(eta_secs);
                        }
                    }
                }
            }

            // バッファサイズチェック
            if total_sent > self.config.max_buffer_size as u64 {
                return Err(Error::StreamError("Buffer size limit exceeded".to_string()));
            }
        }

        Ok(total_sent)
    }

    /// データストリームを受信
    pub async fn receive_stream(&mut self) -> Result<Vec<u8>> {
        let mut received_data = Vec::new();
        let start_time = std::time::Instant::now();

        loop {
            match self.connection.receive().await? {
                Some(WebSocketMessage::Binary(data)) => {
                    // データ解凍（オプション）
                    let decompressed = if self.config.compression_enabled {
                        decompress_data(&data)?
                    } else {
                        data
                    };

                    received_data.extend_from_slice(&decompressed);

                    // プログレス更新
                    {
                        let mut progress = self.progress.write().await;
                        progress.transferred_bytes = received_data.len() as u64;

                        let elapsed = start_time.elapsed().as_secs_f64();
                        if elapsed > 0.0 {
                            progress.transfer_rate = received_data.len() as f64 / elapsed;
                        }
                    }

                    // バッファサイズチェック
                    if received_data.len() > self.config.max_buffer_size {
                        return Err(Error::StreamError("Buffer size limit exceeded".to_string()));
                    }
                }
                Some(WebSocketMessage::Close(_)) => {
                    break; // ストリーム終了
                }
                Some(_) => {
                    // その他のメッセージは無視
                }
                None => {
                    break; // 接続終了
                }
            }
        }

        Ok(received_data)
    }

    /// 進捗状況を取得
    pub async fn get_progress(&self) -> StreamProgress {
        self.progress.read().await.clone()
    }

    /// ファイル転送
    pub async fn transfer_file(&mut self, path: &str) -> Result<u64> {
        let file = tokio::fs::File::open(path)
            .await
            .map_err(|e| Error::StreamError(format!("Failed to open file: {}", e)))?;

        let metadata = file
            .metadata()
            .await
            .map_err(|e| Error::StreamError(format!("Failed to get metadata: {}", e)))?;

        let total_size = metadata.len();

        self.send_stream(file, Some(total_size)).await
    }

    /// ストリームをキャンセル
    pub async fn cancel(&mut self) -> Result<()> {
        self.connection.send(WebSocketMessage::Close(None)).await?;
        self.connection.close().await
    }

    /// 接続を取得
    pub fn connection(&self) -> &WebSocketConnection {
        &self.connection
    }
}

/// データ圧縮（簡易実装）
fn compress_data(data: &[u8]) -> Result<Vec<u8>> {
    // 実際の実装では、flate2やzstdなどの圧縮ライブラリを使用
    Ok(data.to_vec())
}

/// データ解凍（簡易実装）
fn decompress_data(data: &[u8]) -> Result<Vec<u8>> {
    // 実際の実装では、flate2やzstdなどの圧縮ライブラリを使用
    Ok(data.to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compress_decompress() {
        let data = b"test data";
        let compressed = compress_data(data).unwrap();
        let decompressed = decompress_data(&compressed).unwrap();
        assert_eq!(data, decompressed.as_slice());
    }

    #[test]
    fn test_stream_progress_percentage() {
        let progress = StreamProgress {
            total_bytes: 1000,
            transferred_bytes: 250,
            transfer_rate: 100.0,
            estimated_time_remaining: Some(75.0),
        };
        assert_eq!(progress.percentage(), 0.25);
    }
}
