pub mod app;
pub mod command;
pub mod event;
pub mod ui;

use anyhow::{Context, Result};
use az9s_core::{timing, App};
use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io;
use std::panic;
use tracing::{debug, error, info, warn};

pub use app::TuiApp;
pub use command::{Command, CommandKind, Scope};
pub use event::{Event, EventHandler};

/// Run the TUI application
pub async fn run(core_app: App) -> Result<()> {
    info!("Starting TUI");

    // Setup panic hook to restore terminal on panic
    let original_hook = panic::take_hook();
    panic::set_hook(Box::new(move |panic_info| {
        let _ = restore_terminal();
        original_hook(panic_info);
    }));

    // Setup terminal and run
    let result = run_with_terminal(core_app).await;

    // Always restore terminal, even on error
    if let Err(e) = restore_terminal() {
        error!("Failed to restore terminal: {}", e);
    }

    result
}

/// Run the application with terminal setup
async fn run_with_terminal(core_app: App) -> Result<()> {
    let mut terminal = setup_terminal()?;

    // Create TUI app
    let mut tui_app = TuiApp::new(core_app);

    // Create event handler
    let mut event_handler = EventHandler::new(250);

    // Run the main loop
    run_loop(&mut terminal, &mut tui_app, &mut event_handler).await
}

/// Setup the terminal for TUI rendering
fn setup_terminal() -> Result<Terminal<CrosstermBackend<io::Stdout>>> {
    enable_raw_mode().context("Failed to enable raw mode")?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)
        .context("Failed to enter alternate screen")?;

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).context("Failed to create terminal")?;
    terminal.hide_cursor().context("Failed to hide cursor")?;

    Ok(terminal)
}

/// Restore the terminal to its original state
fn restore_terminal() -> Result<()> {
    disable_raw_mode().context("Failed to disable raw mode")?;
    let mut stdout = io::stdout();
    execute!(stdout, LeaveAlternateScreen, DisableMouseCapture)
        .context("Failed to leave alternate screen")?;

    Ok(())
}

/// Main event loop: draw, wait for input, then drain everything queued before
/// drawing again. Draining means a burst of key repeats costs one redraw.
async fn run_loop<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    app: &mut TuiApp,
    event_handler: &mut EventHandler,
) -> Result<()> {
    let mut first_frame = true;
    loop {
        draw_frame(terminal, app)?;
        if first_frame {
            first_frame = false;
            timing::record("startup.first_frame", timing::since_process_start());
        }

        let Some(event) = event_handler.next().await else {
            debug!("Event channel closed, exiting");
            return Ok(());
        };
        if !process_event(app, event).await {
            return Ok(());
        }
        while let Some(event) = event_handler.try_next() {
            if matches!(event, Event::Tick) {
                continue;
            }
            if !process_event(app, event).await {
                return Ok(());
            }
        }
    }
}

/// Render one frame. Returns an error only if the terminal is gone.
fn draw_frame<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    app: &mut TuiApp,
) -> Result<()> {
    let _t = timing::timer("ui.frame");
    if let Err(e) = terminal.draw(|f| {
        if let Err(draw_err) = ui::draw(f, app) {
            warn!("Draw error: {}", draw_err);
            app.state_mut()
                .set_error(Some(format!("Render error: {}", draw_err)));
        }
    }) {
        error!("Terminal draw error: {}", e);
        if e.kind() == io::ErrorKind::BrokenPipe {
            info!("Terminal closed, exiting");
            return Err(e.into());
        }
    }
    Ok(())
}

/// Handle one event. Returns `false` when the app should exit.
async fn process_event(app: &mut TuiApp, event: Event) -> bool {
    if let Event::Error(err) = event {
        warn!("Event handler error: {}", err);
        app.state_mut()
            .set_error(Some(format!("Event error: {}", err)));
        return true;
    }
    let _t = matches!(event, Event::Key(_)).then(|| timing::timer("ui.handle_key"));
    match app.handle_event(event).await {
        Ok(should_continue) => {
            if !should_continue {
                info!("Exiting TUI");
            }
            should_continue
        }
        Err(e) => {
            error!("Error handling event: {}", e);
            app.state_mut()
                .set_error(Some(format!("Event handling error: {}", e)));
            true
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_terminal_lifecycle() {
        // Test that we can setup and restore terminal without panicking
        // This is a basic smoke test
        let result = setup_terminal();
        if let Ok(_terminal) = result {
            let restore_result = restore_terminal();
            assert!(restore_result.is_ok());
        }
    }
}
