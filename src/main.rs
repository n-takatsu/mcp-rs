mod mcp;
mod handlers;

use std::sync::Arc;
use mcp::{McpServer};
use handlers::WordPressHandler;
use tracing_subscriber;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt::init();

    // Create MCP server
    let mut server = McpServer::new();

    // Add WordPress handler
    let wordpress_handler = WordPressHandler::new(
        std::env::var("WORDPRESS_URL").unwrap_or_else(|_| "http://localhost".to_string()),
        std::env::var("WORDPRESS_USERNAME").ok(),
        std::env::var("WORDPRESS_PASSWORD").ok(),
    );

    server.add_handler("wordpress".to_string(), Arc::new(wordpress_handler));

    // Run server on stdio (for MCP clients)
    if std::env::var("MCP_STDIO").is_ok() {
        server.run_stdio().await?;
    } else {
        // Run server on TCP (for development/testing)
        let addr = std::env::var("BIND_ADDR").unwrap_or_else(|_| "127.0.0.1:8080".to_string());
        server.run(&addr).await?;
    }

    Ok(())
}
