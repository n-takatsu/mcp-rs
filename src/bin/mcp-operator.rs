// Kubernetes Operator binary
use anyhow::Result;
use kube::Client;
use tracing::{error, info};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

#[cfg(feature = "kubernetes-operator")]
use mcp_rs::operator;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| "info,mcp_operator=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    info!("Starting MCP Kubernetes Operator");

    #[cfg(feature = "kubernetes-operator")]
    {
        // Create Kubernetes client
        let client = Client::try_default()
            .await
            .expect("Failed to create Kubernetes client");

        info!("Connected to Kubernetes cluster");

        // Run the controller
        if let Err(e) = operator::run_mcpserver_controller(client).await {
            error!("Controller error: {}", e);
            std::process::exit(1);
        }
    }

    #[cfg(not(feature = "kubernetes-operator"))]
    {
        error!("Kubernetes operator feature not enabled. Please build with --features kubernetes-operator");
        return Err(anyhow::anyhow!("kubernetes-operator feature required"));
    }

    #[cfg(feature = "kubernetes-operator")]
    Ok(())
}
