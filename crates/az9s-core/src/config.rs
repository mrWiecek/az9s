//! The config file: `config.toml` in the platform config directory
//! (`~/.config/az9s/config.toml` on Linux and WSL), or the path in
//! `AZ9S_CONFIG` / `--config`.
//!
//! The file is optional, and a broken one never stops az9s: it starts with
//! the defaults and says what is wrong in the status bar.

use serde::Deserialize;
use std::path::{Path, PathBuf};

/// Shortest auto-refresh interval: protects the Resource Graph quota.
pub const MIN_REFRESH_SECS: u64 = 30;

/// A commented config file with every setting at its default, written by
/// `az9s --init-config`.
pub const TEMPLATE: &str = r#"# az9s configuration. Every setting is optional; these are the defaults.

# The first view: "mg" (management group tree) or "sub" (subscriptions).
start = "mg"

# Subscription (name or ID) that :rg, :res, :type and :tag use when there is
# no context (no subscription or management group highlighted or open).
# Without it, they cover all subscriptions.
# default_subscription = "prod-connectivity"

# Refresh the current view from Azure every this many seconds (at least 30).
# 0 turns auto-refresh off; r refreshes by hand.
refresh_interval = 0

# Tenant (ID) to start in, as if you typed :ctx <id>. Without it az9s uses
# the Azure CLI's default login and lists every tenant's subscriptions.
# tenant = "00000000-0000-0000-0000-000000000000"

# Read-only mode: actions that change Azure (planned, M4) are disabled.
# The status bar shows RO while it is on.
read_only = true
"#;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StartView {
    /// Management group tree
    Mg,
    /// Subscriptions
    Sub,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub start: StartView,
    pub default_subscription: Option<String>,
    /// Tenant ID to start in (`:ctx`).
    pub tenant: Option<String>,
    /// Seconds; 0 = off, otherwise at least [`MIN_REFRESH_SECS`].
    pub refresh_interval: u64,
    pub read_only: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            start: StartView::Mg,
            default_subscription: None,
            tenant: None,
            refresh_interval: 0,
            read_only: true,
        }
    }
}

/// A loaded config, and a problem to show if the file couldn't be used.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Loaded {
    pub config: Config,
    pub path: Option<PathBuf>,
    pub problem: Option<String>,
}

impl Config {
    /// Parse a config file's text.
    pub fn parse(text: &str) -> Result<Self, String> {
        let mut config: Config = toml::from_str(text).map_err(|e| {
            // toml's message is several lines: "...at line N, column M",
            // the offending line, then what is wrong. Keep it to one line.
            let full = e.to_string();
            let what = full
                .lines()
                .rev()
                .find(|l| !l.trim().is_empty())
                .unwrap_or("invalid TOML")
                .trim();
            let line = full
                .lines()
                .next()
                .and_then(|l| l.split("at line ").nth(1))
                .and_then(|l| l.split(',').next());
            match line {
                Some(n) => format!("line {n}: {what}"),
                None => what.to_string(),
            }
        })?;
        if config.refresh_interval > 0 {
            config.refresh_interval = config.refresh_interval.max(MIN_REFRESH_SECS);
        }
        let clean = |s: Option<String>| s.map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
        config.default_subscription = clean(config.default_subscription);
        config.tenant = clean(config.tenant).map(|t| t.to_ascii_lowercase());
        Ok(config)
    }

    /// The config file's path: `explicit` (from `--config`), else
    /// `AZ9S_CONFIG`, else the platform config directory.
    pub fn path(explicit: Option<&Path>) -> Option<PathBuf> {
        if let Some(path) = explicit {
            return Some(path.to_path_buf());
        }
        if let Some(path) = std::env::var_os("AZ9S_CONFIG").filter(|p| !p.is_empty()) {
            return Some(PathBuf::from(path));
        }
        directories::ProjectDirs::from("", "", "az9s").map(|d| d.config_dir().join("config.toml"))
    }

    /// Load the config at `path`. A missing file gives the defaults; an
    /// unreadable or invalid one gives the defaults and a problem.
    pub fn load(path: Option<PathBuf>) -> Loaded {
        let Some(path) = path else {
            return Loaded::default();
        };
        let (config, problem) = match std::fs::read_to_string(&path) {
            Ok(text) => match Config::parse(&text) {
                Ok(config) => (config, None),
                Err(e) => (
                    Config::default(),
                    Some(format!("{}: {e}; using defaults", path.display())),
                ),
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (Config::default(), None),
            Err(e) => (
                Config::default(),
                Some(format!("{}: {e}; using defaults", path.display())),
            ),
        };
        Loaded {
            config,
            path: Some(path),
            problem,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_template_parses_to_the_defaults() {
        assert_eq!(Config::parse(TEMPLATE).unwrap(), Config::default());
        assert_eq!(Config::parse("").unwrap(), Config::default());
    }

    #[test]
    fn settings_are_read_and_checked() {
        let config = Config::parse(
            "start = \"sub\"\ndefault_subscription = \" prod \"\nrefresh_interval = 5\nread_only = false\n",
        )
        .unwrap();
        assert_eq!(config.start, StartView::Sub);
        assert_eq!(config.default_subscription.as_deref(), Some("prod"));
        let with_tenant = Config::parse("tenant = \" AB-CD \"").unwrap();
        assert_eq!(with_tenant.tenant.as_deref(), Some("ab-cd"));
        assert_eq!(
            config.refresh_interval, MIN_REFRESH_SECS,
            "raised to the minimum"
        );
        assert!(!config.read_only);
        assert_eq!(
            Config::parse("default_subscription = \"\"")
                .unwrap()
                .default_subscription,
            None
        );
    }

    #[test]
    fn mistakes_are_explained() {
        let e = Config::parse("start = \"mg\"\nrefresh_intervall = 60").unwrap_err();
        assert!(
            e.starts_with("line 2: unknown field `refresh_intervall`, expected one of"),
            "{e}"
        );
        let e = Config::parse("start = \"tree\"").unwrap_err();
        assert!(e.contains("unknown variant `tree`"), "{e}");
        let e = Config::parse("refresh_interval = \"often\"").unwrap_err();
        assert!(e.contains("invalid type"), "{e}");
    }

    #[test]
    fn loading_missing_broken_and_valid_files() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");

        let loaded = Config::load(Some(path.clone()));
        assert_eq!(loaded.config, Config::default());
        assert!(loaded.problem.is_none(), "a missing file is fine");

        std::fs::write(&path, "read_only = maybe").unwrap();
        let loaded = Config::load(Some(path.clone()));
        assert_eq!(loaded.config, Config::default());
        let problem = loaded.problem.unwrap();
        assert!(
            problem.contains("config.toml") && problem.contains("using defaults"),
            "{problem}"
        );

        std::fs::write(&path, "start = \"sub\"").unwrap();
        assert_eq!(Config::load(Some(path)).config.start, StartView::Sub);
    }
}
