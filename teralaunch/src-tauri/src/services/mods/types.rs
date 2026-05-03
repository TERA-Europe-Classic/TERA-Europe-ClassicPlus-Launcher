//! Shared types for the mod manager.
//!
//! These types cross the Tauri boundary — every `Serialize` variant maps to a
//! discriminator the frontend reads to render the right row treatment.

use serde::{Deserialize, Serialize};

/// How the launcher deploys a GPK mod into the game.
///
/// The default (`CompositePatch`) uses the existing composite-mapper splice
/// path. `Dropin` writes the file directly to `S1Game/CookedPC/<filename>`
/// without touching any mapper — used for Type-D mods whose target package
/// isn't in v100 vanilla's PkgMapper.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DeployStrategy {
    /// Default: composite mapper splice (existing path).
    CompositePatch,
    /// Drop-in CookedPC file (Type-D mods whose target package isn't in vanilla).
    Dropin,
    /// Targets a v100 vanilla composite slice. Deploys under a filename derived
    /// from `target_object_path`'s tail (`<tail>_dup.gpk`), then rewrites the
    /// matching `CompositePackageMapper.dat` entry to redirect to our file.
    CompositeRedirect,
}

fn non_empty(s: &str) -> Option<String> {
    if s.is_empty() {
        None
    } else {
        Some(s.to_string())
    }
}

/// Which kind of mod this is. The frontend groups rows by this and renders a
/// different primary-action state machine for each.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModKind {
    /// Separate process (Shinra Meter, TCC). Lifecycle = download, extract,
    /// spawn, monitor. Does not touch game files.
    External,
    /// `.gpk` pack that patches `CompositePackageMapper.dat`.
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

    /// The filename currently deployed into `S1Game/CookedPC`, when known.
    /// Used by uninstall to remove legacy filename-based overrides whose
    /// embedded metadata does not carry a stable container name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deployed_filename: Option<String>,

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

    /// One-line punchy hook (≤90 chars). Row cards display this; falls
    /// back to short_description when missing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tagline: Option<String>,

    /// Hero image at the top of the detail panel. 16:9 preferred, ≥1200w.
    /// For restyles, this is the "after" shot.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub featured_image: Option<String>,

    /// Restyles only — paired "before" shot for side-by-side compare.
    /// Side-by-side panel only renders when both before_image and
    /// featured_image are present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub before_image: Option<String>,

    /// Searchable badges. e.g. ["minimap","quality-of-life","foglio"].
    /// Distinct from `category` (single-string filter).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,

    /// GPK files this mod replaces, e.g. ["S1UI_Chat2.gpk"]. Power-user
    /// info shown in Details row.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub gpk_files: Vec<String>,

    /// Markdown. "Conflicts with X", "Broken on patch Y". Rendered in a
    /// yellow-tinted callout above the screenshot strip.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compatibility_notes: Option<String>,

    /// Last patch the mod was confirmed working on, e.g. "patch 113".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_verified_patch: Option<String>,

    /// Stub for future telemetry. UI does NOT render this yet.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub download_count: Option<u64>,

    /// Long, multi-paragraph description for the detail panel. Short
    /// `description` is used in the list row; this is the full README-style
    /// body shown when the user drills in.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub long_description: Option<String>,

    /// Screenshots shown in the detail panel. URLs resolved against the
    /// catalog base.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub screenshots: Vec<String>,

    /// `Some("x32")` (FileVersion 610, old Classic) or `Some("x64")` (897,
    /// v100.02) when the catalog has confirmed the binary arch of the GPK,
    /// `None` when unknown. The Browse-tab UI surfaces an "incompatible"
    /// badge when this disagrees with the client's arch (Classic+ is x64).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compatible_arch: Option<String>,

    /// How the launcher deployed this GPK. Copied from `CatalogEntry` at
    /// install time so uninstall can take the right teardown path.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deploy_strategy: Option<DeployStrategy>,

    /// The target filename used during a `Dropin` install. Copied from
    /// `CatalogEntry` so uninstall can find and remove the file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_dropin_filename: Option<String>,
}

