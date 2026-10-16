use crate::Buffer;

/// Direction to search in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchDirection {
    Forward,
    Backward,
}

/// A match, as a `[start, end)` character range.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Match {
    pub start: usize,
    pub end: usize,
}

impl Match {
    pub fn len(&self) -> usize {
        self.end - self.start
    }

    pub fn is_empty(&self) -> bool {
        self.start == self.end
    }
}

/// Finds the next literal occurrence of `needle` in `buffer`.
///
/// `from` is the caret character index. Forward search finds the first match
/// starting at or after `from`; backward search finds the last match ending at
/// or before `from`. When `wrap` is true, the search continues from the other
/// end of the buffer if nothing is found.
pub fn find_literal(
    buffer: &Buffer,
    needle: &str,
    from: usize,
    direction: SearchDirection,
    case_sensitive: bool,
    wrap: bool,
) -> Option<Match> {
    let needle: Vec<char> = needle.chars().collect();
    if needle.is_empty() {
        return None;
    }
    let hay: Vec<char> = buffer.text().chars().collect();
    let n = hay.len();
    let m = needle.len();
    if m > n {
        return None;
    }

    let eq = |a: char, b: char| -> bool {
        if case_sensitive {
            a == b
        } else {
            a.eq_ignore_ascii_case(&b)
        }
    };

    let matches_at = |start: usize| -> bool { (0..m).all(|k| eq(hay[start + k], needle[k])) };

    match direction {
        SearchDirection::Forward => {
            let start = from.min(n);
            for i in start..=n.saturating_sub(m) {
                if matches_at(i) {
                    return Some(Match {
                        start: i,
                        end: i + m,
                    });
                }
            }
            if wrap && start > 0 {
                for i in 0..start.min(n.saturating_sub(m) + 1) {
                    if matches_at(i) {
                        return Some(Match {
                            start: i,
                            end: i + m,
                        });
                    }
                }
            }
            None
        }
        SearchDirection::Backward => {
            // Last match ending at or before `from`.
            let last_start = from.saturating_sub(m).min(n.saturating_sub(m));
            for i in (0..=last_start).rev() {
                if matches_at(i) {
                    return Some(Match {
                        start: i,
                        end: i + m,
                    });
                }
            }
            if wrap {
                let hi = n.saturating_sub(m);
                for i in (last_start + 1..=hi).rev() {
                    if matches_at(i) {
                        return Some(Match {
                            start: i,
                            end: i + m,
                        });
                    }
                }
            }
            None
        }
    }
}

/// Finds every non-overlapping occurrence of `needle`.
pub fn find_all(buffer: &Buffer, needle: &str, case_sensitive: bool) -> Vec<Match> {
    let mut out = Vec::new();
    if needle.is_empty() {
        return out;
    }
    let mut cursor = 0;
    while let Some(m) = find_literal(
        buffer,
        needle,
        cursor,
        SearchDirection::Forward,
        case_sensitive,
        false,
    ) {
        out.push(m);
        cursor = m.end.max(m.start + 1);
        if cursor > buffer.len_chars() {
            break;
        }
    }
    out
}
