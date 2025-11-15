//! Runtime Management for MCP-RS
//!
//! Provides runtime control including transport switching and configuration reloading

use crate::config::{ConfigSwitcher, DynamicConfigManager};
use crate::transport::{
    DynamicTransportManager, TransportConfig, TransportSwitcher, TransportType,
};
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{error, info, warn};

/// Runtime control commands
#[derive(Debug, Clone)]
pub enum RuntimeCommand {
    /// Switch to STDIO transport
    SwitchToStdio,
    /// Switch to HTTP transport with specified address
    SwitchToHttp(std::net::SocketAddr),
    /// Reload configuration
    ReloadConfig,
    /// Show current status
    ShowStatus,
    /// Graceful shutdown
    Shutdown,
}

/// Runtime controller for managing MCP server at runtime
pub struct RuntimeController {
    /// Transport manager
    transport_manager: Arc<DynamicTransportManager>,
    /// Configuration manager
    config_manager: Arc<DynamicConfigManager>,
    /// Command receiver
    command_receiver: mpsc::Receiver<RuntimeCommand>,
    /// Command sender (for cloning)
    command_sender: mpsc::Sender<RuntimeCommand>,
    /// Shutdown signal
    shutdown_requested: bool,
}

impl RuntimeController {
    /// Create new runtime controller
    pub fn new(
        transport_config: TransportConfig,
        config_manager: Arc<DynamicConfigManager>,
    ) -> Result<(Self, mpsc::Sender<RuntimeCommand>), Box<dyn std::error::Error>> {
        let transport_manager = Arc::new(DynamicTransportManager::new(transport_config)?);
        let (sender, receiver) = mpsc::channel(100);

        Ok((
            Self {
                transport_manager,
                config_manager,
                command_receiver: receiver,
                command_sender: sender.clone(),
                shutdown_requested: false,
            },
            sender,
        ))
    }

    /// Start runtime control loop
    pub async fn run(mut self) -> Result<(), Box<dyn std::error::Error>> {
        info!("🎛️ Runtime Controller開始");

        // Start initial transport
        self.transport_manager.start().await?;

        let mut config_change_receiver = self.config_manager.get_change_receiver();
        let mut transport_change_receiver = self.transport_manager.get_change_receiver();

        while !self.shutdown_requested {
            tokio::select! {
                // Handle runtime commands
                command = self.command_receiver.recv() => {
                    if let Some(cmd) = command {
                        if let Err(e) = self.handle_command(cmd).await {
                            error!("Runtime command error: {}", e);
                        }
                    }
                }

                // Monitor configuration changes
                _ = config_change_receiver.changed() => {
                    info!("Configuration changed, checking for transport updates...");
                    if let Err(e) = self.handle_config_change().await {
                        error!("Config change handling error: {}", e);
                    }
                }

                // Monitor transport changes
                _ = transport_change_receiver.changed() => {
                    let current_type = self.transport_manager.get_current_type();
                    info!("Transport changed to: {}", current_type);
                }
            }
        }

        info!("🛑 Runtime Controller終了");
        Ok(())
    }

    /// Handle runtime command
    async fn handle_command(
        &mut self,
        command: RuntimeCommand,
    ) -> Result<(), Box<dyn std::error::Error>> {
        match command {
            RuntimeCommand::SwitchToStdio => {
                info!("📡 STDIO transport切り替え要求");
                self.transport_manager.switch_to_stdio().await?;
            }

            RuntimeCommand::SwitchToHttp(addr) => {
                info!("🌐 HTTP transport切り替え要求: {}", addr);
                self.transport_manager.switch_to_http(addr).await?;
            }

            RuntimeCommand::ReloadConfig => {
                info!("🔄 設定リロード要求");
                self.config_manager.reload_config().await?;
            }

            RuntimeCommand::ShowStatus => {
                self.show_runtime_status().await;
            }

            RuntimeCommand::Shutdown => {
                info!("🛑 Graceful shutdown要求");
                self.shutdown_requested = true;
            }
        }

        Ok(())
    }

