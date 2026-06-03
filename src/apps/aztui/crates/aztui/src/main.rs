use anyhow::Result;
use tracing::info;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    info!("Starting aztui...");

    // Initialize application
    let app = aztui_core::App::new().await?;
    
    // Run TUI
    aztui_tui::run(app).await?;

    info!("aztui shutdown complete");
    Ok(())
}
