//! Dynamic Transport Management
//!
//! Provides runtime transport switching capabilities for STDIO/HTTP switching

use crate::transport::{http::HttpConfig, TransportConfig, TransportError, TransportType};
use std::sync::{Arc, RwLock};
use tokio::sync::watch;
use tracing::{error, info, warn};

/// Transport manager state
#[derive(Clone, Copy)]
enum TransportState {
    Stdio,
    Http(std::net::SocketAddr),
    Stopped,
}

/// HTTP server handle for graceful shutdown
struct HttpServerHandle {
    shutdown_tx: tokio::sync::oneshot::Sender<()>,
    addr: std::net::SocketAddr,
}

/// Dynamic transport manager for runtime transport switching
pub struct DynamicTransportManager {
    /// Current transport state
    current_state: Arc<RwLock<TransportState>>,
    /// Current transport configuration
    current_config: Arc<RwLock<TransportConfig>>,
    /// Transport change notification
    change_sender: watch::Sender<TransportType>,
    change_receiver: watch::Receiver<TransportType>,
    /// Transport status
    is_running: Arc<RwLock<bool>>,
    /// HTTP server handle for shutdown
    http_server_handle: Arc<RwLock<Option<HttpServerHandle>>>,
}

impl DynamicTransportManager {
    /// Create new dynamic transport manager
    pub fn new(initial_config: TransportConfig) -> Result<Self, TransportError> {
        let (sender, receiver) = watch::channel(initial_config.transport_type.clone());

        let initial_state = match &initial_config.transport_type {
            TransportType::Stdio => TransportState::Stdio,
            TransportType::Http { addr } => TransportState::Http(*addr),
            _ => TransportState::Stopped,
        };

        Ok(Self {
            current_state: Arc::new(RwLock::new(initial_state)),
            current_config: Arc::new(RwLock::new(initial_config)),
            change_sender: sender,
            change_receiver: receiver,
            is_running: Arc::new(RwLock::new(false)),
            http_server_handle: Arc::new(RwLock::new(None)),
        })
    }

    /// Switch to STDIO transport
    pub async fn switch_to_stdio(&self) -> Result<(), TransportError> {
        info!("🔄 Switching to STDIO transport...");

        // Stop current transport
        self.stop_current_transport().await?;

        // Update state
        {
            let mut state = self.current_state.write().unwrap();
            *state = TransportState::Stdio;
        }

        // Update config
        {
            let mut config = self.current_config.write().unwrap();
            config.transport_type = TransportType::Stdio;
        }

        // Notify change
        if let Err(e) = self.change_sender.send(TransportType::Stdio) {
            error!("Failed to notify transport change: {}", e);
        }

        info!("✅ Successfully switched to STDIO transport");
        Ok(())
    }

    /// Switch to HTTP transport
    pub async fn switch_to_http(&self, addr: std::net::SocketAddr) -> Result<(), TransportError> {
        info!("🔄 Switching to HTTP transport at {}...", addr);

        // Stop current transport
        self.stop_current_transport().await?;

        // Update state
        {
            let mut state = self.current_state.write().unwrap();
            *state = TransportState::Http(addr);
        }

        // Update config
        {
            let mut config = self.current_config.write().unwrap();
            config.transport_type = TransportType::Http { addr };
        }

        // Notify change
        if let Err(e) = self.change_sender.send(TransportType::Http { addr }) {
            error!("Failed to notify transport change: {}", e);
        }

        info!("✅ Successfully switched to HTTP transport at {}", addr);
        Ok(())
    }

    /// Get current transport type
    pub fn get_current_type(&self) -> TransportType {
        self.current_config.read().unwrap().transport_type.clone()
    }

    /// Check if transport is running
    pub fn is_running(&self) -> bool {
        *self.is_running.read().unwrap()
    }

    /// Start current transport
    pub async fn start(&self) -> Result<(), TransportError> {
        let state = self.current_state.read().unwrap();
        match *state {
            TransportState::Stdio => {
                info!("🚀 Starting STDIO transport");
                // STDIO transport is always available
                *self.is_running.write().unwrap() = true;
                info!("✅ STDIO transport started successfully");
            }
            TransportState::Http(addr) => {
                info!("🚀 Starting HTTP transport at {}", addr);

                // Create HTTP config
                let config = HttpConfig {
                    bind_addr: addr,
                    cors_enabled: true,
                    max_request_size: 1024 * 1024,
                    timeout_ms: 30000,
                };

                // Start HTTP server with graceful shutdown support
                let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();

                let server_addr = addr;
                tokio::spawn(async move {
                    use crate::transport::http::HttpTransport;

                    match HttpTransport::new(config) {
                        Ok(transport) => {
                            if let Err(e) = transport.start_server().await {
                                error!("Failed to start HTTP server: {}", e);
                                return;
                            }

                            // Wait for shutdown signal
                            let _ = shutdown_rx.await;
                            info!("HTTP server shutdown signal received at {}", server_addr);
                        }
                        Err(e) => {
                            error!("Failed to create HTTP transport: {}", e);
                        }
                    }
                });

                // Store shutdown handle
                *self.http_server_handle.write().unwrap() =
                    Some(HttpServerHandle { shutdown_tx, addr });

                *self.is_running.write().unwrap() = true;
                info!("✅ HTTP transport started successfully at {}", addr);
            }
            TransportState::Stopped => {
                warn!("⚠️ Transport is in stopped state");
            }
        }
        Ok(())
    }

