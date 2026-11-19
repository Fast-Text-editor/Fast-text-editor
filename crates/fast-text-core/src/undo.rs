//! Persistent undo history.
//!
//! The in-memory undo stack normally dies with the process. When an editor
//! enables persistence, it stores a [`Session`] — the buffer text, the
//! selections and the whole undo/redo stack — under the user's cache directory,
//! keyed by the file's absolute path. Reopening the file restores the stack, so
//! a closed terminal window no longer means lost work.
//!
//! Sessions are plain JSON written through `serde_json`; nothing in this module
//! touches the network, and failures are returned rather than panicking, since
//! a read-only or full cache directory must never stop the editor.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::{Document, History, Selection};

/// The largest number of transactions written to disk per session.
///
/// A long editing session can accumulate thousands of small transactions.
/// Keeping the newest few hundred bounds the cache file to a reasonable size
/// while still preserving plenty of undo depth.
pub const MAX_PERSISTED_TRANSACTIONS: usize = 200;

/// A serializable editing session: everything needed to resume exactly where
/// the user left off.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    /// The full buffer text at the time of the snapshot.
    pub text: String,
    /// The primary selection.
    pub selection: Selection,
    /// Secondary selections for multi-cursor editing.
    #[serde(default)]
    pub extra: Vec<Selection>,
    /// The undo/redo stack.
    pub history: History,
    /// The absolute path this session belongs to, when it has one.
    #[serde(default)]
    pub path: Option<String>,
    /// Unix seconds when the snapshot was taken.
    #[serde(default)]
    pub saved_at: u64,
}

/// The directory holding undo snapshots.
///
/// `FAST_TEXT_CACHE_DIR` wins (useful for tests and portable installs), then
/// `$XDG_CACHE_HOME/fast-text/undo`, then `~/.cache/fast-text/undo`, and
/// finally the system temporary directory.
pub fn cache_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("FAST_TEXT_CACHE_DIR").filter(|v| !v.is_empty()) {
        return PathBuf::from(dir).join("undo");
    }
    if let Some(dir) = std::env::var_os("XDG_CACHE_HOME").filter(|v| !v.is_empty()) {
        return PathBuf::from(dir).join("fast-text").join("undo");
    }
    if let Some(home) = std::env::var_os("HOME").filter(|v| !v.is_empty()) {
        return PathBuf::from(home)
            .join(".cache")
            .join("fast-text")
            .join("undo");
    }
    std::env::temp_dir().join("fast-text").join("undo")
}

/// Resolves a path as far as the filesystem allows, then makes it absolute.
///
/// macOS reports `/var/...` for `std::env::temp_dir()` but canonicalizes it to
/// `/private/var/...`, and Windows canonicalization adds a `\\?\` prefix. Using
/// one normalized form for both the cache key and the recorded path keeps
/// snapshots loadable through any spelling of the same file.
fn canonical_path(path: &Path) -> PathBuf {
    if let Ok(canonical) = std::fs::canonicalize(path) {
        return canonical;
    }
    if path.is_absolute() {
        return path.to_path_buf();
    }
    std::env::current_dir()
        .map(|dir| dir.join(path))
        .unwrap_or_else(|_| path.to_path_buf())
}

/// Whether a recorded session path still identifies `document_path`.
fn paths_match(recorded: &str, document_path: &Path) -> bool {
    let expected = canonical_path(document_path);
    let recorded = Path::new(recorded);
    recorded == expected || canonical_path(recorded) == expected
}

/// A stable FNV-1a hash, used to turn a path into a file name.
fn path_hash(text: &str) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in text.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// The snapshot path for a document path, or `None` for an untitled buffer.
pub fn path_for(document: &Path) -> Option<PathBuf> {
    let key = canonical_path(document).to_string_lossy().into_owned();
    Some(cache_dir().join(format!("{:016x}.json", path_hash(&key))))
}

