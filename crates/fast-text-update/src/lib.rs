//! Release checking and self-update for the Fast Text editors.
//!
//! The editors ask GitHub's API for the newest published release, compare it
//! with their own version, and — when the user says yes — download the
//! platform's release archive, unpack it, and swap the running binaries in
//! place. Everything is done with the standard library alone: HTTP goes
//! through `curl` (which ships with Windows 10+ and every Unix we support)
//! with `wget` as the fallback, and unpacking goes through `tar` (with
//! PowerShell's `Expand-Archive` as the Windows fallback).
//!
//! Frontends pass their own `env!("CARGO_PKG_VERSION")` to [`is_newer`] so
//! this crate never has to assume its version matches the editor's.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

/// The GitHub repository that publishes Fast Text releases.
pub const REPO: &str = "Fast-Text-editor/Fast-text-editor";

/// How long an automatic startup check waits between two attempts.
pub const CHECK_INTERVAL_SECS: u64 = 24 * 60 * 60;

/// Every executable the release archives contain.
pub const ALL_BINARIES: &[&str] = &["fast-text", "ft", "textwrite", "tw", "fast-text-gui"];

/// A published GitHub release.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Release {
    /// The release tag, e.g. `v0.2.0`.
    pub tag: String,
    /// The tag without its leading `v`, e.g. `0.2.0`.
    pub version: String,
}

impl Release {
    /// Builds a release from a bare tag.
    pub fn from_tag(tag: impl Into<String>) -> Self {
        let tag = tag.into();
        let version = tag.trim_start_matches(['v', 'V']).to_string();
        Self { tag, version }
    }
}

/// True when `candidate` names a newer release than `current`.
///
/// Both are version numbers with an optional leading `v`; up to three
/// numeric components are compared and anything after a `-` (a pre-release
/// suffix) is ignored.
pub fn is_newer(candidate: &str, current: &str) -> bool {
    parse_version(candidate) > parse_version(current)
}

/// Splits a version string into up to three numeric components.
fn parse_version(text: &str) -> (u64, u64, u64) {
    let mut parts = text
        .trim()
        .trim_start_matches(['v', 'V'])
        .split(|c: char| !c.is_ascii_digit())
        .filter(|s| !s.is_empty());
    let mut next = || {
        parts
            .next()
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(0)
    };
    (next(), next(), next())
}

/// The release archive this build would be shipped in, e.g.
/// `fast-text-x86_64-unknown-linux-gnu.tar.gz`.
///
/// Returns `None` for platforms without a prebuilt release; those users
/// update by rebuilding from source.
pub fn target_triple() -> Option<&'static str> {
    match (std::env::consts::ARCH, std::env::consts::OS) {
        ("x86_64", "linux") => Some("x86_64-unknown-linux-gnu"),
        ("aarch64", "linux") => Some("aarch64-unknown-linux-gnu"),
        ("x86_64", "macos") => Some("x86_64-apple-darwin"),
        ("aarch64", "macos") => Some("aarch64-apple-darwin"),
        ("x86_64", "windows") => Some("x86_64-pc-windows-msvc"),
        _ => None,
    }
}

/// The archive name for a target triple: `.zip` on Windows, `.tar.gz` elsewhere.
pub fn asset_name(target: &str) -> String {
    if cfg!(windows) {
        format!("fast-text-{target}.zip")
    } else {
        format!("fast-text-{target}.tar.gz")
    }
}

/// Asks GitHub for the newest published release.
///
/// `Ok(None)` means the repository has no release *yet* — not an error, just
/// nothing to upgrade to.
pub fn latest_release(timeout_secs: u64) -> Result<Option<Release>, String> {
    let url = format!("https://api.github.com/repos/{REPO}/releases/latest");
    let dest = scratch_file("latest-release.json");
    let code = http_get(&url, timeout_secs, &dest)?;
    let body = std::fs::read(&dest).unwrap_or_default();
    let _ = std::fs::remove_file(&dest);
    match code {
        200 => {
            let json = String::from_utf8_lossy(&body);
            match extract_tag_name(&json) {
                Some(tag) => Ok(Some(Release::from_tag(tag))),
                None => Err("could not read the release tag from GitHub's answer".to_string()),
            }
        }
        404 => Ok(None),
        0 => Err("the connection to GitHub failed".to_string()),
        code => Err(format!("GitHub answered with HTTP {code}")),
    }
}