    /// Stop current transport gracefully
    async fn stop_current_transport(&self) -> Result<(), TransportError> {
        if *self.is_running.read().unwrap() {
            // Get current state and handle - drop guard before await
            let (state, server_handle) = {
                let state = *self.current_state.read().unwrap();
                let handle = self.http_server_handle.write().unwrap().take();
                (state, handle)
            };

            match state {
                TransportState::Stdio => {
                    info!("⏹️ Stopping STDIO transport");
                }
                TransportState::Http(addr) => {
                    info!("⏹️ Stopping HTTP transport at {}", addr);

                    if let Some(server_handle) = server_handle {
                        info!(
                            "🛑 Sending shutdown signal to HTTP server at {}",
                            server_handle.addr
                        );

                        // Send shutdown signal (ignore error if receiver already dropped)
                        let _ = server_handle.shutdown_tx.send(());

                        // Give server time to gracefully shutdown
                        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

                        info!("✅ HTTP server shutdown completed at {}", addr);
                    } else {
                        warn!("⚠️ No HTTP server handle found for shutdown");
                    }
                }
                TransportState::Stopped => {}
            }
            *self.is_running.write().unwrap() = false;
            info!("⏹️ Transport stopped");
        }
        Ok(())
    }

    /// Get transport change receiver for monitoring
    pub fn get_change_receiver(&self) -> watch::Receiver<TransportType> {
        self.change_receiver.clone()
    }
}

/// Interactive transport switcher for runtime switching
pub struct TransportSwitcher {
    manager: Arc<DynamicTransportManager>,
}

impl TransportSwitcher {
    pub fn new(manager: Arc<DynamicTransportManager>) -> Self {
        Self { manager }
    }

    /// Run interactive transport switching
    pub async fn run_interactive_switch(&self) -> Result<(), TransportError> {
        use std::io::{self, Write};

        println!("🔧 Transport動的切り替え");
        println!("════════════════════════════════════════════════════════════");
        println!();

        // Show current transport
        let current_type = self.manager.get_current_type();
        let is_running = self.manager.is_running();

        println!("📋 現在のTransport:");
        println!("   - タイプ: {}", current_type);
        println!(
            "   - 状態: {}",
            if is_running { "稼働中" } else { "停止中" }
        );
        println!();

        // Show options
        println!("Transport切り替えオプション:");
        println!("  1. STDIOに切り替え");
        println!("  2. HTTPに切り替え");
        println!("  3. 現在の状態確認");
        println!("  0. キャンセル");
        println!();

        loop {
            print!("選択してください [1-3, 0]: ");
            io::stdout().flush().map_err(TransportError::Io)?;

            let mut input = String::new();
            match io::stdin().read_line(&mut input) {
                Ok(0) => {
                    println!("入力が終了しました。");
                    return Ok(());
                }
                Ok(_) => match input.trim() {
                    "1" => {
                        self.switch_to_stdio().await?;
                        break;
                    }
                    "2" => {
                        self.switch_to_http().await?;
                        break;
                    }
                    "3" => {
                        self.show_transport_status().await;
                    }
                    "0" => {
                        println!("キャンセルしました。");
                        return Ok(());
                    }
                    _ => {
                        println!("⚠️  無効な選択です。1-3または0を入力してください。");
                    }
                },
                Err(e) => return Err(TransportError::Io(e)),
            }
        }

        Ok(())
    }

    /// Switch to STDIO transport
    async fn switch_to_stdio(&self) -> Result<(), TransportError> {
        println!("\n📡 STDIO Transportに切り替え");
        println!("────────────────────────────────────────────────────────────");
        println!("⚠️  注意: Claude Desktop使用時はlog_level=\"error\"推奨");

        match self.manager.switch_to_stdio().await {
            Ok(()) => {
                println!("✅ STDIOに切り替えました!");
                println!("💡 Claude Desktopで使用可能です");
            }
            Err(e) => {
                println!("❌ STDIO切り替えに失敗: {}", e);
            }
        }

        Ok(())
    }

    /// Switch to HTTP transport
    async fn switch_to_http(&self) -> Result<(), TransportError> {
        use std::io::{self, Write};

        println!("\n🌐 HTTP Transportに切り替え");
        println!("────────────────────────────────────────────────────────────");

        print!("HTTPポート番号 (デフォルト: 8081): ");
        io::stdout().flush().map_err(TransportError::Io)?;

        let mut input = String::new();
        io::stdin()
            .read_line(&mut input)
            .map_err(TransportError::Io)?;

        let port: u16 = input.trim().parse().unwrap_or(8081);
        let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));

        match self.manager.switch_to_http(addr).await {
            Ok(()) => {
                println!("✅ HTTP Transportに切り替えました!");
                println!("🌐 URL: http://{}", addr);
                println!("💡 Web UIで使用可能です");
            }
            Err(e) => {
                println!("❌ HTTP切り替えに失敗: {}", e);
            }
        }

        Ok(())
    }

    /// Show current transport status
    async fn show_transport_status(&self) {
        println!("\n📊 Transport状態");
        println!("────────────────────────────────────────────────────────────");

        let current_type = self.manager.get_current_type();
        let is_running = self.manager.is_running();

        println!("Transport情報:");
        println!("  - タイプ: {}", current_type);
        println!(
            "  - 状態: {}",
            if is_running {
                "✅ 稼働中"
            } else {
                "⏸️ 停止中"
            }
        );

        match current_type {
            TransportType::Stdio => {
                println!("  - 通信方式: 標準入出力");
                println!("  - 適用場面: Claude Desktop, コマンドライン");
            }
            TransportType::Http { addr } => {
                println!("  - 通信方式: HTTP JSON-RPC");
                println!("  - エンドポイント: http://{}", addr);
                println!("  - 適用場面: Web UI, REST API");
            }
            TransportType::WebSocket { url } => {
                println!("  - 通信方式: WebSocket");
                println!("  - URL: {}", url);
                println!("  - 適用場面: リアルタイム通信");
            }
        }

        println!();
    }
}
