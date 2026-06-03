use crossterm::event::{self, Event as CrosstermEvent, KeyEvent};
use std::time::Duration;
use tokio::time::interval;

/// Application events
#[derive(Debug, Clone)]
pub enum Event {
    /// Key press event
    Key(KeyEvent),
    /// Periodic tick
    Tick,
    /// Terminal resize
    Resize(u16, u16),
}

/// Event handler for terminal events
pub struct EventHandler {
    tick_rate: Duration,
}

impl EventHandler {
    /// Create a new event handler
    pub fn new(tick_rate_ms: u64) -> Self {
        Self {
            tick_rate: Duration::from_millis(tick_rate_ms),
        }
    }

    /// Get the next event
    pub async fn next(&mut self) -> Option<Event> {
        let mut interval = interval(self.tick_rate);
        
        loop {
            tokio::select! {
                _ = interval.tick() => {
                    // Check for terminal events without blocking
                    if event::poll(Duration::from_millis(0)).ok()? {
                        match event::read().ok()? {
                            CrosstermEvent::Key(key) => return Some(Event::Key(key)),
                            CrosstermEvent::Resize(w, h) => return Some(Event::Resize(w, h)),
                            _ => continue,
                        }
                    } else {
                        return Some(Event::Tick);
                    }
                }
            }
        }
    }
}