/// Pulls the `"tag_name"` field out of a GitHub API response.
fn extract_tag_name(json: &str) -> Option<String> {
    let key = json.find("\"tag_name\"")?;
    let rest = json[key + "\"tag_name\"".len()..].trim_start();
    let rest = rest.strip_prefix(':')?.trim_start();
    let rest = rest.strip_prefix('"')?;
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

/// Downloads the release archive for `tag` and swaps the binaries next to the
/// running executable. Returns the names of the executables it replaced.
///
/// `on_progress` receives short human-readable updates ("Downloading…",
/// "Unpacking…", "Installing…") that frontends show in their dialog.
pub fn download_and_install(
    tag: &str,
    on_progress: &mut dyn Fn(&str),
) -> Result<Vec<String>, String> {
    let target = target_triple().ok_or_else(|| {
        format!(
            "no prebuilt release for {}-{}; rebuild from source instead",
            std::env::consts::ARCH,
            std::env::consts::OS
        )
    })?;
    let asset = asset_name(target);
    let url = format!("https://github.com/{REPO}/releases/download/{tag}/{asset}");
    let dir = scratch_dir(tag);

    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;

    on_progress(&format!("Downloading {asset}…"));
    let archive = dir.join(&asset);
    let code = http_get(&url, 600, &archive)?;
    if code != 200 {
        let _ = std::fs::remove_dir_all(&dir);
        return Err(match code {
            404 => format!("no prebuilt release named {asset} in {tag}"),
            0 => "the download from GitHub failed".to_string(),
            code => format!("the download failed (HTTP {code})"),
        });
    }

    on_progress("Unpacking…");
    let unpacked = dir.join("unpacked");
    std::fs::create_dir_all(&unpacked).map_err(|e| format!("cannot unpack: {e}"))?;
    unpack(&archive, &unpacked)?;
    let binaries = find_binaries(&unpacked, target)?;

    on_progress("Installing…");
    let exe =
        std::env::current_exe().map_err(|e| format!("cannot find the running program: {e}"))?;
    let install_dir = exe
        .parent()
        .ok_or_else(|| "the running program has no directory".to_string())?;
    let mut replaced = Vec::new();
    for name in ALL_BINARIES {
        let Some(source) = binaries.get(*name) else {
            continue;
        };
        let destination = install_dir.join(exe_file_name(name));
        // Only touch what is already installed (plus ourselves).
        if !destination.exists() && destination != exe {
            continue;
        }
        place(source, &destination)?;
        replaced.push((*name).to_string());
    }
    let _ = std::fs::remove_dir_all(&dir);
    Ok(replaced)
}

/// Locates the executables inside an unpacked archive.
fn find_binaries(root: &Path, target: &str) -> Result<HashMap<String, PathBuf>, String> {
    // The workflow packs everything under `fast-text-<target>/`; older or
    // hand-made archives may sit the files right at the top.
    let preferred = root.join(format!("fast-text-{target}"));
    let dir = if preferred.is_dir() {
        preferred
    } else {
        root.to_path_buf()
    };
    let mut map = HashMap::new();
    for name in ALL_BINARIES {
        let file = exe_file_name(name);
        for candidate in [dir.join(&file), root.join(&file)] {
            if candidate.is_file() {
                map.insert((*name).to_string(), candidate);
                break;
            }
        }
    }
    if map.is_empty() {
        Err("the downloaded archive contained no Fast Text executables".to_string())
    } else {
        Ok(map)
    }
}

/// Moves `source` over `destination`.
fn place(source: &Path, destination: &Path) -> Result<(), String> {
    // Windows cannot overwrite a running executable, so the old file is
    // renamed aside first. The rename is harmless on Unix too, and the stale
    // copy is deleted on the next start by [`cleanup_old_binaries`].
    if cfg!(windows) && destination.exists() {
        let stale = stale_path(destination);
        let _ = std::fs::remove_file(&stale);
        std::fs::rename(destination, &stale)
            .map_err(|e| format!("cannot set aside {}: {e}", destination.display()))?;
    }
    if let Err(rename_err) = std::fs::rename(source, destination) {
        // The temp directory may be on another filesystem: copy beside the
        // destination and rename into place, which also works where directly
        // overwriting a running executable would not.
        let staging = staging_path(destination);
        std::fs::copy(source, &staging)
            .map_err(|e| format!("cannot write {}: {e}", staging.display()))?;
        make_executable(&staging);
        std::fs::rename(&staging, destination).map_err(|e| {
            format!(
                "cannot replace {} ({rename_err}); the second try said: {e}",
                destination.display()
            )
        })?;
        let _ = std::fs::remove_file(source);
    }
    Ok(())
}

/// Marks a file executable on Unix; a no-op elsewhere.
fn make_executable(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755));
    }
    #[cfg(not(unix))]
    let _ = path;
}

