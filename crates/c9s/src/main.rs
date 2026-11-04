use anyhow::{Context, Result};
use c9s_core::config::{Config, TEMPLATE};
use c9s_core::timing;
use std::path::PathBuf;
use tracing::info;

const USAGE: &str = "\
c9s - keyboard-first Azure terminal UI for platform engineers

Usage: c9s [OPTIONS]

Options:
      --config <PATH>  Use this config file (default: see below, or C9S_CONFIG)
      --init-config    Write a commented config file with the defaults, then exit
      --timings        Print a timing summary (Azure calls, cache, frames) on exit
  -h, --help           Print help
  -V, --version        Print version

Authentication uses your Azure CLI login (`az login`).
Set RUST_LOG=debug and redirect stderr to a file to capture logs.";

#[tokio::main]
async fn main() -> Result<()> {
    timing::mark_process_start();

    let mut show_timings = false;
    let mut config_path: Option<PathBuf> = None;
    let mut init_config = false;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--timings" => show_timings = true,
            "--init-config" => init_config = true,
            "--config" => match args.next() {
                Some(path) => config_path = Some(PathBuf::from(path)),
                None => {
                    eprintln!("error: --config needs a path\n\n{USAGE}");
                    std::process::exit(2);
                }
            },
            "-h" | "--help" => {
                println!("{USAGE}");
                match Config::path(None) {
                    Some(path) => println!("\nConfig file: {}", path.display()),
                    None => println!("\nConfig file: (no config directory found)"),
                }
                return Ok(());
            }
            "-V" | "--version" => {
                println!("c9s {} ({})", env!("CARGO_PKG_VERSION"), env!("C9S_COMMIT"));
                return Ok(());
            }
            other => {
                eprintln!("error: unknown argument '{other}'\n\n{USAGE}");
                std::process::exit(2);
            }
        }
    }

    let path = Config::path(config_path.as_deref());
    if init_config {
        return write_config_template(path);
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

    info!("Starting c9s...");

    let app = c9s_core::App::start(Config::load(path)).await?;
    let result = c9s_tui::run(app).await;

    // Printed after the terminal is restored, so it is readable.
    if show_timings {
        eprintln!("\n{}", timing::summary());
    }

    info!("c9s shutdown complete");
    result
}

/// `--init-config`: write the commented defaults, never over an existing file.
fn write_config_template(path: Option<PathBuf>) -> Result<()> {
    let path = path.context("no config directory found; use --config <PATH>")?;
    if path.exists() {
        println!("{} already exists; not overwritten.", path.display());
        return Ok(());
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    }
    std::fs::write(&path, TEMPLATE).with_context(|| format!("writing {}", path.display()))?;
    println!("Wrote {}", path.display());
    Ok(())
}
