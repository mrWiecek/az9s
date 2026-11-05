//! Starting the cloud CLIs (`az`, `gcloud`, `aws`) safely.
//!
//! - The program is looked up on the PATH only: never in the current
//!   directory (an `az.cmd` planted in a cloned repository would otherwise
//!   run with the user's credentials) and never through a relative PATH
//!   entry.
//! - On Windows the CLIs are batch files (`az.cmd`). They are started by
//!   full path, so Rust's standard library quotes their arguments for
//!   `cmd.exe` (and refuses arguments it can't quote safely), rather than
//!   through `cmd /C`, which would let `&` or `|` in an argument start
//!   another command.
//! - Arguments are checked before anything runs: names that come from
//!   config files (profiles, tenants, configurations) may not carry
//!   control characters or shell metacharacters.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use tokio::process::Command;

/// Why a CLI could not be started.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// The program is not on the PATH.
    NotFound(String),
    /// An argument was refused (it could change what the command does).
    UnsafeArgument(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::NotFound(p) => write!(f, "{p} was not found on the PATH"),
            Error::UnsafeArgument(a) => write!(
                f,
                "refused to pass {a:?} to a command line: it contains a control or shell character"
            ),
        }
    }
}

impl std::error::Error for Error {}

/// Characters an argument may not contain: controls, and what a shell or
/// `cmd.exe` would treat specially.
fn unsafe_char(c: char) -> bool {
    c.is_control()
        || matches!(
            c,
            '&' | '|' | '<' | '>' | '^' | '%' | '"' | '!' | '`' | '$' | ';'
        )
}

/// Check one argument.
pub fn check_arg(arg: &str) -> Result<(), Error> {
    if arg.chars().any(unsafe_char) {
        Err(Error::UnsafeArgument(arg.to_string()))
    } else {
        Ok(())
    }
}

/// Executable names to try for `program` on this platform.
fn candidates(program: &str) -> Vec<String> {
    if cfg!(windows) {
        let ext = std::env::var("PATHEXT").unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".into());
        ext.split(';')
            .filter(|e| !e.is_empty())
            .map(|e| format!("{program}{}", e.to_ascii_lowercase()))
            .collect()
    } else {
        vec![program.to_string()]
    }
}

/// The full path of `program` from `path` (a PATH value), skipping empty
/// and relative entries (the current directory).
pub fn resolve_in(program: &str, path: Option<OsString>) -> Option<PathBuf> {
    let path = path?;
    let names = candidates(program);
    std::env::split_paths(&path)
        .filter(|dir| dir.is_absolute())
        .find_map(|dir| names.iter().map(|n| dir.join(n)).find(|p| p.is_file()))
}

/// The full path of `program` on the PATH.
pub fn resolve(program: &str) -> Option<PathBuf> {
    resolve_in(program, std::env::var_os("PATH"))
}

/// A command that runs `program` with `args`. A bare name is looked up on
/// the PATH; a path (tests' fake CLIs) is used as given. Every argument is
/// checked first.
pub fn command(program: &str, args: &[&str]) -> Result<Command, Error> {
    for arg in args {
        check_arg(arg)?;
    }
    let path = if Path::new(program).components().count() > 1 {
        PathBuf::from(program)
    } else {
        resolve(program).ok_or_else(|| Error::NotFound(program.to_string()))?
    };
    let mut command = Command::new(path);
    command.args(args);
    Ok(command)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_and_control_characters_are_refused() {
        for bad in [
            "prod&calc",
            "a|b",
            "x\"y",
            "%PATH%",
            "a\nb",
            "\x1b[0m",
            "a^b",
            "$(id)",
            "a;b",
        ] {
            assert!(check_arg(bad).is_err(), "{bad:?}");
        }
        for good in [
            "prod-admin",
            "my.profile_1",
            "contoso.onmicrosoft.com",
            "00000000-0000-0000-0000-000000000000",
            "--configuration=work",
            "https://management.azure.com/",
            "accessToken",
        ] {
            assert!(check_arg(good).is_ok(), "{good:?}");
        }
        assert!(matches!(
            command("az", &["--tenant", "t&calc"]),
            Err(Error::UnsafeArgument(_))
        ));
    }

    #[cfg(unix)]
    #[test]
    fn the_current_directory_is_never_searched() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let fake = dir.path().join("az");
        std::fs::write(&fake, "#!/bin/sh\n").unwrap();
        std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
        // Relative and empty entries (the current directory) are skipped.
        let cwd = std::env::current_dir().unwrap();
        std::env::set_current_dir(dir.path()).unwrap();
        let found = resolve_in("az", Some(OsString::from(":.:relative")));
        std::env::set_current_dir(cwd).unwrap();
        assert_eq!(found, None);
        // An absolute entry is used.
        let path = std::env::join_paths([dir.path()]).unwrap();
        assert_eq!(resolve_in("az", Some(path)), Some(fake));
        assert_eq!(resolve_in("az", None), None);
    }
}
