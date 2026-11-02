use anyhow::Result;
use az9s_core::timing;
use tracing::info;

const USAGE: &str = "\
az9s - keyboard-first Azure terminal UI for platform engineers

Usage: az9s [OPTIONS]

Options:
      --timings   Print a timing summary (Azure calls, cache, frames) on exit
  -h, --help      Print help
  -V, --version   Print version

Authentication uses your Azure CLI login (`az login`).
Set RUST_LOG=debug and redirect stderr to a file to capture logs.";

#[tokio::main]
async fn main() -> Result<()> {
    timing::mark_process_start();

    let mut show_timings = false;
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "--timings" => show_timings = true,
            "-h" | "--help" => {
                println!("{USAGE}");
                return Ok(());
            }
            "-V" | "--version" => {
                println!(
                    "az9s {} ({})",
                    env!("CARGO_PKG_VERSION"),
                    env!("AZ9S_COMMIT")
                );
                return Ok(());
            }
            other => {
                eprintln!("error: unknown argument '{other}'\n\n{USAGE}");
                std::process::exit(2);
            }
        }
    }

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

    let app = az9s_core::App::new().await?;
    let result = az9s_tui::run(app).await;

    // Printed after the terminal is restored, so it is readable.
    if show_timings {
        eprintln!("\n{}", timing::summary());
    }

    info!("az9s shutdown complete");
    result
}