    /// Handle configuration change
    async fn handle_config_change(&self) -> Result<(), Box<dyn std::error::Error>> {
        let new_config = self.config_manager.get_config();

        // Check if transport configuration changed
        let current_transport_type = self.transport_manager.get_current_type();

        // Convert config transport type string to TransportType enum
        let new_transport_type = match new_config.transport.transport_type.as_deref() {
            Some("stdio") => crate::transport::TransportType::Stdio,
            Some("http") => {
                let addr = "127.0.0.1:8081".parse().unwrap(); // Default HTTP address
                crate::transport::TransportType::Http { addr }
            }
            Some("websocket") => {
                let url = "ws://127.0.0.1:8082".to_string();
                crate::transport::TransportType::WebSocket { url }
            }
            _ => current_transport_type.clone(), // Keep current if invalid/unknown
        };

        if current_transport_type != new_transport_type {
            info!(
                "Transport configuration changed: {} -> {}",
                current_transport_type, new_transport_type
            );

            match new_transport_type {
                crate::transport::TransportType::Stdio => {
                    self.transport_manager
                        .switch_to_stdio()
                        .await
                        .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
                }
                crate::transport::TransportType::Http { addr } => {
                    self.transport_manager
                        .switch_to_http(addr)
                        .await
                        .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
                }
                crate::transport::TransportType::WebSocket { .. } => {
                    warn!("WebSocket transport not yet implemented, keeping current transport");
                }
            }
        }

        Ok(())
    }

    /// Show current runtime status
    async fn show_runtime_status(&self) {
        println!("\n📊 MCP-RS Runtime Status");
        println!("════════════════════════════════════════════════════════════");

        // Transport status
        let transport_type = self.transport_manager.get_current_type();
        let transport_running = self.transport_manager.is_running();

        println!("🚀 Transport情報:");
        println!("   - タイプ: {}", transport_type);
        println!(
            "   - 状態: {}",
            if transport_running {
                "✅ 稼働中"
            } else {
                "⏸️ 停止中"
            }
        );

        // Configuration status
        let config_path = self
            .config_manager
            .get_config_path()
            .unwrap_or_else(|| "デフォルト".to_string());
        let config_version = self.config_manager.get_version();

        println!("⚙️ 設定情報:");
        println!("   - ファイル: {}", config_path);
        println!("   - バージョン: {}", config_version);

        println!();
    }

    /// Get command sender for external control
    pub fn get_command_sender(&self) -> mpsc::Sender<RuntimeCommand> {
        self.command_sender.clone()
    }
}

/// Interactive runtime control interface
pub struct InteractiveController {
    command_sender: mpsc::Sender<RuntimeCommand>,
}

impl InteractiveController {
    pub fn new(command_sender: mpsc::Sender<RuntimeCommand>) -> Self {
        Self { command_sender }
    }

    /// Run interactive control interface
    pub async fn run_interactive(&self) -> Result<(), Box<dyn std::error::Error>> {
        use std::io::{self, Write};

        println!("🎮 MCP-RS Interactive Control");
        println!("════════════════════════════════════════════════════════════");
        println!("ランタイム制御コマンド:");
        println!("  1. STDIO切り替え");
        println!("  2. HTTP切り替え");
        println!("  3. 設定リロード");
        println!("  4. ステータス表示");
        println!("  9. サーバー終了");
        println!("  0. 終了");
        println!();

        loop {
            print!("コマンド選択 [1-4, 9, 0]: ");
            io::stdout().flush()?;

            let mut input = String::new();
            match io::stdin().read_line(&mut input) {
                Ok(0) => break,
                Ok(_) => match input.trim() {
                    "1" => {
                        self.command_sender
                            .send(RuntimeCommand::SwitchToStdio)
                            .await?;
                        println!("✅ STDIO切り替えコマンド送信");
                    }
                    "2" => {
                        print!("HTTPポート [8081]: ");
                        io::stdout().flush()?;
                        let mut port_input = String::new();
                        io::stdin().read_line(&mut port_input)?;
                        let port: u16 = port_input.trim().parse().unwrap_or(8081);
                        let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));

                        self.command_sender
                            .send(RuntimeCommand::SwitchToHttp(addr))
                            .await?;
                        println!("✅ HTTP切り替えコマンド送信: {}", addr);
                    }
                    "3" => {
                        self.command_sender
                            .send(RuntimeCommand::ReloadConfig)
                            .await?;
                        println!("✅ 設定リロードコマンド送信");
                    }
                    "4" => {
                        self.command_sender.send(RuntimeCommand::ShowStatus).await?;
                        // 少し待ってからステータス表示
                        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                    }
                    "9" => {
                        self.command_sender.send(RuntimeCommand::Shutdown).await?;
                        println!("✅ サーバー終了コマンド送信");
                        break;
                    }
                    "0" => {
                        println!("Interactive Control終了");
                        break;
                    }
                    _ => {
                        println!("⚠️ 無効なコマンドです");
                    }
                },
                Err(e) => {
                    error!("Input error: {}", e);
                    break;
                }
            }
        }

        Ok(())
    }
}
