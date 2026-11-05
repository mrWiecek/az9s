//! Running the Azure CLI.
//!
//! On Windows `az` is a batch file (`az.cmd`); [`c9s_exec`] finds it on the
//! PATH (never in the current directory) and starts it by full path, so
//! its arguments are quoted safely for `cmd.exe`.

use tokio::process::Command;

/// A command that runs `az` (or `program`, for tests) with `args`. An
/// argument that could change the command line (a tenant from the config
/// file with `&` in it, say) is refused; so is an `az` that isn't on the
/// PATH (reported as "not found").
pub fn az_command(program: &str, args: &[&str]) -> std::io::Result<Command> {
    c9s_exec::command(program, args).map_err(|e| match e {
        c9s_exec::Error::NotFound(_) => std::io::Error::from(std::io::ErrorKind::NotFound),
        c9s_exec::Error::UnsafeArgument(_) => {
            std::io::Error::new(std::io::ErrorKind::InvalidInput, e.to_string())
        }
    })
}

/// A readable error for a CLI that could not be started.
pub fn start_error(e: &std::io::Error) -> String {
    if e.kind() == std::io::ErrorKind::InvalidInput {
        format!("did not run the Azure CLI: {e}")
    } else if e.kind() == std::io::ErrorKind::NotFound {
        "the Azure CLI (az) was not found; install it and make sure `az` runs in this shell".into()
    } else {
        format!("could not run the Azure CLI: {e}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Runs the real `az --version`. On GitHub's Windows runners (which have
    /// the Azure CLI) this must work: it is the bug where c9s couldn't find
    /// `az.cmd`. Elsewhere it is skipped when az isn't installed.
    #[tokio::test]
    async fn the_real_azure_cli_can_be_started() {
        let result = match az_command("az", &["--version"]) {
            Ok(mut command) => command.output().await,
            Err(e) => Err(e),
        };
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

    #[test]
    fn not_found_is_explained() {
        let e = std::io::Error::from(std::io::ErrorKind::NotFound);
        assert!(start_error(&e).contains("was not found"));
    }
}
