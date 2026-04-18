//! Shared types for the mod manager.
//!
//! These types cross the Tauri boundary — every `Serialize` variant maps to a
//! discriminator the frontend reads to render the right row treatment.

use serde::{Deserialize, Serialize};

fn non_empty(s: &str) -> Option<String> {
    if s.is_empty() { None } else { Some(s.to_string()) }
}

/// Which kind of mod this is. The frontend groups rows by this and renders a
/// different primary-action state machine for each.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModKind {
    /// Separate process (Shinra Meter, TCC). Lifecycle = download, extract,
    /// spawn, monitor. Does not touch game files.
    External,
    /// TMM-compatible `.gpk` pack that patches `CompositePackageMapper.dat`.
    /// Phase C — not yet implemented end-to-end.
    Gpk,
}

/// Per-row status in the Installed tab.
///
/// The primary-action cell in the UI reads this directly. `NotInstalled` only
/// appears in Browse-tab rows; Installed rows are always at least `Disabled`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModStatus {
    /// In the catalog but not yet downloaded.
    NotInstalled,
    /// Files on disk, mod is inert. External: process not running. GPK: not
    /// patched into the mapper.
    Disabled,
    /// External app only — process is running (exit code not yet observed).
    Running,
    /// External app only — user toggled enable but the process hasn't spawned
    /// yet (pending or just exited).
    Starting,
    /// GPK only — patched into the mapper, currently applied to the game.
    Enabled,
    /// Installed version < catalog version.
    UpdateAvailable,
    /// Last attempt to install / enable / spawn failed. `ModEntry.last_error`
    /// holds the message.
    Error,
    /// Mid-download or mid-install. `ModEntry.progress` holds the percentage.
    Installing,
}

/// Single mod record. The frontend renders one row per `ModEntry`.
///
/// `id` is the stable key. Catalog entries use the catalog id
/// (e.g. `tera-europe-classic.shinra`); user-imported local GPKs use a derived
/// id (`local.<hash>`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModEntry {
    pub id: String,
    pub kind: ModKind,
    pub name: String,
    pub author: String,
    pub description: String,
    pub version: String,
    pub status: ModStatus,

    /// Present when downloaded from the catalog; `None` for local imports.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_url: Option<String>,

    /// URL of the icon image, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon_url: Option<String>,

    /// Install-time progress percentage (0-100) for `ModStatus::Installing`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub progress: Option<u8>,

    /// Populated when `status == Error`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,

    /// External apps: auto-launch alongside TERA when enabled.
    pub auto_launch: bool,

    /// True when the user has enabled this mod (separate from `status` which
    /// tracks runtime). For external apps: auto-launch intent. For GPK: should
    /// be patched into the mapper.
    pub enabled: bool,

    /// Credit / attribution fields — shown in the mod detail panel so users
    /// can find the original source, license, and extra acknowledgments.
    /// `None` when upstream omits the field; the UI hides empty rows.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,

    /// Freeform credit string — e.g. "Originally by Foglio1024; fork by
    /// TERA-Europe-Classic. Icon artwork CC BY-NC by @someone". Rendered
    /// verbatim in the detail panel.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub credits: Option<String>,

    /// Long, multi-paragraph description for the detail panel. Short
    /// `description` is used in the list row; this is the full README-style
    /// body shown when the user drills in.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub long_description: Option<String>,

    /// Screenshots shown in the detail panel. URLs resolved against the
    /// catalog base.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub screenshots: Vec<String>,
}

impl ModEntry {
    /// Minimal builder for a fresh catalog entry before anything has happened.
    pub fn from_catalog(catalog: &CatalogEntry) -> Self {
        Self {
            id: catalog.id.clone(),
            kind: catalog.kind,
            name: catalog.name.clone(),
            author: catalog.author.clone(),
            description: catalog.short_description.clone(),
            version: catalog.version.clone(),
            status: ModStatus::NotInstalled,
            source_url: catalog.source_url.clone(),
            icon_url: catalog.icon_url.clone(),
            progress: None,
            last_error: None,
            auto_launch: catalog.auto_launch_default.unwrap_or(false),
            enabled: false,
            license: non_empty(&catalog.license),
            credits: non_empty(&catalog.credits),
            long_description: non_empty(&catalog.long_description),
            screenshots: catalog.screenshots.clone(),
        }
    }
}