impl ModEntry {
    /// PRD 3.3.4.add-mod-from-file-wire: builder for a user-imported local
    /// GPK, where we have no catalog entry. `bytes_sha256` is the hex digest
    /// of the GPK bytes (lowercase) and `modfile` is the parsed metadata footer.
    /// id format: `local.<sha12>` — stable across reinstalls of the same bytes.
    ///
    /// Caller is responsible for deploying the GPK and upserting into the
    /// registry; this is just the entry shape.
    pub fn from_local_gpk(
        bytes_sha256: &str,
        modfile: &crate::services::mods::gpk::ModFile,
        fallback_display_name: Option<&str>,
    ) -> Self {
        let sha12 = bytes_sha256.get(..12).unwrap_or("unknown0000");
        let id = format!("local.{sha12}");
        let name = if modfile.mod_name.trim().is_empty() {
            if modfile.container.trim().is_empty() {
                fallback_display_name
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .unwrap_or("Local GPK")
                    .to_string()
            } else {
                modfile.container.trim().to_string()
            }
        } else {
            modfile.mod_name.trim().to_string()
        };
        let author = if modfile.mod_author.trim().is_empty() {
            "Unknown".to_string()
        } else {
            modfile.mod_author.trim().to_string()
        };
        Self {
            id,
            kind: ModKind::Gpk,
            name,
            author,
            description: "User-imported GPK".to_string(),
            version: "local".to_string(),
            status: ModStatus::NotInstalled,
            source_url: None,
            deployed_filename: None,
            icon_url: None,
            progress: None,
            last_error: None,
            auto_launch: false,
            enabled: false,
            license: None,
            credits: None,
            tagline: None,
            featured_image: None,
            before_image: None,
            tags: Vec::new(),
            gpk_files: Vec::new(),
            compatibility_notes: None,
            last_verified_patch: None,
            download_count: None,
            long_description: None,
            screenshots: Vec::new(),
            compatible_arch: None,
            deploy_strategy: None,
            target_dropin_filename: None,
        }
    }

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
            deployed_filename: None,
            icon_url: catalog.icon_url.clone(),
            progress: None,
            last_error: None,
            auto_launch: catalog.auto_launch_default.unwrap_or(false),
            enabled: false,
            license: non_empty(&catalog.license),
            credits: non_empty(&catalog.credits),
            tagline: catalog.tagline.clone(),
            featured_image: catalog.featured_image.clone(),
            before_image: catalog.before_image.clone(),
            tags: catalog.tags.clone(),
            gpk_files: catalog.gpk_files.clone(),
            compatibility_notes: catalog.compatibility_notes.clone(),
            last_verified_patch: catalog.last_verified_patch.clone(),
            download_count: catalog.download_count,
            long_description: non_empty(&catalog.long_description),
            screenshots: catalog.screenshots.clone(),
            compatible_arch: catalog.compatible_arch.clone(),
            deploy_strategy: catalog.deploy_strategy,
            target_dropin_filename: catalog.target_dropin_filename.clone(),
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

    /// One-line punchy hook (≤90 chars). Row cards display this; falls
    /// back to short_description when missing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tagline: Option<String>,

    /// Hero image at the top of the detail panel. 16:9 preferred, ≥1200w.
    /// For restyles, this is the "after" shot.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub featured_image: Option<String>,

    /// Restyles only — paired "before" shot for side-by-side compare.
    /// Side-by-side panel only renders when both before_image and
    /// featured_image are present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub before_image: Option<String>,

    /// Searchable badges. e.g. ["minimap","quality-of-life","foglio"].
    /// Distinct from `category` (single-string filter).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,

    /// GPK files this mod replaces, e.g. ["S1UI_Chat2.gpk"]. Power-user
    /// info shown in Details row.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub gpk_files: Vec<String>,

    /// Markdown. "Conflicts with X", "Broken on patch Y". Rendered in a
    /// yellow-tinted callout above the screenshot strip.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compatibility_notes: Option<String>,

    /// Last patch the mod was confirmed working on, e.g. "patch 113".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_verified_patch: Option<String>,

    /// Stub for future telemetry. UI does NOT render this yet.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub download_count: Option<u64>,

    /// `Some("x32")` (FileVersion 610) or `Some("x64")` (897); `None` when
    /// the catalog hasn't confirmed it. Lets the launcher refuse / warn on
    /// arch mismatch before download.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compatible_arch: Option<String>,

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

    /// How the launcher should deploy this GPK. `None` means `CompositePatch`
    /// (the default path). Set to `dropin` for Type-D mods whose target
    /// package isn't in v100 vanilla's PkgMapper.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deploy_strategy: Option<DeployStrategy>,

    /// Target filename for `deploy_strategy=dropin`. Written as-is into
    /// `S1Game/CookedPC/<target_dropin_filename>`. Required when
    /// `deploy_strategy=dropin`; unused otherwise.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_dropin_filename: Option<String>,

