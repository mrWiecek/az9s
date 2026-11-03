pub mod app;
pub mod cache;
pub mod complete;
pub mod detail;
pub mod export;
pub mod fuzzy;
pub mod loader;
pub mod models;
pub mod palette;
pub mod query;
pub mod resource_type;
pub mod scope_tree;
pub mod state;
/// Timing stats (defined in az9s-azure so its HTTP clients can record too).
pub use az9s_azure::timing;

pub use app::App;
pub use state::AppState;

use thiserror::Error;

#[derive(Error, Debug)]
pub enum CoreError {
    #[error("Cache error: {0}")]
    Cache(#[from] rusqlite::Error),

    #[error("Azure error: {0}")]
    Azure(#[from] az9s_azure::AzureError),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Invalid state: {0}")]
    InvalidState(String),
}

pub type Result<T> = std::result::Result<T, CoreError>;
