//! Copy to the clipboard and open URLs: the only places c9s reaches outside
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
        check_url(url)?;
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
            if platform == Platform::Wsl && program.ends_with("rundll32.exe") {
                // Windows programs warn about a Linux working directory.
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

/// Where c9s runs, for choosing how to open a browser.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    MacOs,
    Windows,
    /// Linux under Windows Subsystem for Linux: the browser is on Windows.
    Wsl,
    Linux,
}

/// The consoles c9s links to. Nothing else is opened.
const CONSOLE_HOSTS: &[&str] = &[
    "portal.azure.com",
    "console.cloud.google.com",
    "console.aws.amazon.com",
];

/// Refuse anything but an `https://` link to a known console whose
/// characters need no quoting anywhere: links are built from cloud data
/// (resource names), and the program that opens them must never see a
/// space, quote, or shell character.
pub fn check_url(url: &str) -> Result<(), String> {
    let rest = url
        .strip_prefix("https://")
        .ok_or_else(|| "refused to open a link that isn't https".to_string())?;
    let host = rest.split(['/', '?', '#']).next().unwrap_or_default();
    let known = CONSOLE_HOSTS
        .iter()
        .any(|h| host == *h || host.ends_with(&format!(".{h}")));
    if !known {
        return Err(format!("refused to open a link to {host:?}"));
    }
    // RFC 3986 characters only; no quotes, spaces, `<>\\^{|}` or backticks.
    let safe = url.chars().all(|c| {
        c.is_ascii_alphanumeric()
            || matches!(
                c,
                '-' | '.'
                    | '_'
                    | '~'
                    | ':'
                    | '/'
                    | '?'
                    | '#'
                    | '['
                    | ']'
                    | '@'
                    | '!'
                    | '$'
                    | '&'
                    | '\''
                    | '('
                    | ')'
                    | '*'
                    | '+'
                    | ','
                    | ';'
                    | '='
                    | '%'
            )
    });
    if !safe {
        return Err("refused to open a link with unexpected characters".into());
    }
    Ok(())
}

/// Commands that open `url`, in the order to try them (the first one that
/// starts wins). None goes through a shell: on Windows the URL protocol
/// handler opens it (as `start` would, without `cmd.exe` parsing it).
pub fn open_commands(platform: Platform, url: &str) -> Vec<(String, Vec<String>)> {
    let cmd = |program: &str, args: &[&str]| {
        (
            program.to_string(),
            args.iter().map(|a| a.to_string()).collect::<Vec<_>>(),
        )
    };
    let handler = ["url.dll,FileProtocolHandler", url];
    match platform {
        Platform::MacOs => vec![cmd("open", &[url])],
        Platform::Windows => vec![cmd("rundll32", &handler)],
        Platform::Wsl => vec![
            // wslview (from wslu) is the WSL way to open URLs on Windows.
            cmd("wslview", &[url]),
            // Windows interop: rundll32.exe is normally on PATH; if PATH
            // doesn't include Windows, use its usual location.
            cmd("rundll32.exe", &handler),
            cmd("/mnt/c/Windows/System32/rundll32.exe", &handler),
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
                "rundll32.exe",
                "/mnt/c/Windows/System32/rundll32.exe",
                "xdg-open"
            ]
        );
        let (_, args) = &open_commands(Platform::Wsl, url)[1];
        assert_eq!(args, &["url.dll,FileProtocolHandler", url]);
        assert_eq!(open_commands(Platform::Linux, url)[0].0, "xdg-open");
    }

    #[test]
    fn windows_opens_links_without_a_shell() {
        let url = "https://console.aws.amazon.com/go/view?arn=a&b=c";
        let commands = open_commands(Platform::Windows, url);
        assert_eq!(commands[0].0, "rundll32");
        assert_eq!(
            commands[0].1[1], url,
            "passed as is, no shell to escape for"
        );
        assert!(commands.iter().all(|(p, _)| !p.contains("cmd")));
    }

    #[test]
    fn only_https_links_to_the_consoles_open() {
        for ok in [
            "https://portal.azure.com/#@t/resource/subscriptions/s",
            "https://console.cloud.google.com/home/dashboard?project=p",
            "https://console.aws.amazon.com/go/view?arn=arn%3Aaws",
            "https://eu-west-1.console.aws.amazon.com/ec2/home",
        ] {
            assert!(check_url(ok).is_ok(), "{ok}");
        }
        for bad in [
            "http://portal.azure.com/",
            "file:///etc/passwd",
            "https://evil.example/",
            "https://portal.azure.com.evil.example/",
            "https://portal.azure.com/#x\" & calc",
            "https://portal.azure.com/a b",
            "https://portal.azure.com/%PATH%^",
            "https://portal.azure.com/\x1b]0;x",
            "javascript:alert(1)",
        ] {
            assert!(check_url(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn osc52_wraps_the_encoded_text() {
        assert_eq!(osc52("hi"), "\x1b]52;c;aGk=\x07");
    }
}
