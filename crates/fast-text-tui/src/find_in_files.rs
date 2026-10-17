//! Project-wide literal search ("find in files").
//!
//! A dependency-free recursive scan rooted at the explorer's folder. It skips
//! dot-directories, a small set of well-known heavy directories (`target`,
//! `node_modules`, …), binary-looking files, and anything above a size cap, so
//! it stays responsive on real projects without pulling in a walker crate.

use std::path::{Path, PathBuf};

/// Files larger than this are skipped (they are almost never source text).
pub const MAX_FILE_BYTES: u64 = 2 * 1024 * 1024;
/// Total matches collected before the scan stops.
pub const MAX_RESULTS: usize = 2000;
/// Matches recorded per file before moving on.
pub const MAX_PER_FILE: usize = 200;
/// How deep the walk descends.
const MAX_DEPTH: usize = 32;

/// Directories never descended into.
const SKIP_DIRS: &[&str] = &[
    ".git",
    ".hg",
    ".svn",
    "target",
    "node_modules",
    "dist",
    "build",
    "vendor",
    "__pycache__",
    ".venv",
    "venv",
    ".idea",
    ".vscode",
];

/// One matching line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileMatch {
    pub path: PathBuf,
    /// 0-based line number.
    pub line: usize,
    /// 0-based character column of the match within the line.
    pub col: usize,
    /// The full text of the matching line, trimmed of trailing whitespace.
    pub text: String,
}

/// Whether a directory should be skipped.
fn skip_dir(name: &str) -> bool {
    name.starts_with('.') || SKIP_DIRS.contains(&name)
}

/// Whether a file looks binary (a NUL byte in its first few kilobytes).
fn looks_binary(bytes: &[u8]) -> bool {
    bytes.iter().take(8192).any(|&b| b == 0)
}

/// Searches every file under `root` for `query`, returning matching lines.
///
/// Matching is literal; `case_sensitive` chooses whether case matters. Results
/// are ordered by path and then by line.
pub fn find(root: &Path, query: &str, case_sensitive: bool) -> Vec<FileMatch> {
    if query.is_empty() {
        return Vec::new();
    }
    let mut files = Vec::new();
    walk(root, 0, &mut files);
    files.sort();
    matches_in_files(&files, query, case_sensitive)
}

/// Collects candidate files under `dir`, honouring the skip rules.
fn walk(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    if depth > MAX_DEPTH {
        return;
    }
    let Ok(reader) = std::fs::read_dir(dir) else {
        return;
    };
    let mut dirs: Vec<PathBuf> = Vec::new();
    for entry in reader.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_dir() {
            if !skip_dir(&name) {
                dirs.push(entry.path());
            }
        } else if file_type.is_file() {
            if let Ok(meta) = entry.metadata() {
                if meta.is_file() && meta.len() <= MAX_FILE_BYTES {
                    out.push(entry.path());
                }
            }
        }
    }
    dirs.sort();
    for dir in dirs {
        walk(&dir, depth + 1, out);
    }
}

/// Scans `files` for `query`, stopping at the result caps.
fn matches_in_files(files: &[PathBuf], query: &str, case_sensitive: bool) -> Vec<FileMatch> {
    let needle = if case_sensitive {
        query.to_string()
    } else {
        query.to_lowercase()
    };
    let mut out = Vec::new();
    'files: for path in files {
        let Ok(bytes) = std::fs::read(path) else {
            continue;
        };
        if looks_binary(&bytes) {
            continue;
        }
        let Ok(text) = String::from_utf8(bytes) else {
            continue;
        };
        let mut per_file = 0;
        for (line_no, line) in text.lines().enumerate() {
            let hay = if case_sensitive {
                line.to_string()
            } else {
                line.to_lowercase()
            };
            let Some(byte_col) = hay.find(&needle) else {
                continue;
            };
            let col = line[..byte_col].chars().count();
            out.push(FileMatch {
                path: path.clone(),
                line: line_no,
                col,
                text: line.trim_end().to_string(),
            });
            per_file += 1;
            if out.len() >= MAX_RESULTS {
                break 'files;
            }
            if per_file >= MAX_PER_FILE {
                continue 'files;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static COUNTER: AtomicUsize = AtomicUsize::new(0);

    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            let n = COUNTER.fetch_add(1, Ordering::Relaxed);
            let dir = std::env::temp_dir().join(format!("ft-fif-{}-{n}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }

        fn path(&self) -> &Path {
            &self.0
        }

        fn write(&self, rel: &str, contents: &str) {
            let path = self.0.join(rel);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, contents).unwrap();
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn finds_matching_lines_with_line_and_column() {
        let dir = TempDir::new();
        dir.write("src/a.rs", "one\nlet needle = 1;\n");
        let hits = find(dir.path(), "needle", true);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].line, 1);
        assert_eq!(hits[0].col, 4);
        assert_eq!(hits[0].text, "let needle = 1;");
    }

    #[test]
    fn case_insensitive_by_default() {
        let dir = TempDir::new();
        dir.write("a.txt", "Needle\nneedle\nNEEDLE\n");
        assert_eq!(find(dir.path(), "needle", false).len(), 3);
        assert_eq!(find(dir.path(), "needle", true).len(), 1);
    }

    #[test]
    fn skips_heavy_and_dot_directories() {
        let dir = TempDir::new();
        dir.write("src/a.rs", "needle\n");
        dir.write("target/b.rs", "needle\n");
        dir.write("node_modules/c.js", "needle\n");
        dir.write(".git/d", "needle\n");
        let hits = find(dir.path(), "needle", true);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].path, dir.path().join("src/a.rs"));
    }

    #[test]
    fn skips_binary_files() {
        let dir = TempDir::new();
        std::fs::write(dir.path().join("blob.bin"), b"needle\0needle").unwrap();
        dir.write("note.txt", "needle\n");
        let hits = find(dir.path(), "needle", true);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].path, dir.path().join("note.txt"));
    }

    #[test]
    fn columns_count_characters_not_bytes() {
        let dir = TempDir::new();
        dir.write("u.txt", "héllo wörld needle\n");
        let hits = find(dir.path(), "needle", true);
        assert_eq!(hits[0].col, 12);
        assert_eq!(hits[0].text, "héllo wörld needle");
    }

    #[test]
    fn a_missing_root_yields_no_results() {
        assert!(find(Path::new("/definitely/not/here"), "x", true).is_empty());
    }
}
