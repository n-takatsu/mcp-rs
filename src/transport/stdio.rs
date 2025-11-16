//! Standard I/O transport implementation for MCP.
//!
//! This module provides a transport implementation that communicates via
//! standard input and output streams, commonly used for process-based MCP servers.

use super::{
    ConnectionStats, FramingMethod, Transport, TransportCapabilities, TransportError,
    TransportInfo, TransportType,
};
use crate::types::{JsonRpcRequest, JsonRpcResponse};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::{
    collections::VecDeque,
    time::{Duration, Instant, SystemTime},
};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader, BufWriter},
    sync::Mutex,
    time::timeout,
};

/// Standard I/O transport configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StdioConfig {
    /// Buffer size for I/O operations
    pub buffer_size: usize,
    /// Operation timeout in milliseconds
    pub timeout_ms: u64,
    /// Enable Content-Length header framing
    pub content_length_header: bool,
    /// Message framing method
    pub framing_method: FramingMethod,
    /// Maximum message size in bytes
    pub max_message_size: usize,
    /// Enable JSON pretty printing for outgoing messages
    pub pretty_print: bool,
}

impl Default for StdioConfig {
    fn default() -> Self {
        Self {
            buffer_size: 8192,
            timeout_ms: 30_000,
            content_length_header: true,
            framing_method: FramingMethod::ContentLength,
            max_message_size: 1_048_576, // 1MB
            pretty_print: false,
        }
    }
}

/// Standard I/O transport implementation
#[derive(Debug)]
pub struct StdioTransport {
    config: StdioConfig,
    stats: Mutex<ConnectionStats>,
    stdin_reader: Mutex<Option<BufReader<tokio::io::Stdin>>>,
    stdout_writer: Mutex<Option<BufWriter<tokio::io::Stdout>>>,
    message_buffer: Mutex<VecDeque<JsonRpcRequest>>,
    is_active: Mutex<bool>,
}

impl StdioTransport {
    /// Create a new stdio transport with the given configuration
    pub fn new(config: StdioConfig) -> Result<Self, TransportError> {
        // Validate configuration
        if config.buffer_size == 0 {
            return Err(TransportError::Configuration(
                "Buffer size must be greater than 0".to_string(),
            ));
        }

        if config.max_message_size == 0 {
            return Err(TransportError::Configuration(
                "Maximum message size must be greater than 0".to_string(),
            ));
        }

        Ok(Self {
            config,
            stats: Mutex::new(ConnectionStats::default()),
            stdin_reader: Mutex::new(None),
            stdout_writer: Mutex::new(None),
            message_buffer: Mutex::new(VecDeque::new()),
            is_active: Mutex::new(false),
        })
    }

    /// Initialize stdin reader with buffering
    async fn initialize_stdin(&self) -> Result<(), TransportError> {
        let stdin = tokio::io::stdin();
        let reader = BufReader::with_capacity(self.config.buffer_size, stdin);
        *self.stdin_reader.lock().await = Some(reader);
        Ok(())
    }

    /// Initialize stdout writer with buffering
    async fn initialize_stdout(&self) -> Result<(), TransportError> {
        let stdout = tokio::io::stdout();
        let writer = BufWriter::with_capacity(self.config.buffer_size, stdout);
        *self.stdout_writer.lock().await = Some(writer);
        Ok(())
    }

    /// Read a single message from stdin based on framing method
    async fn read_message_from_stdin(&self) -> Result<Option<JsonRpcRequest>, TransportError> {
        let mut stdin_lock = self.stdin_reader.lock().await;
        let reader = stdin_lock.as_mut().ok_or(TransportError::Protocol(
            "Stdin not initialized".to_string(),
        ))?;

        let timeout_duration = Duration::from_millis(self.config.timeout_ms);

        match self.config.framing_method {
            FramingMethod::ContentLength => {
                self.read_content_length_message(reader, timeout_duration)
                    .await
            }
            FramingMethod::LineBased => {
                self.read_line_based_message(reader, timeout_duration).await
            }
            FramingMethod::WebSocketFrame => Err(TransportError::NotSupported(
                "WebSocket framing not supported for stdio".to_string(),
            )),
        }
    }

