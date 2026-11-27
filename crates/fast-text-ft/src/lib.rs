//! The `.ft` extension framework for Fast Text.
//!
//! `.ft` is a lightweight extension framework inspired by modern app platforms:
//! each extension is a package with a manifest (`ft.toml`) and one or more
//! TextWrite/Rhai scripts. Extensions can declare commands, hooks, themes, and
//! metadata, making them easy to share and install.
//!
//! # Package layout
//!
//! ```text
//! my-extension/
//!   ft.toml
//!   main.tw        # or main.ft
//!   lib/
//!     helpers.tw
//! ```
//!
//! # Manifest (`ft.toml`)
//!
//! ```toml
//! [extension]
//! name = "my-extension"
//! version = "0.1.0"
//! description = "Example extension"
//! author = "You"
//! main = "main.tw"
//!
//! [extension.hooks]
//! on_startup = true
//! on_buffer_open = true
//! ```

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Manifest file name inside an `.ft` extension package.
pub const MANIFEST_FILE: &str = "ft.toml";

/// Supported script extensions for .ft framework extensions.
pub const SCRIPT_EXTENSIONS: &[&str] = &["ft", "tw"];

/// An `.ft` extension manifest (`ft.toml`).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FtManifest {
    #[serde(default)]
    pub extension: ExtensionMeta,
    #[serde(default)]
    pub hooks: BTreeMap<String, bool>,
    #[serde(default)]
    pub commands: Vec<CommandDecl>,
    #[serde(default)]
    pub keybindings: Vec<KeybindingDecl>,
    #[serde(default)]
    pub permissions: Permissions,
}

/// Extension metadata.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtensionMeta {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub author: String,
    /// Main script file relative to the extension root. Defaults to `main.tw` or `main.ft`.
    #[serde(default)]
    pub main: Option<String>,
    /// Minimum Fast Text version (informational).
    #[serde(default)]
    pub min_fast_text: Option<String>,
}

/// A command declared in the manifest.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CommandDecl {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub description: String,
}

/// A keybinding declared in the manifest.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct KeybindingDecl {
    #[serde(default)]
    pub key: String,
    #[serde(default)]
    pub command: String,
}

/// Declared permissions (informational for now; enforced by host policy).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Permissions {
    #[serde(default)]
    pub filesystem: bool,
    #[serde(default)]
    pub shell: bool,
    #[serde(default)]
    pub network: bool,
}

/// A discovered .ft extension on disk.
#[derive(Debug, Clone)]
pub struct FtExtension {
    pub root: PathBuf,
    pub manifest: FtManifest,
    pub main_script: Option<PathBuf>,
}

impl FtExtension {
    /// Load an extension from a directory containing `ft.toml`.
    pub fn load_dir(dir: &Path) -> Result<Self, String> {
        let manifest_path = dir.join(MANIFEST_FILE);
        let manifest = if manifest_path.exists() {
            let text = std::fs::read_to_string(&manifest_path)
                .map_err(|e| format!("{}: {e}", manifest_path.display()))?;
            toml::from_str::<FtManifest>(&text)
                .map_err(|e| format!("{}: {e}", manifest_path.display()))?
        } else {
            // Allow manifest-less .ft dirs: infer from files.
            let mut m = FtManifest::default();
            m.extension.name = dir
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| "extension".to_string());
            m
        };

        let main_script = resolve_main_script(dir, &manifest);
        Ok(Self {
            root: dir.to_path_buf(),
            manifest,
            main_script,
        })
    }

    /// Display name for UI.
    pub fn display_name(&self) -> String {
        if !self.manifest.extension.name.is_empty() {
            self.manifest.extension.name.clone()
        } else if !self.manifest.extension.id.is_empty() {
            self.manifest.extension.id.clone()
        } else {
            self.root
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| "extension".to_string())
        }
    }
}

/// Resolve the main script for an extension.
fn resolve_main_script(dir: &Path, manifest: &FtManifest) -> Option<PathBuf> {
    if let Some(main) = &manifest.extension.main {
        let p = dir.join(main);
        if p.exists() {
            return Some(p);
        }
    }
    for candidate in ["main.ft", "main.tw", "init.ft", "init.tw"] {
        let p = dir.join(candidate);
        if p.exists() {
            return Some(p);
        }
    }
    // Fallback: any .ft/.tw file in root
    let mut entries: Vec<PathBuf> = std::fs::read_dir(dir)
        .ok()?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.extension()
                .and_then(|e| e.to_str())
                .map(|e| SCRIPT_EXTENSIONS.contains(&e))
                .unwrap_or(false)
        })
        .collect();
    entries.sort();
    entries.into_iter().next()
}

/// Discover .ft extension packages in a directory.
/// An extension package is a subdirectory containing `ft.toml` or .ft/.tw scripts.
pub fn discover_extensions(dir: &Path) -> Vec<FtExtension> {
    let mut out = Vec::new();
    let Ok(rd) = std::fs::read_dir(dir) else {
        return out;
    };
    for entry in rd.filter_map(|e| e.ok()) {
        let path = entry.path();
        if path.is_dir() {
            let has_manifest = path.join(MANIFEST_FILE).exists();
            let has_script = std::fs::read_dir(&path)
                .ok()
                .map(|rd| {
                    rd.filter_map(|e| e.ok()).any(|e| {
                        e.path()
                            .extension()
                            .and_then(|x| x.to_str())
                            .map(|x| SCRIPT_EXTENSIONS.contains(&x))
                            .unwrap_or(false)
                    })
                })
                .unwrap_or(false);
            if has_manifest || has_script {
                if let Ok(ext) = FtExtension::load_dir(&path) {
                    out.push(ext);
                }
            }
        }
    }
    out.sort_by(|a, b| a.display_name().cmp(&b.display_name()));
    out
}

/// Create a new `.ft` extension package scaffold at `dir/name`.
pub fn scaffold_extension(dir: &Path, name: &str) -> Result<PathBuf, String> {
    let safe_name: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect();
    let root = dir.join(&safe_name);
    std::fs::create_dir_all(&root).map_err(|e| format!("{}: {e}", root.display()))?;

    let manifest = format!(
        r#"[extension]
id = "{safe}"
name = "{name}"
version = "0.1.0"
description = "A Fast Text .ft extension"
author = ""
main = "main.ft"

[hooks]
on_startup = false
"#,
        safe = safe_name,
        name = name,
    );
    std::fs::write(root.join(MANIFEST_FILE), manifest).map_err(|e| format!("ft.toml: {e}"))?;

    let main = r#"// Fast Text .ft extension
// Use the editor API to register commands, keybindings, and hooks.

fn hello() {
    message("Hello from .ft!")
}

command_with_desc("FT: Hello", "Say hello from .ft", "hello")
"#;
    std::fs::write(root.join("main.ft"), main).map_err(|e| format!("main.ft: {e}"))?;

    Ok(root)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_parses() {
        let toml = r#"
[extension]
name = "demo"
version = "0.1.0"
description = "Demo extension"
main = "main.tw"

[hooks]
on_buffer_open = true
"#;
        let m: FtManifest = toml::from_str(toml).unwrap();
        assert_eq!(m.extension.name, "demo");
        assert_eq!(m.hooks.get("on_buffer_open"), Some(&true));
    }
}