/// Writes the document's snapshot, returning the file it wrote.
///
/// Untitled buffers have no stable identity, so they are skipped (`Ok(None)`).
pub fn save(document: &Document) -> crate::Result<Option<PathBuf>> {
    let Some(document_path) = document.path() else {
        return Ok(None);
    };
    let Some(target) = path_for(document_path) else {
        return Ok(None);
    };
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            crate::Error::io(
                None,
                std::io::Error::new(e.kind(), format!("cannot create {}: {e}", parent.display())),
            )
        })?;
    }
    let mut session = document.snapshot();
    // Record the canonical spelling so `load` can compare like-for-like even
    // when the caller opened the file through a symlink or a relative path.
    session.path = Some(canonical_path(document_path).to_string_lossy().into_owned());
    // Keep only the newest transactions so a marathon session cannot produce a
    // multi-megabyte cache file.
    let undo = session.history.undo_transactions();
    if undo.len() > MAX_PERSISTED_TRANSACTIONS {
        let keep = &undo[undo.len() - MAX_PERSISTED_TRANSACTIONS..];
        session.history = History::from_parts(
            keep.to_vec(),
            session.history.redo_transactions().to_vec(),
            session.history.limit(),
        );
    }
    let json = serde_json::to_vec(&session).map_err(|e| {
        crate::Error::io(
            None,
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("cannot serialize undo history: {e}"),
            ),
        )
    })?;
    // Write to a sibling then rename, so a crash mid-write cannot leave a
    // half-written snapshot that fails to parse on the next launch.
    let temporary = target.with_extension("json.tmp");
    std::fs::write(&temporary, json).map_err(|e| {
        crate::Error::io(
            None,
            std::io::Error::new(
                e.kind(),
                format!("cannot write {}: {e}", temporary.display()),
            ),
        )
    })?;
    std::fs::rename(&temporary, &target).map_err(|e| {
        crate::Error::io(
            None,
            std::io::Error::new(
                e.kind(),
                format!("cannot replace {}: {e}", target.display()),
            ),
        )
    })?;
    Ok(Some(target))
}

/// Loads the snapshot for `document_path`, if one exists and matches.
///
/// A corrupt or stale snapshot is ignored (and removed), never fatal.
pub fn load(document_path: &Path) -> Option<Document> {
    let target = path_for(document_path)?;
    let bytes = std::fs::read(&target).ok()?;
    let session: Session = serde_json::from_slice(&bytes).ok()?;
    match session.path.as_deref() {
        // The hash already pins the path, but check the recorded path too so a
        // hash collision can never restore the wrong document. Compare through
        // each path's canonical form: `save` records one, a caller may pass the
        // other (notably `/var` vs `/private/var` on macOS).
        Some(recorded) if paths_match(recorded, document_path) => {}
        Some(_) => return None,
        None => {}
    }
    let mut document = Document::restore(session);
    document
        .buffer_mut()
        .set_path(canonical_path(document_path));
    Some(document)
}

