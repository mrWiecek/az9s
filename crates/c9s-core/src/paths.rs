//! Where c9s keeps its files, and what carries over from az9s (its name
//! before v1.0.0): the config and cache directories move to the new name on
//! first start, and `AZ9S_*` environment variables are still read.

use directories::ProjectDirs;
use std::path::Path;
use tracing::{info, warn};

/// c9s's directories (config, cache). The first call moves az9s's over if
/// c9s has none yet.
pub fn project_dirs() -> Option<ProjectDirs> {
    let dirs = ProjectDirs::from("", "", "c9s")?;
    if let Some(old) = ProjectDirs::from("", "", "az9s") {
        move_if_new(old.config_dir(), dirs.config_dir());
        move_if_new(old.data_local_dir(), dirs.data_local_dir());
    }
    Some(dirs)
}

/// Move `old` to `new` when `old` exists and `new` doesn't.
fn move_if_new(old: &Path, new: &Path) {
    if old == new || !old.exists() || new.exists() {
        return;
    }
    if let Some(parent) = new.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    match std::fs::rename(old, new) {
        Ok(()) => info!("Moved {} to {}", old.display(), new.display()),
        Err(e) => warn!(
            "Could not move {} to {}: {}",
            old.display(),
            new.display(),
            e
        ),
    }
}

/// An environment variable by its c9s name (`C9S_…`), else by its az9s
/// name (`AZ9S_…`), so existing setups keep working.
pub fn env_var(name: &str) -> Option<std::ffi::OsString> {
    let old = name.replacen("C9S_", "AZ9S_", 1);
    std::env::var_os(name)
        .filter(|v| !v.is_empty())
        .or_else(|| std::env::var_os(old).filter(|v| !v.is_empty()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_old_directory_moves_once() {
        let dir = tempfile::tempdir().unwrap();
        let old = dir.path().join("az9s");
        let new = dir.path().join("nested/c9s");
        std::fs::create_dir_all(&old).unwrap();
        std::fs::write(old.join("config.toml"), "theme = \"nord\"").unwrap();
        move_if_new(&old, &new);
        assert!(!old.exists());
        assert_eq!(
            std::fs::read_to_string(new.join("config.toml")).unwrap(),
            "theme = \"nord\""
        );
        // An existing new directory is never overwritten.
        std::fs::create_dir_all(&old).unwrap();
        std::fs::write(old.join("config.toml"), "old").unwrap();
        move_if_new(&old, &new);
        assert!(old.exists(), "left alone");
        assert_eq!(
            std::fs::read_to_string(new.join("config.toml")).unwrap(),
            "theme = \"nord\""
        );
    }

    #[test]
    fn old_variable_names_still_work() {
        // Names no other test uses: the environment is shared.
        std::env::set_var("AZ9S_PATHS_TEST_ONLY_OLD", "1");
        assert_eq!(env_var("C9S_PATHS_TEST_ONLY_OLD").unwrap(), "1");
        std::env::set_var("C9S_PATHS_TEST_ONLY_BOTH", "new");
        std::env::set_var("AZ9S_PATHS_TEST_ONLY_BOTH", "old");
        assert_eq!(
            env_var("C9S_PATHS_TEST_ONLY_BOTH").unwrap(),
            "new",
            "new name wins"
        );
        assert!(env_var("C9S_PATHS_TEST_ONLY_NEITHER").is_none());
    }
}
