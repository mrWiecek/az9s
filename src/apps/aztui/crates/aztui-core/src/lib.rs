pub mod app;
pub mod cache;
pub mod models;
pub mod state;

pub use app::App;
pub use state::AppState;

use thiserror::Error;

#[derive(Error, Debug)]
pub enum CoreError {
    #[error("Cache error: {0}")]
    Cache(#[from] rusqlite::Error),
    
    #[error("Azure error: {0}")]
    Azure(#[from] aztui_azure::AzureError),
    
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    
    #[error("Invalid state: {0}")]
    InvalidState(String),
}

pub type Result<T> = std::result::Result<T, CoreError>;
