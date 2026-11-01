use anyhow::Result;
use tracing::info;

#[tokio::main]
async fn main() -> Result<()> {
    // Logging is off unless RUST_LOG is set. Logs go to stderr so they never mix with
    // the TUI on stdout; redirect stderr to a file (see `just run-debug`).
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("off")),
        )
        .init();

    info!("Starting az9s...");

    // Initialize application
    let app = az9s_core::App::new().await?;

    // Run TUI
    az9s_tui::run(app).await?;

    info!("az9s shutdown complete");
    Ok(())
}