    /// Full logical path (`Package.Object`) of the specific composite slice
    /// this mod replaces. Required for mods targeting multi-object widget
    /// packages — without this qualifier `vanilla_resolver` errors with
    /// "maps to multiple vanilla composite byte ranges". Catalog entries
    /// for single-object packages may leave this `None`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_object_path: Option<String>,

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
    fn catalog_entry_deserializes_full_enriched_shape() {
        let json = r#"{
            "id": "test.full",
            "kind": "gpk",
            "name": "Full Mod",
            "author": "Tester",
            "short_description": "Test",
            "version": "1.0.0",
            "download_url": "https://example.com/x.gpk",
            "sha256": "abcd",
            "tagline": "Punchy hook",
            "featured_image": "https://example.com/hero.png",
            "before_image": "https://example.com/before.png",
            "tags": ["minimap","quality-of-life"],
            "gpk_files": ["S1UI_Map.gpk"],
            "compatibility_notes": "Conflicts with X",
            "last_verified_patch": "patch 113",
            "download_count": 42
        }"#;
        let entry: CatalogEntry = serde_json::from_str(json).unwrap();
        assert_eq!(entry.tagline.as_deref(), Some("Punchy hook"));
        assert_eq!(
            entry.featured_image.as_deref(),
            Some("https://example.com/hero.png")
        );
        assert_eq!(
            entry.before_image.as_deref(),
            Some("https://example.com/before.png")
        );
        assert_eq!(entry.tags, vec!["minimap", "quality-of-life"]);
        assert_eq!(entry.gpk_files, vec!["S1UI_Map.gpk"]);
        assert_eq!(
            entry.compatibility_notes.as_deref(),
            Some("Conflicts with X")
        );
        assert_eq!(entry.last_verified_patch.as_deref(), Some("patch 113"));
        assert_eq!(entry.download_count, Some(42));
    }

    #[test]
    fn catalog_entry_minimal_shape_keeps_new_fields_default() {
        let json = r#"{
            "id": "test.min",
            "kind": "external",
            "name": "Minimal",
            "author": "Tester",
            "short_description": "Test",
            "version": "1.0.0",
            "download_url": "https://example.com/x.zip",
            "sha256": "abcd"
        }"#;
        let entry: CatalogEntry = serde_json::from_str(json).unwrap();
        assert!(entry.tagline.is_none());
        assert!(entry.featured_image.is_none());
        assert!(entry.before_image.is_none());
        assert!(entry.tags.is_empty());
        assert!(entry.gpk_files.is_empty());
        assert!(entry.compatibility_notes.is_none());
        assert!(entry.last_verified_patch.is_none());
        assert!(entry.download_count.is_none());
    }

    #[test]
    fn mod_entry_from_catalog_copies_new_fields() {
        let catalog = CatalogEntry {
            id: "x".into(),
            kind: ModKind::Gpk,
            name: "X".into(),
            author: "A".into(),
            tagline: Some("Hook".into()),
            featured_image: Some("hero".into()),
            before_image: Some("before".into()),
            tags: vec!["t1".into(), "t2".into()],
            gpk_files: vec!["A.gpk".into()],
            compatibility_notes: Some("note".into()),
            last_verified_patch: Some("patch 113".into()),
            download_count: Some(100),
            short_description: "s".into(),
            long_description: "".into(),
            category: "".into(),
            license: "".into(),
            credits: "".into(),
            version: "1".into(),
            download_url: "".into(),
            sha256: "".into(),
            size_bytes: 0,
            source_url: None,
            icon_url: None,
            screenshots: vec![],
            executable_relpath: None,
            auto_launch_default: None,
            settings_folder: None,
            target_patch: None,
            composite_flag: None,
            target_object_path: None,
            compatible_arch: None,
            deploy_strategy: None,
            target_dropin_filename: None,
            updated_at: "".into(),
        };
        let entry = ModEntry::from_catalog(&catalog);
        assert_eq!(entry.tagline.as_deref(), Some("Hook"));
        assert_eq!(entry.featured_image.as_deref(), Some("hero"));
        assert_eq!(entry.before_image.as_deref(), Some("before"));
        assert_eq!(entry.tags, vec!["t1", "t2"]);
        assert_eq!(entry.gpk_files, vec!["A.gpk"]);
        assert_eq!(entry.compatibility_notes.as_deref(), Some("note"));
        assert_eq!(entry.last_verified_patch.as_deref(), Some("patch 113"));
        assert_eq!(entry.download_count, Some(100));
    }

    #[test]
    fn catalog_entry_round_trips_compatible_arch() {
        let json = r#"{
            "id": "test.x32",
            "kind": "gpk",
            "name": "x32 mod",
            "author": "old-author",
            "short_description": "Old Classic GPK",
            "version": "1.0.0",
            "download_url": "https://example.com/x.gpk",
            "sha256": "abcd",
            "compatible_arch": "x32"
        }"#;
        let entry: CatalogEntry = serde_json::from_str(json).unwrap();
        assert_eq!(entry.compatible_arch.as_deref(), Some("x32"));
        let mod_entry = ModEntry::from_catalog(&entry);
        assert_eq!(mod_entry.compatible_arch.as_deref(), Some("x32"));
    }

    #[test]
    fn mod_entry_from_catalog_copies_relevant_fields() {
        let catalog = CatalogEntry {
            id: "test.mod".into(),
            kind: ModKind::Gpk,
            name: "Test".into(),
            author: "Author".into(),
            tagline: None,
            featured_image: None,
            before_image: None,
            tags: vec![],
            gpk_files: vec![],
            compatibility_notes: None,
            last_verified_patch: None,
            download_count: None,
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
            target_object_path: None,
            compatible_arch: Some("x64".into()),
            deploy_strategy: None,
            target_dropin_filename: None,
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
            tagline: None,
            featured_image: None,
            before_image: None,
            tags: vec![],
            gpk_files: vec![],
            compatibility_notes: None,
            last_verified_patch: None,
            download_count: None,
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
            target_object_path: None,
            compatible_arch: None,
            deploy_strategy: None,
            target_dropin_filename: None,
            updated_at: "".into(),
        };
        let entry = ModEntry::from_catalog(&catalog);
        assert!(entry.auto_launch);
    }

    // --- PRD 3.3.4.add-mod-from-file-wire ----------------------------------

    use crate::services::mods::gpk::ModFile;

    fn modfile(name: &str, author: &str, container: &str) -> ModFile {
        ModFile {
            mod_name: name.into(),
            mod_author: author.into(),
            container: container.into(),
            ..Default::default()
        }
    }

    #[test]
    fn from_local_gpk_id_uses_sha_prefix() {
        let mf = modfile("Tiny Icons", "someone", "S1Data_icons.gpk");
        let sha = "abcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890";
        let entry = ModEntry::from_local_gpk(sha, &mf, None);
        assert_eq!(entry.id, "local.abcdef123456");
        assert_eq!(entry.kind, ModKind::Gpk);
        assert_eq!(entry.name, "Tiny Icons");
        assert_eq!(entry.author, "someone");
        assert_eq!(entry.version, "local");
        assert_eq!(entry.status, ModStatus::NotInstalled);
    }

    #[test]
    fn from_local_gpk_empty_name_falls_back_to_container() {
        let mf = modfile("", "", "S1Data_nameless.gpk");
        let sha = "000000000000000000000000000000000000000000000000000000000000abcd";
        let entry = ModEntry::from_local_gpk(sha, &mf, None);
        assert_eq!(entry.name, "S1Data_nameless.gpk");
        assert_eq!(entry.author, "Unknown");
    }

    #[test]
    fn from_local_gpk_empty_name_and_container_falls_back_to_generic() {
        let mf = modfile("", "", "");
        let sha = "deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef";
        let entry = ModEntry::from_local_gpk(sha, &mf, None);
        assert_eq!(entry.name, "Local GPK");
    }

    #[test]
    fn from_local_gpk_is_deterministic() {
        // Same bytes -> same id every time. Re-import is idempotent via
        // registry upsert.
        let mf = modfile("X", "Y", "Z.gpk");
        let sha = "1111111111111111111111111111111111111111111111111111111111111111";
        let a = ModEntry::from_local_gpk(sha, &mf, None);
        let b = ModEntry::from_local_gpk(sha, &mf, None);
        assert_eq!(a.id, b.id);
    }

    #[test]
    fn from_local_gpk_trims_whitespace_from_name_and_author() {
        let mf = modfile("  Spaced  ", "  Author  ", "S1.gpk");
        let sha = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        let entry = ModEntry::from_local_gpk(sha, &mf, None);
        assert_eq!(entry.name, "Spaced");
        assert_eq!(entry.author, "Author");
    }

    #[test]
    fn from_local_gpk_empty_container_can_fall_back_to_filename_stem() {
        let mf = modfile("", "", "");
        let sha = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
        let entry = ModEntry::from_local_gpk(sha, &mf, Some("S1UI_ProgressBar"));
        assert_eq!(entry.name, "S1UI_ProgressBar");
        assert_eq!(entry.author, "Unknown");
    }
}