    /// Read message using Content-Length header framing
    async fn read_content_length_message(
        &self,
        reader: &mut BufReader<tokio::io::Stdin>,
        timeout_duration: Duration,
    ) -> Result<Option<JsonRpcRequest>, TransportError> {
        // Read Content-Length header
        let mut header_line = String::new();
        match timeout(timeout_duration, reader.read_line(&mut header_line)).await {
            Ok(Ok(0)) => return Ok(None), // EOF
            Ok(Ok(_)) => {}
            Ok(Err(e)) => return Err(TransportError::Io(e)),
            Err(_) => {
                return Err(TransportError::Timeout(
                    "Timeout reading Content-Length header".to_string(),
                ))
            }
        }

        // Parse Content-Length
        let content_length = if header_line.starts_with("Content-Length:") {
            let length_str = header_line.strip_prefix("Content-Length:").unwrap().trim();
            length_str.parse::<usize>().map_err(|_| {
                TransportError::Protocol(format!("Invalid Content-Length: {}", length_str))
            })?
        } else {
            return Err(TransportError::Protocol(format!(
                "Expected Content-Length header, got: {}",
                header_line.trim()
            )));
        };

        // Validate message size
        if content_length > self.config.max_message_size {
            return Err(TransportError::BufferOverflow);
        }

        // Read empty line separator
        let mut separator = String::new();
        timeout(timeout_duration, reader.read_line(&mut separator))
            .await
            .map_err(|_| TransportError::Timeout("Timeout reading separator".to_string()))?
            .map_err(TransportError::Io)?;

        // Read JSON content
        let mut content = vec![0u8; content_length];
        timeout(timeout_duration, reader.read_exact(&mut content))
            .await
            .map_err(|_| TransportError::Timeout("Timeout reading message content".to_string()))?
            .map_err(TransportError::Io)?;

        // Parse JSON
        let message: JsonRpcRequest = serde_json::from_slice(&content)?;

        // Update stats
        let mut stats = self.stats.lock().await;
        stats.messages_received += 1;
        stats.bytes_received += content_length as u64;
        stats.last_activity = Some(SystemTime::now());

        Ok(Some(message))
    }

    /// Read message using line-based framing
    async fn read_line_based_message(
        &self,
        reader: &mut BufReader<tokio::io::Stdin>,
        timeout_duration: Duration,
    ) -> Result<Option<JsonRpcRequest>, TransportError> {
        let mut line = String::new();
        match timeout(timeout_duration, reader.read_line(&mut line)).await {
            Ok(Ok(0)) => return Ok(None), // EOF
            Ok(Ok(_)) => {}
            Ok(Err(e)) => return Err(TransportError::Io(e)),
            Err(_) => {
                return Err(TransportError::Timeout(
                    "Timeout reading line-based message".to_string(),
                ))
            }
        }

        // Validate message size
        if line.len() > self.config.max_message_size {
            return Err(TransportError::BufferOverflow);
        }

        // Parse JSON
        let message: JsonRpcRequest = serde_json::from_str(line.trim())?;

        // Update stats
        let mut stats = self.stats.lock().await;
        stats.messages_received += 1;
        stats.bytes_received += line.len() as u64;
        stats.last_activity = Some(SystemTime::now());

        Ok(Some(message))
    }

    /// Write message to stdout based on framing method
    async fn write_message_to_stdout(
        &self,
        message: &JsonRpcResponse,
    ) -> Result<(), TransportError> {
        let mut stdout_lock = self.stdout_writer.lock().await;
        let writer = stdout_lock.as_mut().ok_or(TransportError::Protocol(
            "Stdout not initialized".to_string(),
        ))?;

        let timeout_duration = Duration::from_millis(self.config.timeout_ms);

        let json_str = if self.config.pretty_print {
            serde_json::to_string_pretty(message)?
        } else {
            serde_json::to_string(message)?
        };

        let bytes_to_send = match self.config.framing_method {
            FramingMethod::ContentLength => {
                let content = format!("Content-Length: {}\r\n\r\n{}", json_str.len(), json_str);
                content.into_bytes()
            }
            FramingMethod::LineBased => {
                let content = format!("{}\n", json_str);
                content.into_bytes()
            }
            FramingMethod::WebSocketFrame => {
                return Err(TransportError::NotSupported(
                    "WebSocket framing not supported for stdio".to_string(),
                ));
            }
        };

        // Validate message size
        if bytes_to_send.len() > self.config.max_message_size {
            return Err(TransportError::BufferOverflow);
        }

        // Write with timeout
        timeout(timeout_duration, writer.write_all(&bytes_to_send))
            .await
            .map_err(|_| TransportError::Timeout("Timeout writing message".to_string()))?
            .map_err(TransportError::Io)?;

        timeout(timeout_duration, writer.flush())
            .await
            .map_err(|_| TransportError::Timeout("Timeout flushing output".to_string()))?
            .map_err(TransportError::Io)?;

        // Update stats
        let mut stats = self.stats.lock().await;
        stats.messages_sent += 1;
        stats.bytes_sent += bytes_to_send.len() as u64;
        stats.last_activity = Some(SystemTime::now());

        Ok(())
    }
}

