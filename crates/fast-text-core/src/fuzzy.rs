//! Fuzzy matching and workspace file discovery.
//!
//! [`fuzzy_match`] is the shared scoring function used by every picker: the
//! terminal and desktop file finders, the command palette, and the web build.
//! [`FileFinder`] adds filesystem discovery on top, walking a workspace while
//! honouring `.gitignore` (via the `ignore` crate) so generated directories and
//! vendored trees never clutter the list.
//!
//! The matcher is deliberately cheap: `fuzzy_match` scores a single candidate,
//! while [`FileFinder::search`] scores, sorts and truncates a whole workspace.
//! Both are pure functions for a given file list, so the tests are offline and
//! reproducible.

use std::path::PathBuf;

#[cfg(feature = "fs")]
use std::path::Path;

use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;

/// Scores `candidate` against `query`; higher is better, `None` means no match.
///
/// The matcher is smart-case: an all-lowercase query matches case-insensitively,
/// while an uppercase letter in the query requires an uppercase letter in the
/// candidate. A blank query matches everything with a score of zero.
pub fn fuzzy_match(query: &str, candidate: &str) -> Option<i64> {
    if query.trim().is_empty() {
        return Some(0);
    }
    let matcher = SkimMatcherV2::default();
    matcher.fuzzy_match(candidate, query)
}

/// One file in a fuzzy search result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileHit {
    /// The absolute path of the file.
    pub path: PathBuf,
    /// The path relative to the workspace root, with `/` separators.
    pub relative: String,
    /// The match score (higher is better).
    pub score: i64,
}

/// A workspace file index for the fuzzy finder.
///
/// Building the index walks the tree once; every later keystroke only scores
/// the cached list, which keeps the picker responsive even in large repos.
#[cfg(feature = "fs")]
#[derive(Debug, Clone)]
pub struct FileFinder {
    root: PathBuf,
    files: Vec<(PathBuf, String)>,
}

#[cfg(feature = "fs")]
impl FileFinder {
    /// Walks `root`, skipping ignored and hidden entries.
    ///
    /// Symbolic links are not followed, so a link cycle cannot hang startup.
    pub fn new(root: impl AsRef<Path>) -> Self {
        let root = root.as_ref().to_path_buf();
        let mut finder = Self {
            root,
            files: Vec::new(),
        };
        finder.refresh();
        finder
    }

    /// Re-walks the workspace, picking up new and deleted files.
    pub fn refresh(&mut self) {
        let mut files = Vec::new();
        let walker = ignore::WalkBuilder::new(&self.root)
            .hidden(true)
            .follow_links(false)
            .git_ignore(true)
            .git_global(true)
            .git_exclude(true)
            .parents(true)
            .build();
        for entry in walker.flatten() {
            let Some(file_type) = entry.file_type() else {
                continue;
            };
            if !file_type.is_file() {
                continue;
            }
            let path = entry.into_path();
            let relative = path
                .strip_prefix(&self.root)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            files.push((path, relative));
        }
        files.sort_by(|a, b| a.1.cmp(&b.1));
        self.files = files;
    }

    /// The workspace root.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Every indexed file, in path order.
    pub fn files(&self) -> &[(PathBuf, String)] {
        &self.files
    }

    /// Scores every indexed file against `query`, best first.
    ///
    /// Ties break toward shorter paths, so `lib.rs` beats
    /// `vendor/something/lib.rs` for the query `lib`.
    pub fn search(&self, query: &str, limit: usize) -> Vec<FileHit> {
        search_paths(&self.files, query, limit)
    }
}