/// Removes the snapshot for a document path, if any.
pub fn clear(document_path: &Path) -> std::io::Result<()> {
    let Some(target) = path_for(document_path) else {
        return Ok(());
    };
    match std::fs::remove_file(&target) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Transaction, TxnKind};
    use std::sync::{Mutex, MutexGuard};

    /// Tests share the process environment, so only one may set the cache
    /// directory at a time.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn isolated_dir(name: &str) -> (MutexGuard<'static, ()>, PathBuf) {
        let guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = std::env::temp_dir().join(format!(
            "fast-text-undo-{name}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::env::set_var("FAST_TEXT_CACHE_DIR", &dir);
        (guard, dir)
    }

    #[test]
    fn snapshot_round_trips_text_selection_and_history() {
        let (_guard, dir) = isolated_dir("roundtrip");
        let path = dir.join("note.txt");
        std::fs::write(&path, "one").unwrap();
        let mut doc = Document::from_file(&path).unwrap();
        doc.set_cursor(3);
        doc.insert_str(" two");
        doc.insert_str(" three");
        assert!(doc.can_undo());

        let written = save(&doc).unwrap().expect("snapshot path");
        assert!(written.exists());

        let restored = load(&path).expect("snapshot");
        assert_eq!(restored.text(), "one two three");
        assert_eq!(restored.selection(), doc.selection());
        // Every step is still undoable after the restart.
        let mut restored = restored;
        restored.undo();
        assert_eq!(restored.text(), "one two");
        restored.undo();
        assert_eq!(restored.text(), "one");
        restored.redo();
        assert_eq!(restored.text(), "one two");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn untitled_documents_are_not_persisted() {
        let (_guard, dir) = isolated_dir("untitled");
        let doc = Document::from_str("scratch");
        assert!(save(&doc).unwrap().is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_corrupt_snapshot_is_ignored() {
        let (_guard, dir) = isolated_dir("corrupt");
        let path = dir.join("file.txt");
        std::fs::write(&path, "hello").unwrap();
        let target = path_for(&path).unwrap();
        std::fs::create_dir_all(target.parent().unwrap()).unwrap();
        std::fs::write(target, b"not json").unwrap();
        assert!(load(&path).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn clearing_removes_the_snapshot() {
        let (_guard, dir) = isolated_dir("clear");
        let path = dir.join("file.txt");
        std::fs::write(&path, "hello").unwrap();
        let doc = Document::from_file(&path).unwrap();
        let written = save(&doc).unwrap().unwrap();
        assert!(written.exists());
        clear(&path).unwrap();
        assert!(!written.exists());
        clear(&path).unwrap(); // idempotent
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn snapshots_load_through_a_non_canonical_alias() {
        let (_guard, dir) = isolated_dir("alias");
        let path = dir.join("file.txt");
        std::fs::write(&path, "hello").unwrap();
        let mut doc = Document::from_file(&path).unwrap();
        doc.set_cursor(doc.text().len());
        doc.insert_str(" world");
        let written = save(&doc).unwrap().unwrap();

        // Record and accept the canonical spelling, even when the caller uses
        // a dotted or symlinked path (macOS temp dirs are the common case).
        let session: Session = serde_json::from_slice(&std::fs::read(&written).unwrap()).unwrap();
        assert_eq!(
            session.path.as_deref(),
            Some(canonical_path(&path).to_str().unwrap())
        );
        let alias = dir.join(".").join("file.txt");
        let restored = load(&alias).expect("snapshot through an alias");
        assert_eq!(restored.text(), "hello world");
        assert_eq!(restored.path(), Some(canonical_path(&path).as_path()));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn long_histories_are_truncated_on_disk() {
        let (_guard, dir) = isolated_dir("truncate");
        let path = dir.join("long.txt");
        std::fs::write(&path, "").unwrap();
        let mut doc = Document::from_file(&path).unwrap();
        for i in 0..(MAX_PERSISTED_TRANSACTIONS + 20) {
            doc.insert_at(i, "x");
        }
        assert!(doc.history().undo_len() > MAX_PERSISTED_TRANSACTIONS);
        save(&doc).unwrap().unwrap();
        let restored = load(&path).unwrap();
        assert_eq!(
            restored.history().undo_len(),
            MAX_PERSISTED_TRANSACTIONS,
            "only the newest transactions should be stored"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn restored_selections_are_clamped_to_the_text() {
        let history = History::from_parts(
            vec![Transaction::new(TxnKind::Other, Selection::caret(0))],
            Vec::new(),
            10,
        );
        let session = Session {
            text: "abc".to_string(),
            selection: Selection::new(99, 99),
            extra: vec![Selection::caret(42)],
            history,
            path: None,
            saved_at: 0,
        };
        let doc = Document::restore(session);
        assert_eq!(doc.selection().head, 3);
        assert_eq!(doc.extra_selections()[0].head, 3);
    }
}