/// The `.old` twin of a binary renamed aside during an update.
fn stale_path(destination: &Path) -> PathBuf {
    let name = destination
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    destination.with_file_name(format!("{name}.old"))
}

/// A temporary name beside a binary, used when a plain rename is impossible.
fn staging_path(destination: &Path) -> PathBuf {
    let name = destination
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    destination.with_file_name(format!("{name}.new"))
}

/// Deletes the `.old` twins a previous self-update left beside the running
/// executable. Windows cannot remove a running executable, which is why the
/// clean-up happens on the *next* start.
pub fn cleanup_old_binaries() -> Vec<PathBuf> {
    match std::env::current_exe() {
        Ok(exe) => match exe.parent() {
            Some(dir) => cleanup_stale_in(dir),
            None => Vec::new(),
        },
        Err(_) => Vec::new(),
    }
}

/// Removes `*.old` files in `dir` that shadow one of [`ALL_BINARIES`].
pub fn cleanup_stale_in(dir: &Path) -> Vec<PathBuf> {
    let mut removed = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return removed;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(stem) = name.to_str().and_then(|name| name.strip_suffix(".old")) else {
            continue;
        };
        if !ALL_BINARIES.iter().any(|base| stem == exe_file_name(base)) {
            continue;
        }
        let path = entry.path();
        if std::fs::remove_file(&path).is_ok() {
            removed.push(path);
        }
    }
    removed
}

/// A command that starts the current program again with the same arguments —
/// the last step of an update.
pub fn restart_command() -> std::io::Result<Command> {
    let exe = std::env::current_exe()?;
    let mut command = Command::new(exe);
    command.args(std::env::args_os().skip(1));
    Ok(command)
}

/// The on-disk record of past update checks: when we last asked, which
/// release GitHub named, and which one the user declined.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct CheckRecord {
    /// Unix time of the last automatic check.
    pub last_check: Option<u64>,
    /// The newest release GitHub reported at that time.
    pub latest: Option<String>,
    /// A release tag the user answered "no" to.
    pub declined: Option<String>,
}

/// Reads a [`CheckRecord`]; a missing file is simply an empty record.
pub fn load_record(path: &Path) -> CheckRecord {
    let mut record = CheckRecord::default();
    let Ok(text) = std::fs::read_to_string(path) else {
        return record;
    };
    for line in text.lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let value = value.trim();
        match key {
            "last_check" => record.last_check = value.parse().ok(),
            "latest" => record.latest = Some(value.to_string()),
            "declined" => record.declined = Some(value.to_string()),
            _ => {}
        }
    }
    record
}

/// Writes a [`CheckRecord`], creating parent directories as needed.
pub fn save_record(path: &Path, record: &CheckRecord) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        if !dir.as_os_str().is_empty() {
            std::fs::create_dir_all(dir)?;
        }
    }
    let mut text = String::new();
    if let Some(last) = record.last_check {
        text.push_str(&format!("last_check={last}\n"));
    }
    if let Some(latest) = &record.latest {
        text.push_str(&format!("latest={latest}\n"));
    }
    if let Some(declined) = &record.declined {
        text.push_str(&format!("declined={declined}\n"));
    }
    std::fs::write(path, text)
}

/// Seconds since the Unix epoch.
pub fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// The file name an executable has on this platform.
fn exe_file_name(name: &str) -> String {
    if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_string()
    }
}

