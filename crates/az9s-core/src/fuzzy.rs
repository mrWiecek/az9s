//! Fuzzy matching for the command palette, in the style of fzf: the query's
//! characters must appear in order, and matches score higher when they are
//! contiguous, start words, or sit early in a short text.
//!
//! Spaces separate words that must all match: `vm prod` matches
//! `vm-web-prod`. Matching ignores ASCII case.

/// A match: its score (higher is better) and the matched character indices
/// in the text, for highlighting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FuzzyMatch {
    pub score: i64,
    pub positions: Vec<usize>,
}

/// Match `query` against `text`. `None` if some query character is missing.
pub fn fuzzy_match(text: &str, query: &str) -> Option<FuzzyMatch> {
    let chars: Vec<char> = text.chars().collect();
    let lower: Vec<char> = chars.iter().map(|c| c.to_ascii_lowercase()).collect();
    let mut total = 0;
    let mut positions = Vec::new();
    for word in query.split_whitespace() {
        let word: Vec<char> = word.chars().map(|c| c.to_ascii_lowercase()).collect();
        let (score, pos) = match_word(&chars, &lower, &word)?;
        total += score;
        positions.extend(pos);
    }
    positions.sort_unstable();
    positions.dedup();
    // Among equal matches, prefer shorter texts.
    total -= chars.len() as i64 / 8;
    Some(FuzzyMatch {
        score: total,
        positions,
    })
}

/// Best placement of one query word. Tries every start where the first
/// character occurs and matches the rest greedily from there; takes the
/// best score. Texts here are short (names, IDs), so this is cheap.
fn match_word(chars: &[char], lower: &[char], word: &[char]) -> Option<(i64, Vec<usize>)> {
    let first = *word.first()?;
    let mut best: Option<(i64, Vec<usize>)> = None;
    for start in (0..lower.len()).filter(|&i| lower[i] == first) {
        let mut positions = vec![start];
        let mut at = start + 1;
        for &c in &word[1..] {
            match lower[at..].iter().position(|&l| l == c) {
                Some(offset) => {
                    positions.push(at + offset);
                    at += offset + 1;
                }
                None => break,
            }
        }
        if positions.len() < word.len() {
            // Later starts only have less text left: no match at all.
            break;
        }
        let score = score(chars, &positions);
        if best.as_ref().is_none_or(|(b, _)| score > *b) {
            best = Some((score, positions));
        }
    }
    best
}

fn score(chars: &[char], positions: &[usize]) -> i64 {
    let mut score = 0;
    for (n, &p) in positions.iter().enumerate() {
        score += 1;
        if is_word_start(chars, p) {
            // The first character starting a word matters most; later
            // ones (v…M in virtualMachines) still count.
            score += if n == 0 { 8 } else { 6 };
        }
        if n > 0 {
            let gap = p - positions[n - 1] - 1;
            if gap == 0 {
                score += 8; // contiguous
            } else {
                score -= (gap as i64).min(3);
            }
        }
    }
    // Earlier matches rank higher.
    score - (positions[0] as i64).min(10)
}

/// Start of the text, after a separator, or a lower→upper case change
/// (`virtualMachines`).
fn is_word_start(chars: &[char], i: usize) -> bool {
    if i == 0 {
        return true;
    }
    let prev = chars[i - 1];
    matches!(prev, '-' | '_' | '.' | '/' | ' ' | ':' | '=')
        || (prev.is_ascii_lowercase() && chars[i].is_ascii_uppercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn score(text: &str, query: &str) -> i64 {
        fuzzy_match(text, query)
            .map(|m| m.score)
            .unwrap_or(i64::MIN)
    }

    #[test]
    fn characters_must_appear_in_order() {
        assert!(fuzzy_match("kv-prod-weu", "kvprod").is_some());
        assert!(fuzzy_match("kv-prod-weu", "kvweu").is_some());
        assert!(fuzzy_match("kv-prod-weu", "weukv").is_none());
        assert!(fuzzy_match("kv-prod-weu", "x").is_none());
        assert!(
            fuzzy_match("anything", "").is_some(),
            "empty query matches all"
        );
    }

    #[test]
    fn positions_point_at_the_matched_characters() {
        let m = fuzzy_match("kv-prod-weu", "kvp").unwrap();
        assert_eq!(m.positions, [0, 1, 3]);
    }

    #[test]
    fn contiguous_and_word_start_matches_rank_higher() {
        assert!(score("prod-app", "prod") > score("pxrxoxd", "prod"));
        assert!(score("app-prod", "prod") > score("approved", "prod"));
        assert!(score("virtualMachines", "vm") > score("evmx", "vm"));
        assert!(score("prod", "prod") > score("a-very-long-name-with-prod-in-it", "prod"));
    }

    #[test]
    fn the_best_placement_wins_not_the_first() {
        // Greedy from the first 'p' would match p..r..o..d across words;
        // starting at "prod" is contiguous.
        let m = fuzzy_match("p-r-o-d-prod", "prod").unwrap();
        assert_eq!(m.positions, [8, 9, 10, 11]);
    }

    #[test]
    fn several_words_must_all_match() {
        assert!(fuzzy_match("vm-web-prod", "vm prod").is_some());
        assert!(fuzzy_match("vm-web-prod", "vm dev").is_none());
        assert!(
            fuzzy_match("VM-Web-Prod", "web PROD").is_some(),
            "ignores case"
        );
    }
}
