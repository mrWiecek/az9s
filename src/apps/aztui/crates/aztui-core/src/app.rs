use crate::{cache::Cache, state::AppState};
use anyhow::Context;
use std::path::PathBuf;
use tracing::info;

/// Main application coordinator
pub struct App {
    state: AppState,
    cache: Cache,
}

impl App {
    /// Create a new application instance
    pub async fn new() -> anyhow::Result<Self> {
        info!("Initializing aztui application");

        // Determine cache directory
        let cache_dir = Self::cache_dir()?;
        std::fs::create_dir_all(&cache_dir)
            .context("Failed to create cache directory")?;

        // Initialize cache
        let cache_path = cache_dir.join("aztui.db");
        let cache = Cache::new(&cache_path)?;

        // Initialize state
        let state = AppState::new();

        Ok(Self { state, cache })
    }

    /// Get the application state
    pub fn state(&self) -> &AppState {
        &self.state
    }

    /// Get mutable application state
    pub fn state_mut(&mut self) -> &mut AppState {
        &mut self.state
    }

    /// Get the cache
    pub fn cache(&self) -> &Cache {
        &self.cache
    }

    /// Determine the cache directory
    fn cache_dir() -> anyhow::Result<PathBuf> {
        let base = dirs::data_local_dir()
            .or_else(|| dirs::home_dir().map(|h| h.join(".local/share")))
            .context("Could not determine data directory")?;
        
        Ok(base.join("aztui"))
    }
}

// Add dirs crate for cross-platform directory detection
// This is a minimal implementation - we'll add the dependency later
mod dirs {
    use std::path::PathBuf;

    pub fn data_local_dir() -> Option<PathBuf> {
        std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| home_dir().map(|h| h.join(".local/share")))
    }

    pub fn home_dir() -> Option<PathBuf> {
        std::env::var_os("HOME").map(PathBuf::from)
    }
}
