use crossterm::event::{self, Event as CrosstermEvent, KeyEvent, KeyEventKind, MouseEvent};
use std::time::Duration;
use tokio::sync::mpsc;
use tracing::{debug, warn};

/// Application events
#[derive(Debug, Clone)]
pub enum Event {
    /// Key press event
    Key(KeyEvent),
    /// Mouse event
    Mouse(MouseEvent),
    /// Periodic tick (no input arrived within the tick interval)
    Tick,
    /// Terminal resize
    Resize(u16, u16),
    /// Error occurred in event handling
    Error(String),
}

/// Event handler for terminal events.
///
/// A dedicated OS thread blocks on terminal input and forwards **every** event
/// as soon as it arrives. If no input arrives within `tick_rate`, a
/// [`Event::Tick`] is sent instead, so the UI can redraw (spinners, data age).
///
/// The previous implementation polled once per tick and forwarded at most one
/// event per tick, which capped input at ~4 keys/second and made navigation lag.
pub struct EventHandler {
    rx: mpsc::UnboundedReceiver<Event>,
}

impl EventHandler {
    /// Create a new event handler.
    ///
    /// # Arguments
    /// * `tick_rate_ms` - Milliseconds of input inactivity before a tick is sent
    pub fn new(tick_rate_ms: u64) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        let tick_rate = Duration::from_millis(tick_rate_ms);

        std::thread::Builder::new()
            .name("c9s-input".into())
            .spawn(move || Self::reader_loop(tx, tick_rate))
            .expect("failed to spawn input thread");

        Self { rx }
    }

    fn reader_loop(tx: mpsc::UnboundedSender<Event>, tick_rate: Duration) {
        loop {
            let event = match event::poll(tick_rate) {
                Ok(true) => match event::read() {
                    // Windows reports key release events too; only act on presses.
                    Ok(CrosstermEvent::Key(key)) if key.kind != KeyEventKind::Release => {
                        Event::Key(key)
                    }
                    Ok(CrosstermEvent::Mouse(mouse)) => Event::Mouse(mouse),
                    Ok(CrosstermEvent::Resize(w, h)) => Event::Resize(w, h),
                    Ok(_) => continue,
                    Err(e) => {
                        warn!("Error reading terminal event: {}", e);
                        Event::Error(e.to_string())
                    }
                },
                Ok(false) => Event::Tick,
                Err(e) => {
                    warn!("Error polling terminal events: {}", e);
                    Event::Error(e.to_string())
                }
            };

            let is_error = matches!(event, Event::Error(_));
            if tx.send(event).is_err() {
                debug!("Event receiver dropped, stopping input thread");
                return;
            }
            if is_error {
                // Avoid a hot loop if the terminal is persistently failing.
                std::thread::sleep(tick_rate);
            }
        }
    }

    /// Wait for the next event.
    ///
    /// Returns `None` if the event channel is closed.
    pub async fn next(&mut self) -> Option<Event> {
        self.rx.recv().await
    }

    /// Return an already-queued event without waiting.
    ///
    /// Used to drain all pending input before the next redraw, so holding a key
    /// moves the selection as fast as the terminal repeats it.
    pub fn try_next(&mut self) -> Option<Event> {
        self.rx.try_recv().ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_event_handler_creation() {
        let mut handler = EventHandler::new(50);

        // Without a terminal we still receive ticks or errors, never silence.
        tokio::time::timeout(Duration::from_secs(1), async {
            let event = handler.next().await;
            assert!(event.is_some());
        })
        .await
        .expect("Should receive event within timeout");
    }
}