/// Remote catalog entry. Fetched from the `TERA-Europe-Classic/mod-catalog`
/// GitHub repo and cached locally.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalogEntry {
    pub id: String,
    pub kind: ModKind,
    pub name: String,
    pub author: String,
    pub short_description: String,
    #[serde(default)]
    pub long_description: String,
    #[serde(default)]
    pub category: String,
    /// SPDX license identifier (MIT, GPL-3.0, etc.) or a freeform string
    /// when the project has no SPDX match. Shown in the credits section of
    /// the detail panel.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub license: String,
    /// Freeform acknowledgments — the launcher renders it verbatim below
    /// Author/License in the detail panel. Use this to credit original
    /// authors of forks, artwork sources, packet parser authors, etc.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub credits: String,
    pub version: String,
    pub download_url: String,
    pub sha256: String,
    #[serde(default)]
    pub size_bytes: u64,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon_url: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub screenshots: Vec<String>,

    // External-app-only fields
    #[serde(skip_serializing_if = "Option::is_none")]
    pub executable_relpath: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auto_launch_default: Option<bool>,
    /// OS-specific path template, e.g. `%APPDATA%\\ShinraMeter`. If present,
    /// the uninstall flow prompts the user whether to also delete it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub settings_folder: Option<String>,

    // GPK-only fields
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_patch: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub composite_flag: Option<bool>,

    #[serde(default)]
    pub updated_at: String,
}

/// Top-level catalog document shape.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Catalog {
    pub version: u32,
    pub updated_at: String,
    pub mods: Vec<CatalogEntry>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_entry_deserializes_minimal_shape() {
        let json = r#"{
            "id": "test.example",
            "kind": "external",
            "name": "Example",
            "author": "Someone",
            "short_description": "A test",
            "version": "1.0.0",
            "download_url": "https://example.com/x.zip",
            "sha256": "abcd"
        }"#;
        let entry: CatalogEntry = serde_json::from_str(json).unwrap();
        assert_eq!(entry.id, "test.example");
        assert!(entry.long_description.is_empty());
        assert!(entry.screenshots.is_empty());
        assert_eq!(entry.kind, ModKind::External);
    }

    #[test]
    fn mod_entry_from_catalog_copies_relevant_fields() {
        let catalog = CatalogEntry {
            id: "test.mod".into(),
            kind: ModKind::Gpk,
            name: "Test".into(),
            author: "Author".into(),
            short_description: "Short".into(),
            long_description: "Long body".into(),
            category: "ui".into(),
            license: "MIT".into(),
            credits: "Originally by Upstream".into(),
            version: "2.0".into(),
            download_url: "url".into(),
            sha256: "hash".into(),
            size_bytes: 100,
            source_url: Some("src".into()),
            icon_url: None,
            screenshots: vec![],
            executable_relpath: None,
            auto_launch_default: None,
            settings_folder: None,
            target_patch: Some("v100.02".into()),
            composite_flag: Some(true),
            updated_at: "2026-04-18".into(),
        };
        let entry = ModEntry::from_catalog(&catalog);
        assert_eq!(entry.id, "test.mod");
        assert_eq!(entry.status, ModStatus::NotInstalled);
        assert!(!entry.enabled);
        assert!(!entry.auto_launch);
    }

    #[test]
    fn mod_entry_from_catalog_defaults_auto_launch_from_catalog() {
        let catalog = CatalogEntry {
            id: "shinra".into(),
            kind: ModKind::External,
            name: "Shinra".into(),
            author: "neowutran".into(),
            short_description: "".into(),
            long_description: "".into(),
            category: "".into(),
            license: "".into(),
            credits: "".into(),
            version: "3.0".into(),
            download_url: "".into(),
            sha256: "".into(),
            size_bytes: 0,
            source_url: None,
            icon_url: None,
            screenshots: vec![],
            executable_relpath: Some("ShinraMeter.exe".into()),
            auto_launch_default: Some(true),
            settings_folder: None,
            target_patch: None,
            composite_flag: None,
            updated_at: "".into(),
        };
        let entry = ModEntry::from_catalog(&catalog);
        assert!(entry.auto_launch);
    }
}
