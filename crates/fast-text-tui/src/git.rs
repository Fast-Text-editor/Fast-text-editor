use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Gutter mark for a line relative to HEAD.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GitMark {
    Added,
    Modified,
    Deleted,
}

impl GitMark {
    pub fn as_char(self) -> char {
        match self {
            Self::Added => '+',
            Self::Modified => '~',
            Self::Deleted => '▾',
        }
    }
}

/// Cached git info for the current workspace/file.
#[derive(Debug, Clone, Default)]
pub struct GitInfo {
    pub branch: Option<String>,
    pub marks: HashMap<PathBuf, HashMap<usize, GitMark>>,
}

impl GitInfo {
    pub fn refresh_branch(&mut self, cwd: &Path) {
        self.branch = git_branch(cwd);
    }

    pub fn refresh_file(&mut self, path: &Path) {
        if let Some(marks) = git_diff_marks(path) {
            self.marks.insert(path.to_path_buf(), marks);
        } else {
            self.marks.remove(path);
        }
    }

    pub fn marks_for(&self, path: &Path) -> Option<&HashMap<usize, GitMark>> {
        self.marks.get(path)
    }
}

fn git_branch(cwd: &Path) -> Option<String> {
    let out = Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .current_dir(cwd)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if s.is_empty() {
        None
    } else {
        Some(s)
    }
}

/// Parse `git diff --unified=0 -- <file>` for line marks.
/// Uses the file's parent dir as cwd so git resolves correctly.
fn git_diff_marks(path: &Path) -> Option<HashMap<usize, GitMark>> {
    let cwd = path.parent()?;
    let name = path.file_name()?;
    // Ensure file is in a git repo.
    let inside = Command::new("git")
        .args(["rev-parse", "--is-inside-work-tree"])
        .current_dir(cwd)
        .output()
        .ok()?;
    if !inside.status.success() {
        return None;
    }

    let out = Command::new("git")
        .args([
            "diff",
            "--no-color",
            "--unified=0",
            "--",
            &name.to_string_lossy(),
        ])
        .current_dir(cwd)
        .output()
        .ok()?;
    if !out.status.success() {
        return Some(HashMap::new());
    }
    let text = String::from_utf8_lossy(&out.stdout);
    Some(parse_unified_diff(&text))
}

fn parse_unified_diff(diff: &str) -> HashMap<usize, GitMark> {
    let mut marks = HashMap::new();
    let mut new_line: usize = 0;
    let mut hunk_new_start: usize = 0;
    let mut hunk_new_count: usize = 0;
    let mut hunk_old_count: usize = 0;
    let mut seen_plus = false;
    let mut seen_minus = false;

    let flush_hunk = |marks: &mut HashMap<usize, GitMark>,
                      start: usize,
                      new_count: usize,
                      old_count: usize,
                      seen_plus: bool,
                      seen_minus: bool| {
        if start == 0 && new_count == 0 && !seen_plus && !seen_minus {
            return;
        }
        if old_count > 0 && new_count == 0 {
            // Pure deletion: mark the line before/at start.
            let line = start.saturating_sub(1);
            marks.insert(line, GitMark::Deleted);
        } else if old_count > 0 && new_count > 0 {
            for l in start.saturating_sub(1)..start.saturating_sub(1) + new_count {
                marks.insert(l, GitMark::Modified);
            }
        } else if old_count == 0 && new_count > 0 {
            for l in start.saturating_sub(1)..start.saturating_sub(1) + new_count {
                marks.insert(l, GitMark::Added);
            }
        } else if seen_plus || seen_minus {
            // Fallback
            if seen_minus && !seen_plus {
                marks.insert(start.saturating_sub(1), GitMark::Deleted);
            } else if seen_plus && !seen_minus {
                marks.insert(start.saturating_sub(1), GitMark::Added);
            } else {
                marks.insert(start.saturating_sub(1), GitMark::Modified);
            }
        }
    };

    for line in diff.lines() {
        if let Some((old_start, old_count, ns, nc)) = parse_hunk_header(line) {
            flush_hunk(
                &mut marks,
                hunk_new_start,
                hunk_new_count,
                hunk_old_count,
                seen_plus,
                seen_minus,
            );
            hunk_new_start = ns;
            hunk_new_count = nc;
            hunk_old_count = old_count;
            let _ = old_start;
            new_line = ns;
            seen_plus = false;
            seen_minus = false;
            continue;
        }
        if line.starts_with("+++") || line.starts_with("---") || line.starts_with("@@") {
            continue;
        }
        if line.starts_with('+') {
            seen_plus = true;
            let _ = new_line;
        } else if line.starts_with('-') {
            seen_minus = true;
        }
    }
    flush_hunk(
        &mut marks,
        hunk_new_start,
        hunk_new_count,
        hunk_old_count,
        seen_plus,
        seen_minus,
    );
    marks
}

fn parse_hunk_header(line: &str) -> Option<(usize, usize, usize, usize)> {
    // @@ -old,oc +new,nc @@
    if !line.starts_with("@@ ") {
        return None;
    }
    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.len() < 4 {
        return None;
    }
    let old = parts[1].strip_prefix('-')?;
    let new = parts[2].strip_prefix('+')?;
    let (os, oc) = parse_range(old);
    let (ns, nc) = parse_range(new);
    Some((os, oc, ns, nc))
}

fn parse_range(s: &str) -> (usize, usize) {
    if let Some((a, b)) = s.split_once(',') {
        (a.parse().unwrap_or(0), b.parse().unwrap_or(0))
    } else {
        (s.parse().unwrap_or(0), 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_added_hunk() {
        let diff = "@@ -0,0 +1,2 @@\n+hello\n+world\n";
        let marks = parse_unified_diff(diff);
        assert_eq!(marks.get(&0), Some(&GitMark::Added));
        assert_eq!(marks.get(&1), Some(&GitMark::Added));
    }

    #[test]
    fn parse_modified_hunk() {
        let diff = "@@ -1,1 +1,1 @@\n-old\n+new\n";
        let marks = parse_unified_diff(diff);
        assert_eq!(marks.get(&0), Some(&GitMark::Modified));
    }

    #[test]
    fn parse_deleted_hunk() {
        let diff = "@@ -2,2 +1,0 @@\n-a\n-b\n";
        let marks = parse_unified_diff(diff);
        assert_eq!(marks.get(&0), Some(&GitMark::Deleted));
    }
}
