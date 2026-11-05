//! Signing in to Azure: tokens from the Azure CLI (`az login`).

use crate::{AzureError, Result};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tracing::info;

/// Where Azure tokens come from.
pub trait AuthProvider: Send + Sync {
    fn credential(&self) -> Arc<CliCredential>;
}

/// The Azure CLI's login.
pub struct AzureAuth {
    credential: Arc<CliCredential>,
}

impl AzureAuth {
    /// Tokens from the Azure CLI's login.
    pub fn new_cli() -> Result<Self> {
        info!("Initializing Azure CLI authentication");
        Ok(Self {
            credential: Arc::new(CliCredential::default()),
        })
    }

    /// The Azure CLI's login (the only sign-in c9s uses).
    pub async fn new_auto() -> Result<Self> {
        Self::new_cli()
    }
}

impl AuthProvider for AzureAuth {
    fn credential(&self) -> Arc<CliCredential> {
        self.credential.clone()
    }
}

/// An access token and when it expires (Unix seconds).
#[derive(Clone)]
pub struct Token {
    secret: String,
    pub expires_on: u64,
}

impl Token {
    pub fn secret(&self) -> &str {
        &self.secret
    }
}

impl std::fmt::Debug for Token {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Never the token itself.
        f.debug_struct("Token")
            .field("expires_on", &self.expires_on)
            .finish_non_exhaustive()
    }
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_secs()
}

/// Tokens from `az account get-access-token`, started safely: `az` is
/// looked up on the PATH only and, on Windows, run by full path instead of
/// through `cmd /C` (see [`c9s_exec`]). Tokens are cached per scope until
/// five minutes before they expire.
#[derive(Default)]
pub struct CliCredential {
    cache: tokio::sync::Mutex<HashMap<String, Token>>,
}

impl std::fmt::Debug for CliCredential {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("CliCredential")
    }
}

/// The access token and its expiry from `az account get-access-token`.
fn parse_cli_token(json: &str) -> Result<Token> {
    let v: serde_json::Value = serde_json::from_str(json).map_err(|e| {
        AzureError::AuthenticationFailed(format!(
            "unreadable `az account get-access-token` output: {e}"
        ))
    })?;
    let secret = v
        .get("accessToken")
        .and_then(|t| t.as_str())
        .filter(|t| !t.is_empty())
        .ok_or_else(|| {
            AzureError::AuthenticationFailed(
                "`az account get-access-token` returned no token".into(),
            )
        })?;
    // `expires_on` (Unix seconds) since Azure CLI 2.54; older ones only
    // give a local time, so assume ten minutes.
    let expires_on = v
        .get("expires_on")
        .and_then(|e| e.as_u64().or_else(|| e.as_str()?.parse().ok()))
        .unwrap_or_else(|| now() + 600);
    Ok(Token {
        secret: secret.to_string(),
        expires_on,
    })
}

impl CliCredential {
    /// A token for `scope` (e.g. `https://management.azure.com/.default`).
    pub async fn get_token(&self, scope: &str) -> Result<Token> {
        let mut cache = self.cache.lock().await;
        if let Some(token) = cache.get(scope) {
            if token.expires_on > now() + 300 {
                return Ok(token.clone());
            }
        }
        let args = [
            "account",
            "get-access-token",
            "--output",
            "json",
            "--scope",
            scope,
        ];
        let output = crate::cli::az_command("az", &args)
            .map_err(|e| AzureError::AuthenticationFailed(crate::cli::start_error(&e)))?
            .output()
            .await
            .map_err(|e| AzureError::AuthenticationFailed(crate::cli::start_error(&e)))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let first = stderr
                .lines()
                .find(|l| !l.trim().is_empty())
                .unwrap_or("unknown error")
                .trim()
                .to_string();
            return Err(AzureError::AuthenticationFailed(format!(
                "`az account get-access-token` failed: {first}; run `az login`"
            )));
        }
        let token = parse_cli_token(&String::from_utf8_lossy(&output.stdout))?;
        cache.insert(scope.to_string(), token.clone());
        Ok(token)
    }
}

#[cfg(test)]
mod cli_credential_tests {
    use super::*;

    #[test]
    fn tokens_and_expiry_come_from_the_cli_json() {
        let t = parse_cli_token(
            r#"{"accessToken":"eyJ0","expiresOn":"2024-01-01 19:23:16.000000","expires_on":1704158596,"tenant":"t","tokenType":"Bearer"}"#,
        )
        .unwrap();
        assert_eq!(t.secret(), "eyJ0");
        assert_eq!(t.expires_on, 1704158596);
        assert!(!format!("{t:?}").contains("eyJ"), "Debug hides the token");
        let old =
            parse_cli_token(r#"{"accessToken":"x","expiresOn":"2024-01-01 19:23:16.000000"}"#)
                .unwrap();
        assert!(old.expires_on > now());
        assert!(parse_cli_token(r#"{"error":"no"}"#).is_err());
        assert!(!format!("{:?}", CliCredential::default()).contains("eyJ"));
    }
}