/// A unique scratch file path inside the system temp directory.
fn scratch_file(label: &str) -> PathBuf {
    std::env::temp_dir().join(format!("fast-text-update-{}-{label}", std::process::id()))
}

/// A unique scratch directory inside the system temp directory.
fn scratch_dir(tag: &str) -> PathBuf {
    let safe: String = tag
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || ".-_".contains(c) {
                c
            } else {
                '_'
            }
        })
        .collect();
    std::env::temp_dir().join(format!("fast-text-update-{}-{safe}", std::process::id()))
}

/// Downloads `url` into `dest` and returns the HTTP status code.
///
/// `curl` is tried first (it ships with Windows 10+ and every supported Unix);
/// `wget` is the fallback. A returned `0` means "a tool ran but the status is
/// unknown"; `Err` means no tool could be used at all.
fn http_get(url: &str, timeout_secs: u64, dest: &Path) -> Result<u16, String> {
    let timeout = timeout_secs.to_string();
    if let Ok(out) = Command::new("curl")
        .args(["-sS", "-L", "--max-time", &timeout, "-o"])
        .arg(dest)
        .arg("-w")
        .arg("%{http_code}")
        .arg(url)
        .output()
    {
        if !out.status.success() && out.stdout.is_empty() {
            let detail = String::from_utf8_lossy(&out.stderr);
            return Err(format!("curl failed: {}", detail.trim()));
        }
        let code = String::from_utf8_lossy(&out.stdout)
            .trim()
            .parse::<u16>()
            .unwrap_or(0);
        return Ok(code);
    }
    if let Ok(out) = Command::new("wget")
        .args(["-q", "-S"])
        .arg(format!("-T{timeout}"))
        .arg("-O")
        .arg(dest)
        .arg(url)
        .output()
    {
        if !out.status.success() {
            let stderr = String::from_utf8_lossy(&out.stderr);
            return Ok(http_status_from(&stderr).unwrap_or(0));
        }
        return Ok(200);
    }
    Err("checking for updates needs curl or wget on PATH".to_string())
}

/// Finds the last HTTP status line in `wget -S` output, e.g.
/// `  HTTP/1.1 404 Not Found`.
fn http_status_from(stderr: &str) -> Option<u16> {
    stderr
        .lines()
        .filter_map(|line| {
            let mut words = line.split_whitespace();
            let head = words.next()?;
            if !head.starts_with("HTTP/") {
                return None;
            }
            words.next()?.parse::<u16>().ok()
        })
        .next_back()
}

/// Unpacks `archive` into `into`: `tar` everywhere, PowerShell on Windows
/// systems where `tar` is missing.
fn unpack(archive: &Path, into: &Path) -> Result<(), String> {
    let flag = if archive.extension().and_then(|e| e.to_str()) == Some("zip") {
        "-xf"
    } else {
        "-xzf"
    };
    match Command::new("tar")
        .arg(flag)
        .arg(archive)
        .current_dir(into)
        .output()
    {
        Ok(out) if out.status.success() => Ok(()),
        Ok(out) => expand_archive(archive, into, Some(&out)),
        Err(e) => expand_archive(archive, into, None)
            .map_err(|_| format!("unpacking needs tar on PATH: {e}")),
    }
}

