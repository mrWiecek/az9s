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
        let mut command = if cfg!(target_os = "macos") {
            let mut c = Command::new("open");
            c.arg(url);
            c
        } else if cfg!(windows) {
            // `start` treats the first quoted argument as a window title.
            let mut c = Command::new("cmd");
            c.args(["/C", "start", "", url]);
            c
        } else {
            let mut c = Command::new("xdg-open");
            c.arg(url);
            c
        };
        command
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map(|_| ())
            .map_err(|e| e.to_string())
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
    fn osc52_wraps_the_encoded_text() {
        assert_eq!(osc52("hi"), "\x1b]52;c;aGk=\x07");
    }
}