/// Scores and sorts `(path, relative)` pairs. Used by the finder and the tests.
#[cfg(feature = "fs")]
pub fn search_paths(files: &[(PathBuf, String)], query: &str, limit: usize) -> Vec<FileHit> {
    if limit == 0 {
        return Vec::new();
    }
    let mut hits: Vec<FileHit> = files
        .iter()
        .filter_map(|(path, relative)| {
            let score = fuzzy_match(query, relative)?;
            Some(FileHit {
                path: path.clone(),
                relative: relative.clone(),
                score,
            })
        })
        .collect();
    hits.sort_by(|a, b| {
        b.score
            .cmp(&a.score)
            .then_with(|| a.relative.len().cmp(&b.relative.len()))
            .then_with(|| a.relative.cmp(&b.relative))
    });
    hits.truncate(limit);
    hits
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fuzzy_matching_is_smart_case() {
        assert!(fuzzy_match("abc", "a/b/c.rs").is_some());
        assert!(fuzzy_match("ABC", "a/b/c.rs").is_none());
        assert!(fuzzy_match("ABC", "ABC.rs").is_some());
        assert!(fuzzy_match("", "anything").is_some());
        assert!(fuzzy_match("zzz", "a/b/c.rs").is_none());
    }

    #[test]
    fn better_matches_score_higher() {
        let direct = fuzzy_match("main", "main.rs").unwrap();
        let scattered = fuzzy_match("main", "vendor/misc/ain.txt").unwrap();
        assert!(direct > scattered, "{direct} vs {scattered}");
    }

    #[cfg(feature = "fs")]
    mod fs {
        use super::*;

        fn temp_dir(name: &str) -> PathBuf {
            let dir = std::env::temp_dir().join(format!(
                "fast-text-fuzzy-{name}-{}-{:?}",
                std::process::id(),
                std::thread::current().id()
            ));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            dir
        }

        #[test]
        fn walker_respects_gitignore_and_skips_hidden() {
            let root = temp_dir("gitignore");
            std::fs::create_dir_all(root.join("src")).unwrap();
            std::fs::create_dir_all(root.join("target")).unwrap();
            std::fs::create_dir_all(root.join(".git")).unwrap();
            std::fs::write(root.join(".gitignore"), "target/\n").unwrap();
            std::fs::write(root.join("src/main.rs"), "fn main() {}").unwrap();
            std::fs::write(root.join("target/junk.rs"), "fn main() {}").unwrap();
            std::fs::write(root.join(".hidden.rs"), "").unwrap();
            std::fs::write(root.join("README.md"), "").unwrap();

            let finder = FileFinder::new(&root);
            let names: Vec<&str> = finder.files().iter().map(|(_, r)| r.as_str()).collect();
            assert!(names.contains(&"src/main.rs"), "{names:?}");
            assert!(names.contains(&"README.md"), "{names:?}");
            assert!(!names.iter().any(|n| n.contains("junk")), "{names:?}");
            assert!(!names.iter().any(|n| n.starts_with('.')), "{names:?}");
            let _ = std::fs::remove_dir_all(&root);
        }

        #[test]
        fn search_orders_and_truncates() {
            let root = temp_dir("search");
            std::fs::create_dir_all(root.join("src")).unwrap();
            std::fs::create_dir_all(root.join("vendor/deep")).unwrap();
            std::fs::write(root.join("src/main.rs"), "").unwrap();
            std::fs::write(root.join("vendor/deep/main.rs"), "").unwrap();
            std::fs::write(root.join("README.md"), "").unwrap();

            let finder = FileFinder::new(&root);
            let hits = finder.search("main", 10);
            assert_eq!(hits.len(), 2);
            assert_eq!(hits[0].relative, "src/main.rs");
            assert!(hits[0].score >= hits[1].score);
            assert_eq!(finder.search("main", 1).len(), 1);
            assert!(finder.search("nothere", 10).is_empty());
            assert!(finder.search("main", 0).is_empty());
            let _ = std::fs::remove_dir_all(&root);
        }

        #[test]
        fn refresh_sees_new_files() {
            let root = temp_dir("refresh");
            std::fs::write(root.join("one.txt"), "").unwrap();
            let mut finder = FileFinder::new(&root);
            assert_eq!(finder.files().len(), 1);
            std::fs::write(root.join("two.txt"), "").unwrap();
            finder.refresh();
            assert_eq!(finder.files().len(), 2);
            let _ = std::fs::remove_dir_all(&root);
        }
    }
}
