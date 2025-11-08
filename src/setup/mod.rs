//! Setup Module
//! 
//! Interactive configuration setup functionality

pub mod ui;
pub mod validator;

pub use ui::ConfigSetupUI;
pub use validator::ConfigValidator;

use crate::config::McpConfig;
use crate::error::Error;

/// Setup configuration interactively
pub async fn setup_config_interactive() -> Result<(), Error> {
    let mut ui = ConfigSetupUI::new();
    ui.run().await
}