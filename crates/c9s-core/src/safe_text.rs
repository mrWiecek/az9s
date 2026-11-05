//! Text from the clouds is untrusted: any member of a tenant, project or
//! account can name a resource, tag it or describe a group. These helpers
//! keep such text from acting on the user's terminal, clipboard or
//! spreadsheet.

/// Whether `c` must not reach a terminal or a pasted command line: C0 and
/// C1 control characters (escape sequences, BEL, CR, newlines), DEL, and
/// bidirectional overrides and isolates (text that displays differently
/// from what it is, e.g. `exe.txt` shown as `txt.exe`).
pub fn is_unsafe(c: char) -> bool {
    c.is_control()
        || matches!(
            c,
            '\u{200e}' | '\u{200f}' | '\u{061c}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}'
        )
}

/// `text` with every unsafe character replaced by U+FFFD, so it is visibly
/// altered rather than silently changed.
pub fn clean(text: &str) -> std::borrow::Cow<'_, str> {
    if text.chars().any(is_unsafe) {
        text.chars()
            .map(|c| if is_unsafe(c) { '\u{fffd}' } else { c })
            .collect::<String>()
            .into()
    } else {
        text.into()
    }
}

/// Text of several lines (tab-separated rows, JSON) for the clipboard:
/// newlines and tabs are kept, every other unsafe character replaced.
pub fn clean_lines(text: &str) -> String {
    text.split('\n')
        .map(|line| {
            line.split('\t')
                .map(|cell| clean(cell.trim_end_matches('\r')).into_owned())
                .collect::<Vec<_>>()
                .join("\t")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// One cell of tab-separated text meant for a spreadsheet: cleaned, and
/// with a leading `'` when a spreadsheet would read it as a formula
/// (`=`, `+`, `-`, `@`), so pasting a hostile name runs nothing.
pub fn spreadsheet_cell(text: &str) -> String {
    let text = clean(text);
    let formula = match text.chars().next() {
        Some('=' | '+' | '@') => true,
        // A negative number, a lone dash (an empty cell) and "- text" stay.
        Some('-') => text[1..]
            .chars()
            .next()
            .is_some_and(|c| !(c.is_ascii_digit() || c == '.' || c == ' ')),
        _ => false,
    };
    if formula {
        format!("'{text}")
    } else {
        text.into_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escape_sequences_and_bidi_overrides_are_replaced() {
        assert_eq!(clean("vm-1"), "vm-1");
        assert_eq!(clean("a\x1b]0;x\x07b"), "a\u{fffd}]0;x\u{fffd}b");
        assert_eq!(clean("line\nnext\r"), "line\u{fffd}next\u{fffd}");
        assert_eq!(clean("\u{9b}31m"), "\u{fffd}31m", "C1 CSI");
        assert_eq!(clean("doc\u{202e}fdp.exe"), "doc\u{fffd}fdp.exe");
        assert_eq!(clean("tab\there"), "tab\u{fffd}here");
        assert_eq!(clean("zoë 東京"), "zoë 東京", "ordinary text is kept");
    }

    #[test]
    fn copied_lines_keep_rows_and_columns_only() {
        assert_eq!(
            clean_lines("a\tb\r\nc\x1b[2Jd\te"),
            "a\tb\nc\u{fffd}[2Jd\te"
        );
    }

    #[test]
    fn formulas_are_defused_for_spreadsheets() {
        assert_eq!(spreadsheet_cell("=HYPERLINK(\"x\")"), "'=HYPERLINK(\"x\")");
        assert_eq!(
            spreadsheet_cell("+cmd|' /C calc'!A0"),
            "'+cmd|' /C calc'!A0"
        );
        assert_eq!(spreadsheet_cell("@SUM(A1)"), "'@SUM(A1)");
        assert_eq!(spreadsheet_cell("-x"), "'-x");
        assert_eq!(spreadsheet_cell("-12.5"), "-12.5");
        assert_eq!(spreadsheet_cell("-"), "-");
        assert_eq!(spreadsheet_cell("prod"), "prod");
        assert_eq!(spreadsheet_cell("a\tb"), "a\u{fffd}b", "no stray columns");
    }
}
