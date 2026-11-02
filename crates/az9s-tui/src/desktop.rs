//! Copy to the clipboard and open URLs: the only places az9s reaches outside
//! the terminal. Behind a trait so tests can record instead.

use std::io::Write;
use std::process::{Command, Stdio};

pub trait Desktop: Send {
    /// Put `text` on the clipboard.
    fn copy(&mut self, text: &str) -> Result<(), String>;
    /// Open `url` in the default browser.
    fn open(&mut self, url: &str) -> Result<(), String>;
}

/// The real desktop.
///
/// Copying uses the OSC 52 terminal escape sequence: the terminal itself puts
/// the text on the clipboard, which also works over SSH and needs no
/// clipboard tools. Most terminals support it (in tmux, enable
/// `set-clipboard`).
pub struct SystemDesktop;

impl Desktop for SystemDesktop {
    fn copy(&mut self, text: &str) -> Result<(), String> {
        let mut out = std::io::stdout();
        write!(out, "{}", osc52(text))
            .and_then(|_| out.flush())
            .map_err(|e| e.to_string())
    }

    fn open(&mut self, url: &str) -> Result<(), String> {
        let platform = if cfg!(target_os = "macos") {
            Platform::MacOs
        } else if cfg!(windows) {
            Platform::Windows
        } else if is_wsl() {
            Platform::Wsl
        } else {
            Platform::Linux
        };
        let mut errors = Vec::new();
        for (program, args) in open_commands(platform, url) {
            let mut command = Command::new(&program);
            command
                .args(&args)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null());
            if platform == Platform::Wsl && program.ends_with("cmd.exe") {
                // cmd.exe warns about (and ignores) a Linux working directory.
                command.current_dir("/mnt/c");
            }
            match command.spawn() {
                Ok(_) => return Ok(()),
                Err(e) => errors.push(format!("{program}: {}", short_io_error(&e))),
            }
        }
        Err(errors.join("; "))
    }
}

/// Where az9s runs, for choosing how to open a browser.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    MacOs,
    Windows,
    /// Linux under Windows Subsystem for Linux: the browser is on Windows.
    Wsl,
    Linux,
}

/// Commands that open `url`, in the order to try them (the first one that
/// starts wins).
pub fn open_commands(platform: Platform, url: &str) -> Vec<(String, Vec<String>)> {
    let cmd = |program: &str, args: &[&str]| {
        (
            program.to_string(),
            args.iter().map(|a| a.to_string()).collect::<Vec<_>>(),
        )
    };
    // `start` treats the first quoted argument as the window title, and cmd
    // would split the URL at `&`, `|`, `<`, `>` or `^` unless escaped.
    let escaped = escape_for_cmd(url);
    let start = ["/C", "start", "", escaped.as_str()];
    match platform {
        Platform::MacOs => vec![cmd("open", &[url])],
        Platform::Windows => vec![cmd("cmd", &start)],
        Platform::Wsl => vec![
            // wslview (from wslu) is the WSL way to open URLs on Windows.
            cmd("wslview", &[url]),
            // Windows interop: cmd.exe is normally on PATH; if PATH doesn't
            // include Windows, use its usual location.
            cmd("cmd.exe", &start),
            cmd("/mnt/c/Windows/System32/cmd.exe", &start),
            cmd("xdg-open", &[url]),
        ],
        Platform::Linux => vec![cmd("xdg-open", &[url])],
    }
}

/// Whether this Linux is running under WSL.
fn is_wsl() -> bool {
    std::env::var_os("WSL_DISTRO_NAME").is_some()
        || std::env::var_os("WSL_INTEROP").is_some()
        || std::fs::read_to_string("/proc/sys/kernel/osrelease")
            .map(|r| r.to_ascii_lowercase().contains("microsoft"))
            .unwrap_or(false)
}

/// `^`-escape characters that cmd.exe treats specially.
fn escape_for_cmd(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        if matches!(c, '^' | '&' | '|' | '<' | '>') {
            out.push('^');
        }
        out.push(c);
    }
    out
}

/// "not installed" instead of "No such file or directory (os error 2)".
fn short_io_error(e: &std::io::Error) -> String {
    match e.kind() {
        std::io::ErrorKind::NotFound => "not installed".into(),
        std::io::ErrorKind::PermissionDenied => "not allowed".into(),
        _ => e.to_string(),
    }
}

/// The OSC 52 sequence that sets the clipboard to `text`.
pub fn osc52(text: &str) -> String {
    format!("\x1b]52;c;{}\x07", base64(text.as_bytes()))
}

/// Standard base64 with padding (small enough not to need a dependency).
fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(ALPHABET[(n >> (18 - 6 * i)) as usize & 63] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_matches_rfc_4648_vectors() {
        for (input, expected) in [
            ("", ""),
            ("f", "Zg=="),
            ("fo", "Zm8="),
            ("foo", "Zm9v"),
            ("foob", "Zm9vYg=="),
            ("fooba", "Zm9vYmE="),
            ("foobar", "Zm9vYmFy"),
        ] {
            assert_eq!(base64(input.as_bytes()), expected, "{input}");
        }
    }

    #[test]
    fn wsl_opens_the_windows_browser_before_trying_xdg_open() {
        let url = "https://portal.azure.com/#@t/resource/subscriptions/s";
        let programs: Vec<String> = open_commands(Platform::Wsl, url)
            .into_iter()
            .map(|(p, _)| p)
            .collect();
        assert_eq!(
            programs,
            [
                "wslview",
                "cmd.exe",
                "/mnt/c/Windows/System32/cmd.exe",
                "xdg-open"
            ]
        );
        let (_, args) = &open_commands(Platform::Wsl, url)[1];
        assert_eq!(args, &["/C", "start", "", url]);
        assert_eq!(open_commands(Platform::Linux, url)[0].0, "xdg-open");
    }

    #[test]
    fn cmd_special_characters_are_escaped() {
        let (_, args) = &open_commands(Platform::Windows, "https://x/?a=1&b=2|c")[0];
        assert_eq!(args[3], "https://x/?a=1^&b=2^|c");
    }

    #[test]
    fn osc52_wraps_the_encoded_text() {
        assert_eq!(osc52("hi"), "\x1b]52;c;aGk=\x07");
    }
}
