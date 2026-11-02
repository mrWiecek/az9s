//! Vim-style text search in the detail view: `/pattern`, `n`, `N`.
//!
//! Matching is "smartcase", like Vim with `ignorecase smartcase`:
//! case-insensitive unless the pattern has an uppercase letter. Only ASCII
//! letters are case-folded, so byte offsets stay valid for highlighting.

use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};

/// A match: line index and byte range within that line's text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Match {
    pub line: usize,
    pub start: usize,
    pub end: usize,
}

/// Every non-overlapping match of `pattern` in `lines`, in reading order.
pub fn find_matches(lines: &[String], pattern: &str) -> Vec<Match> {
    if pattern.is_empty() {
        return vec![];
    }
    let fold = !pattern.chars().any(|c| c.is_ascii_uppercase());
    let needle = if fold {
        pattern.to_ascii_lowercase()
    } else {
        pattern.to_string()
    };
    let mut matches = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        let hay = if fold {
            line.to_ascii_lowercase()
        } else {
            line.clone()
        };
        let mut from = 0;
        while let Some(pos) = hay[from..].find(&needle) {
            let start = from + pos;
            let end = start + needle.len();
            matches.push(Match {
                line: i,
                start,
                end,
            });
            from = end;
        }
    }
    matches
}

/// The match to go to from position (`line`, `col`): the first one after it
/// (`forward`) or before it, wrapping around. Returns its index and whether
/// the search wrapped. `inclusive` also accepts a match exactly at the
/// position (used while typing, so the first match can be where you are).
pub fn next_match(
    matches: &[Match],
    (line, col): (usize, usize),
    forward: bool,
    inclusive: bool,
) -> Option<(usize, bool)> {
    if matches.is_empty() {
        return None;
    }
    let at = |m: &Match| (m.line, m.start);
    let found = if forward {
        matches
            .iter()
            .position(|m| at(m) > (line, col) || (inclusive && at(m) == (line, col)))
    } else {
        matches
            .iter()
            .rposition(|m| at(m) < (line, col) || (inclusive && at(m) == (line, col)))
    };
    match found {
        Some(i) => Some((i, false)),
        None if forward => Some((0, true)),
        None => Some((matches.len() - 1, true)),
    }
}

/// `line` with `ranges` (byte ranges of its text) highlighted; the range
/// starting at `current` gets the stronger highlight.
pub fn highlight(
    line: Line<'static>,
    ranges: &[(usize, usize)],
    current: Option<usize>,
) -> Line<'static> {
    if ranges.is_empty() {
        return line;
    }
    let other = Style::default().fg(Color::Black).bg(Color::Yellow);
    let strong = Style::default()
        .fg(Color::Black)
        .bg(Color::LightRed)
        .add_modifier(Modifier::BOLD);

    let mut spans = Vec::new();
    let mut offset = 0; // byte offset of the current span in the line
    for span in line.spans {
        let text = span.content.into_owned();
        let len = text.len();
        let mut cut = 0; // how much of this span is emitted
        for &(start, end) in ranges {
            // The part of the match inside this span.
            let s = start.max(offset).max(offset + cut);
            let e = end.min(offset + len);
            if s >= e {
                continue;
            }
            let (s, e) = (s - offset, e - offset);
            if s > cut {
                spans.push(Span::styled(text[cut..s].to_string(), span.style));
            }
            let style = if Some(start) == current {
                strong
            } else {
                other
            };
            spans.push(Span::styled(
                text[s..e].to_string(),
                span.style.patch(style),
            ));
            cut = e;
        }
        if cut < len {
            spans.push(Span::styled(text[cut..].to_string(), span.style));
        }
        offset += len;
    }
    Line::from(spans)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn smartcase_matching() {
        let text = lines(&["\"vmSize\": \"Standard_D4s\"", "no", "vmsize VMSIZE"]);
        let m = find_matches(&text, "vmsize");
        assert_eq!(m.len(), 3, "lowercase pattern ignores case: {m:?}");
        assert_eq!(
            m[0],
            Match {
                line: 0,
                start: 1,
                end: 7
            }
        );
        let m = find_matches(&text, "VMSIZE");
        assert_eq!(
            m,
            [Match {
                line: 2,
                start: 7,
                end: 13
            }],
            "uppercase: exact case"
        );
        assert!(find_matches(&text, "").is_empty());
    }

    #[test]
    fn next_and_previous_wrap_around() {
        let text = lines(&["a x", "b", "x x"]);
        let m = find_matches(&text, "x"); // (0,2) (2,0) (2,2)
        assert_eq!(next_match(&m, (0, 2), true, false), Some((1, false)));
        assert_eq!(next_match(&m, (0, 2), true, true), Some((0, false)));
        assert_eq!(
            next_match(&m, (2, 2), true, false),
            Some((0, true)),
            "wraps to top"
        );
        assert_eq!(
            next_match(&m, (0, 2), false, false),
            Some((2, true)),
            "wraps to bottom"
        );
        assert_eq!(next_match(&m, (2, 2), false, false), Some((1, false)));
        assert_eq!(next_match(&[], (0, 0), true, false), None);
    }

    #[test]
    fn highlighting_splits_spans_and_keeps_text() {
        let line = Line::from(vec![
            Span::styled("  \"name\"", Style::default().fg(Color::Cyan)),
            Span::raw(": "),
            Span::styled("\"name-1\"", Style::default().fg(Color::Green)),
        ]);
        let text = line.to_string();
        let ranges: Vec<(usize, usize)> = find_matches(std::slice::from_ref(&text), "name")
            .iter()
            .map(|m| (m.start, m.end))
            .collect();
        assert_eq!(ranges, [(3, 7), (11, 15)]);
        let out = highlight(line, &ranges, Some(11));
        assert_eq!(out.to_string(), text, "same text");
        let marked: Vec<(&str, Option<Color>)> = out
            .spans
            .iter()
            .filter(|s| s.style.bg.is_some())
            .map(|s| (s.content.as_ref(), s.style.bg))
            .collect();
        assert_eq!(
            marked,
            [
                ("name", Some(Color::Yellow)),
                ("name", Some(Color::LightRed))
            ]
        );
    }

    #[test]
    fn a_match_across_two_spans_is_highlighted_in_both() {
        let line = Line::from(vec![Span::raw("ab"), Span::raw("cd")]);
        let out = highlight(line, &[(1, 3)], None);
        let marked: Vec<&str> = out
            .spans
            .iter()
            .filter(|s| s.style.bg.is_some())
            .map(|s| s.content.as_ref())
            .collect();
        assert_eq!(marked, ["b", "c"]);
        assert_eq!(out.to_string(), "abcd");
    }
}
