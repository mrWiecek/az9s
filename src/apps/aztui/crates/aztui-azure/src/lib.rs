pub mod auth;
pub mod client;
pub mod models;

pub use auth::{AuthProvider, AzureAuth};
pub use client::AzureClient;

use thiserror::Error;

#[derive(Error, Debug)]
pub enum AzureError {
    #[error("Authentication failed: {0}")]
    AuthenticationFailed(String),
    
    #[error("API request failed: {0}")]
    ApiError(String),
    
    #[error("Resource not found: {0}")]
    NotFound(String),
    
    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),
}

pub type Result<T> = std::result::Result<T, AzureError>;