#[async_trait]
impl Transport for StdioTransport {
    type Error = TransportError;

    async fn start(&mut self) -> Result<(), Self::Error> {
        let mut is_active = self.is_active.lock().await;
        if *is_active {
            return Err(TransportError::Protocol(
                "Transport already started".to_string(),
            ));
        }

        self.initialize_stdin().await?;
        self.initialize_stdout().await?;

        *is_active = true;
        Ok(())
    }

    async fn stop(&mut self) -> Result<(), Self::Error> {
        let mut is_active = self.is_active.lock().await;
        if !*is_active {
            return Ok(()); // Already stopped
        }

        // Flush any remaining output
        if let Some(writer) = self.stdout_writer.lock().await.as_mut() {
            writer.flush().await.map_err(TransportError::Io)?;
        }

        // Clear resources
        *self.stdin_reader.lock().await = None;
        *self.stdout_writer.lock().await = None;
        self.message_buffer.lock().await.clear();

        *is_active = false;
        Ok(())
    }

    async fn send_message(&mut self, message: JsonRpcResponse) -> Result<(), Self::Error> {
        if !*self.is_active.lock().await {
            return Err(TransportError::Protocol(
                "Transport not started".to_string(),
            ));
        }

        self.write_message_to_stdout(&message).await
    }

    async fn receive_message(&mut self) -> Result<Option<JsonRpcRequest>, Self::Error> {
        if !*self.is_active.lock().await {
            return Err(TransportError::Protocol(
                "Transport not started".to_string(),
            ));
        }

        // Check buffer first
        {
            let mut buffer = self.message_buffer.lock().await;
            if let Some(message) = buffer.pop_front() {
                return Ok(Some(message));
            }
        }

        // Read from stdin
        self.read_message_from_stdin().await
    }

    fn is_connected(&self) -> bool {
        // For stdio, we consider it "connected" if the transport is active
        // In a real implementation, we might check if stdin/stdout are still valid
        futures::executor::block_on(async { *self.is_active.lock().await })
    }

    fn transport_info(&self) -> TransportInfo {
        TransportInfo {
            transport_type: TransportType::Stdio,
            description: "Standard I/O transport for MCP communication".to_string(),
            capabilities: TransportCapabilities {
                bidirectional: true,
                multiplexing: false,
                compression: false,
                max_message_size: Some(self.config.max_message_size),
                framing_methods: vec![FramingMethod::ContentLength, FramingMethod::LineBased],
            },
        }
    }

    fn connection_stats(&self) -> ConnectionStats {
        futures::executor::block_on(async { self.stats.lock().await.clone() })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stdio_config_default() {
        let config = StdioConfig::default();
        assert_eq!(config.buffer_size, 8192);
        assert_eq!(config.timeout_ms, 30_000);
        assert!(config.content_length_header);
        assert_eq!(config.max_message_size, 1_048_576);
        assert!(!config.pretty_print);
    }

    #[test]
    fn test_stdio_transport_creation() {
        let config = StdioConfig::default();
        let result = StdioTransport::new(config);
        assert!(result.is_ok());
    }

    #[test]
    fn test_invalid_config() {
        let config = StdioConfig {
            buffer_size: 0,
            ..Default::default()
        };

        let result = StdioTransport::new(config);
        assert!(result.is_err());

        match result.unwrap_err() {
            TransportError::Configuration(msg) => {
                assert!(msg.contains("Buffer size must be greater than 0"));
            }
            _ => panic!("Expected Configuration error"),
        }
    }

    #[test]
    fn test_transport_info() {
        let config = StdioConfig::default();
        let transport = StdioTransport::new(config).unwrap();
        let info = transport.transport_info();

        assert!(matches!(info.transport_type, TransportType::Stdio));
        assert!(info.capabilities.bidirectional);
        assert!(!info.capabilities.multiplexing);
        assert_eq!(info.capabilities.max_message_size, Some(1_048_576));
    }

    #[tokio::test]
    async fn test_transport_lifecycle() {
        let config = StdioConfig::default();
        let transport = StdioTransport::new(config).unwrap();

        assert!(!transport.is_connected());

        // Note: We can't actually test start() with real stdio in unit tests
        // This would require integration tests or mocking
    }
}
