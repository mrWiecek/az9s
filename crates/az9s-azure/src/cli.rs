//! Running the Azure CLI.
//!
//! On Windows `az` is a batch file (`az.cmd`). `std::process::Command` only
//! starts `.exe` files by bare name there, so `Command::new("az")` fails with
//! "program not found" even though `az` works in a shell. Like the Azure
//! SDK, run it through `cmd /C` on Windows.

use tokio::process::Command;

/// A command that runs `az` (or `program`, for tests) with `args`.
pub fn az_command(program: &str, args: &[&str]) -> Command {
    let mut command = if cfg!(windows) && program == "az" {
        let mut c = Command::new("cmd");
        c.args(["/C", "az"]);
        c
    } else {
        Command::new(program)
    };
    command.args(args);
    command
}

/// A readable error for a CLI that could not be started.
pub fn start_error(e: &std::io::Error) -> String {
    if e.kind() == std::io::ErrorKind::NotFound {
        "the Azure CLI (az) was not found; install it and make sure `az` runs in this shell".into()
    } else {
        format!("could not run the Azure CLI: {e}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Runs the real `az --version`. On GitHub's Windows runners (which have
    /// the Azure CLI) this must work: it is the bug where az9s couldn't find
    /// `az.cmd`. Elsewhere it is skipped when az isn't installed.
    #[tokio::test]
    async fn the_real_azure_cli_can_be_started() {
        let result = az_command("az", &["--version"]).output().await;
        let required = cfg!(windows) && std::env::var_os("GITHUB_ACTIONS").is_some();
        match result {
            Ok(output) => assert!(
                output.status.success() || !required,
                "az --version failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ),
            Err(e) if !required => eprintln!("skipped: {}", start_error(&e)),
            Err(e) => panic!("{}", start_error(&e)),
        }
    }

    /// The bug this module fixes, shown on GitHub's Windows runners: starting
    /// `az` by bare name (what az9s did before) fails there, although the
    /// Azure CLI is installed and `az_command` starts it.
    #[cfg(windows)]
    #[tokio::test]
    async fn on_windows_bare_az_is_not_found() {
        if std::env::var_os("GITHUB_ACTIONS").is_none() {
            return;
        }
        let bare = Command::new("az").arg("--version").output().await;
        assert_eq!(
            bare.map(|_| ()).unwrap_err().kind(),
            std::io::ErrorKind::NotFound,
            "Command::new(\"az\") can't start az.cmd"
        );
    }

    #[test]
    fn not_found_is_explained() {
        let e = std::io::Error::from(std::io::ErrorKind::NotFound);
        assert!(start_error(&e).contains("was not found"));
    }
}
