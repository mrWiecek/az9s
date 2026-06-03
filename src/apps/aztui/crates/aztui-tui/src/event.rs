use crossterm::event::{self, Event as CrosstermEvent, KeyEvent, MouseEvent};
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::interval;
use tracing::{debug, warn};

/// Application events
#[derive(Debug, Clone)]
pub enum Event {
    /// Key press event
    Key(KeyEvent),
    /// Mouse event
    Mouse(MouseEvent),
    /// Periodic tick
    Tick,
    /// Terminal resize
    Resize(u16, u16),
    /// Error occurred in event handling
    Error(String),
}

/// Event handler for terminal events
/// 
/// This handler runs a background task that polls for terminal events
/// and sends them through a channel. It also generates periodic tick events.
pub struct EventHandler {
    rx: mpsc::UnboundedReceiver<Event>,
    #[allow(dead_code)]
    _tx: mpsc::UnboundedSender<Event>,
}

impl EventHandler {
    /// Create a new event handler
    /// 
    /// # Arguments
    /// * `tick_rate_ms` - Milliseconds between tick events
    pub fn new(tick_rate_ms: u64) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        let event_tx = tx.clone();
        
        // Spawn event reader task
        tokio::spawn(async move {
            Self::event_reader_task(event_tx, tick_rate_ms).await;
        });
        
        Self {
            rx,
            _tx: tx,
        }
    }

    /// Background task that reads terminal events
    async fn event_reader_task(tx: mpsc::UnboundedSender<Event>, tick_rate_ms: u64) {
        let mut tick_interval = interval(Duration::from_millis(tick_rate_ms));
        
        loop {
            tokio::select! {
                _ = tick_interval.tick() => {
                    // Check for terminal events without blocking
                    match event::poll(Duration::from_millis(0)) {
                        Ok(true) => {
                            match event::read() {
                                Ok(CrosstermEvent::Key(key)) => {
                                    if tx.send(Event::Key(key)).is_err() {
                                        debug!("Event receiver dropped, stopping event reader");
                                        break;
                                    }
                                }
                                Ok(CrosstermEvent::Mouse(mouse)) => {
                                    if tx.send(Event::Mouse(mouse)).is_err() {
                                        break;
                                    }
                                }
                                Ok(CrosstermEvent::Resize(w, h)) => {
                                    if tx.send(Event::Resize(w, h)).is_err() {
                                        break;
                                    }
                                }
                                Ok(_) => {
                                    // Ignore other events
                                }
                                Err(e) => {
                                    warn!("Error reading terminal event: {}", e);
                                    if tx.send(Event::Error(e.to_string())).is_err() {
                                        break;
                                    }
                                }
                            }
                        }
                        Ok(false) => {
                            // No events available, send tick
                            if tx.send(Event::Tick).is_err() {
                                break;
                            }
                        }
                        Err(e) => {
                            warn!("Error polling terminal events: {}", e);
                            if tx.send(Event::Error(e.to_string())).is_err() {
                                break;
                            }
                        }
                    }
                }
            }
        }
        
        debug!("Event reader task stopped");
    }

    /// Get the next event
    /// 
    /// Returns `None` if the event channel is closed
    pub async fn next(&mut self) -> Option<Event> {
        self.rx.recv().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_event_handler_creation() {
        let mut handler = EventHandler::new(250);
        
        // Should receive tick events
        tokio::time::timeout(Duration::from_secs(1), async {
            let event = handler.next().await;
            assert!(event.is_some());
        })
        .await
        .expect("Should receive event within timeout");
    }
}