fn expand_archive(
    archive: &Path,
    into: &Path,
    tar_error: Option<&std::process::Output>,
) -> Result<(), String> {
    let script = format!(
        "Expand-Archive -LiteralPath '{}' -DestinationPath '{}' -Force",
        archive.display(),
        into.display()
    );
    for shell in ["pwsh", "powershell"] {
        if let Ok(out) = Command::new(shell)
            .args(["-NoProfile", "-NonInteractive", "-Command", &script])
            .output()
        {
            if out.status.success() {
                return Ok(());
            }
        }
    }
    let detail = match tar_error {
        Some(out) => format!(": {}", String::from_utf8_lossy(&out.stderr).trim()),
        None => String::new(),
    };
    Err(format!(
        "unpacking the update needs tar or PowerShell{detail}"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_comparison_is_numeric_not_lexicographic() {
        assert!(is_newer("0.2.0", "0.1.0"));
        assert!(is_newer("v0.10.0", "v0.9.0"));
        assert!(is_newer("1.0.0", "0.99.99"));
        assert!(!is_newer("0.1.0", "0.1.0"));
        assert!(!is_newer("0.0.9", "0.1.0"));
    }

    #[test]
    fn version_comparison_handles_tags_and_short_versions() {
        assert!(is_newer("v0.2.0", "0.1"));
        assert!(is_newer("0.2", "0.1.9"));
        assert!(!is_newer("v0.1.0", "0.1.0"));
        // A missing component counts as zero, so 0.2 and 0.2.0 are the same.
        assert!(!is_newer("0.2.0", "0.2"));
        assert!(is_newer("0.2.1", "0.2"));
        // Pre-release suffixes do not outrank the plain version.
        assert!(!is_newer("0.2.0-rc1", "0.2.0"));
        assert!(is_newer("0.2.1-rc1", "0.2.0"));
    }

    #[test]
    fn a_release_remembers_both_spellings_of_its_version() {
        let release = Release::from_tag("v1.2.3");
        assert_eq!(release.tag, "v1.2.3");
        assert_eq!(release.version, "1.2.3");
        assert_eq!(Release::from_tag("4.5").version, "4.5");
    }

    #[test]
    fn the_tag_name_comes_out_of_a_github_answer() {
        let json = r#"{
            "url": "https://api.github.com/repos/x/releases/1",
            "tag_name": "v0.2.0",
            "name": "Fast Text 0.2.0",
            "draft": false
        }"#;
        assert_eq!(extract_tag_name(json).as_deref(), Some("v0.2.0"));
        assert_eq!(
            extract_tag_name(r#"{"tag_name":"v9"}"#).as_deref(),
            Some("v9")
        );
        assert_eq!(extract_tag_name(r#"{"name":"no tag here"}"#), None);
    }

    #[test]
    fn the_asset_matches_this_platform() {
        let target = target_triple().expect("this platform has a prebuilt release");
        let asset = asset_name(target);
        assert!(asset.starts_with("fast-text-"), "unexpected asset: {asset}");
        assert!(
            asset.ends_with(".tar.gz") || asset.ends_with(".zip"),
            "unexpected asset: {asset}"
        );
        assert!(asset.contains(target));
    }

    #[test]
    fn a_check_record_round_trips() {
        let path = std::env::temp_dir().join(format!(
            "fast-text-update-record-test-{}",
            std::process::id()
        ));
        let record = CheckRecord {
            last_check: Some(1_700_000_000),
            latest: Some("v0.2.0".to_string()),
            declined: Some("v0.2.0".to_string()),
        };
        save_record(&path, &record).unwrap();
        assert_eq!(load_record(&path), record);
        let _ = std::fs::remove_file(&path);

        let missing = path.with_file_name("definitely-not-here");
        assert_eq!(load_record(&missing), CheckRecord::default());
    }

    #[test]
    fn stale_twins_of_our_binaries_are_cleaned_up() {
        let dir = std::env::temp_dir().join(format!(
            "fast-text-update-clean-test-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let suffix = if cfg!(windows) { ".exe.old" } else { ".old" };
        let ours = dir.join(format!("ft{suffix}"));
        let foreign = dir.join("notes.txt.old");
        std::fs::write(&ours, b"stale").unwrap();
        std::fs::write(&foreign, b"keep me").unwrap();

        let removed = cleanup_stale_in(&dir);
        assert!(removed.contains(&ours), "should have removed {ours:?}");
        assert!(!ours.exists());
        assert!(foreign.exists(), "unrelated .old files must survive");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn placing_a_binary_replaces_the_destination() {
        let dir = std::env::temp_dir().join(format!(
            "fast-text-update-place-test-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let source = dir.join("source-bin");
        let destination = dir.join("installed-bin");
        std::fs::write(&source, b"new").unwrap();
        std::fs::write(&destination, b"old").unwrap();

        place(&source, &destination).unwrap();
        assert_eq!(std::fs::read(&destination).unwrap(), b"new");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn http_status_lines_are_found_in_wget_output() {
        let stderr = "  HTTP/1.1 302 Found\n  HTTP/1.1 404 Not Found\n";
        assert_eq!(http_status_from(stderr), Some(404));
        assert_eq!(http_status_from("no status here"), None);
    }
}
