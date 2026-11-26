use std::collections::HashSet;
use std::sync::OnceLock;

static WORDS: OnceLock<HashSet<String>> = OnceLock::new();

fn words() -> &'static HashSet<String> {
    WORDS.get_or_init(|| {
        include_str!("common_words.txt")
            .lines()
            .filter(|l| !l.is_empty())
            .map(|l| l.to_ascii_lowercase())
            .collect()
    })
}

/// Returns true if the word looks misspelled.
/// Only checks alphabetic words with length >= 4 to reduce noise.
pub fn is_misspelled(word: &str) -> bool {
    if word.len() < 4 {
        return false;
    }
    if !word.chars().all(|c| c.is_ascii_alphabetic() || c == '\'') {
        return false;
    }
    let lower = word.to_ascii_lowercase();
    let trimmed = lower.trim_matches('\'');
    if trimmed.len() < 4 {
        return false;
    }
    // All-caps acronyms are usually fine.
    if word.chars().all(|c| c.is_ascii_uppercase()) {
        return false;
    }
    !words().contains(trimmed)
}

/// Find misspelled word ranges in a line as (start_col_bytes, end_col_bytes).
pub fn misspellings_in_line(line: &str) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i] as char;
        if c.is_ascii_alphabetic() || c == '\'' {
            let start = i;
            i += 1;
            while i < bytes.len() {
                let c = bytes[i] as char;
                if c.is_ascii_alphabetic() || c == '\'' {
                    i += 1;
                } else {
                    break;
                }
            }
            let word = &line[start..i];
            if is_misspelled(word) {
                out.push((start, i));
            }
        } else {
            i += c.len_utf8().max(1);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn common_words_are_ok() {
        assert!(!is_misspelled("hello"));
        assert!(!is_misspelled("world"));
        assert!(!is_misspelled("focus"));
    }

    #[test]
    fn obvious_misspelling_flagged() {
        assert!(is_misspelled("speling"));
        assert!(is_misspelled("mispelled"));
    }

    #[test]
    fn short_words_ignored() {
        assert!(!is_misspelled("xyz"));
    }
}
