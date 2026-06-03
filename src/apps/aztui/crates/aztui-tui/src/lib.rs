pub mod app;
pub mod event;
pub mod ui;

use anyhow::{Context, Result};
use aztui_core::App;
use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io;
use tracing::{debug, info};

pub use app::TuiApp;
pub use event::{Event, EventHandler};

/// Run the TUI application
pub async fn run(core_app: App) -> Result<()> {
    info!("Starting TUI");

    // Setup terminal
    enable_raw_mode().context("Failed to enable raw mode")?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)
        .context("Failed to enter alternate screen")?;
    
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).context("Failed to create terminal")?;

    // Create TUI app
    let mut tui_app = TuiApp::new(core_app);
    
    // Create event handler
    let mut event_handler = EventHandler::new(250);

    // Run the main loop
    let result = run_loop(&mut terminal, &mut tui_app, &mut event_handler).await;

    // Restore terminal
    disable_raw_mode().context("Failed to disable raw mode")?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )
    .context("Failed to leave alternate screen")?;
    terminal.show_cursor().context("Failed to show cursor")?;

    result
}

/// Main event loop
async fn run_loop<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    app: &mut TuiApp,
    event_handler: &mut EventHandler,
) -> Result<()> {
    loop {
        // Draw UI
        terminal.draw(|f| {
            if let Err(e) = ui::draw(f, app) {
                debug!("Draw error: {}", e);
            }
        })?;

        // Handle events
        if let Some(event) = event_handler.next().await {
            if !app.handle_event(event).await? {
                info!("Exiting TUI");
                break;
            }
        }
    }

    Ok(())
}
