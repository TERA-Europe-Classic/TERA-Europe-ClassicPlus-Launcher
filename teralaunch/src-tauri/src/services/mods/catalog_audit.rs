//! Catalog GPK migration audit helpers for the launcher-maintained mod index.
//!
//! This module reads the external catalog plus cached package-header facts and
//! produces deterministic Markdown/JSON summaries. It must keep untrusted
//! catalog text escaped for tables, classify x32/x64/composite blockers without
//! touching live game files, and remain stable enough for review diffs to show
//! only real migration-state changes.

use std::collections::BTreeMap;
use std::path::Path;

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct AuditCatalog {
    pub version: u32,
    #[serde(default)]
    pub updated_at: String,
    pub mods: Vec<AuditCatalogEntry>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AuditCatalogEntry {
    pub id: String,
    pub kind: String,
    pub name: String,
    pub author: String,
    #[serde(default)]
    pub short_description: String,
    pub download_url: String,
    #[serde(default)]
    pub sha256: String,
    #[serde(default)]
    pub gpk_files: Vec<String>,
    #[serde(default)]
    pub compatible_arch: Option<String>,
    #[serde(default)]
    pub target_patch: Option<String>,
    #[serde(default)]
    pub composite_flag: Option<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum AuditArch {
    X64,
    X32,
    Unknown,
}

impl AuditArch {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::X64 => "x64",
            Self::X32 => "x32",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum MigrationStatus {
    BinaryDiffAuditRequired,
    StructuralManifestRequired,
    PublishX64RebuildRequired,
    NeedsTargetMetadata,
}

impl MigrationStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::BinaryDiffAuditRequired => "binary-diff-audit-required",
            Self::StructuralManifestRequired => "structural-manifest-required",
            Self::PublishX64RebuildRequired => "publish-x64-rebuild-required",
            Self::NeedsTargetMetadata => "needs-target-metadata",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditRow {
    pub id: String,
    pub name: String,
    pub author: String,
    pub arch: AuditArch,
    pub target_patch: String,
    pub package_hints: Vec<String>,
    pub migration_status: MigrationStatus,
    pub required_operations: Vec<String>,
    pub smoke_test: String,
    pub notes: Vec<String>,
    pub cached_header: Option<CachedPackageHeader>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CachedPackageHeader {
    pub file_version: u16,
    pub license_version: u16,
    pub arch: AuditArch,
}

pub fn audit_catalog(catalog: &AuditCatalog) -> Vec<AuditRow> {
    catalog.mods.iter().filter_map(audit_entry).collect()
}

pub fn audit_catalog_with_cached_headers(
    catalog: &AuditCatalog,
    cache_dir: &Path,
) -> Vec<AuditRow> {
    catalog
        .mods
        .iter()
        .filter_map(|entry| audit_entry_with_cached_header(entry, cache_dir))
        .collect()
}

fn audit_entry_with_cached_header(entry: &AuditCatalogEntry, cache_dir: &Path) -> Option<AuditRow> {
    if entry.kind != "gpk" {
        return None;
    }

    let cached_header = read_cached_header(entry, cache_dir);
    let inferred_arch = infer_arch(entry);
    let effective_arch = cached_header
        .as_ref()
        .map(|header| header.arch)
        .unwrap_or(inferred_arch);
    let mut row = build_audit_row(entry, effective_arch);
    if cached_header
        .as_ref()
        .is_some_and(|header| header.arch != inferred_arch)
    {
        row.notes.push(format!(
            "Cached artifact header {} overrides catalog-inferred {}; install/rebuild decisions use the artifact header.",
            effective_arch.as_str(),
            inferred_arch.as_str()
        ));
    }
    row.cached_header = cached_header;
    Some(row)
}

pub fn audit_entry(entry: &AuditCatalogEntry) -> Option<AuditRow> {
    if entry.kind != "gpk" {
        return None;
    }

    Some(build_audit_row(entry, infer_arch(entry)))
}

fn build_audit_row(entry: &AuditCatalogEntry, arch: AuditArch) -> AuditRow {
    let target_patch = entry.target_patch.clone().unwrap_or_default();
    let package_hints = package_hints(entry);
    let blocker = known_structural_blocker(entry.id.as_str());
    let mut notes = Vec::new();

    if entry.gpk_files.is_empty() {
        notes.push("Catalog entry is missing gpk_files; using download filename as the package hint until the catalog is enriched.".to_string());
    }
    if entry.composite_flag != Some(true) {
        notes.push("Catalog entry is not explicitly marked composite; verify loader path before publishing a patch artifact.".to_string());
    }

    let migration_status = match arch {
        AuditArch::X32 => {
            notes.push("Old x32 GPK bytes cannot be loaded directly by the v100.02 x64 client; extract intent/assets and publish a rebuilt x64 artifact.".to_string());
            MigrationStatus::PublishX64RebuildRequired
        }
        AuditArch::X64 | AuditArch::Unknown => {
            if let Some(blocker) = &blocker {
                notes.push(blocker.note.to_string());
                MigrationStatus::StructuralManifestRequired
            } else if package_hints.is_empty() {
                MigrationStatus::NeedsTargetMetadata
            } else {
                MigrationStatus::BinaryDiffAuditRequired
            }
        }
    };

    AuditRow {
        id: entry.id.clone(),
        name: entry.name.clone(),
        author: entry.author.clone(),
        arch,
        target_patch,
        package_hints,
        migration_status,
        required_operations: blocker
            .map(|b| {
                b.required_operations
                    .iter()
                    .map(|op| (*op).to_string())
                    .collect()
            })
            .unwrap_or_default(),
        smoke_test: smoke_test_for(entry),
        notes,
        cached_header: None,
    }
}

pub fn render_markdown_report(catalog: &AuditCatalog, rows: &[AuditRow]) -> String {
    let mut by_status = BTreeMap::new();
    let mut by_arch = BTreeMap::new();
    for row in rows {
        *by_status.entry(row.migration_status).or_insert(0usize) += 1;
        *by_arch.entry(row.arch).or_insert(0usize) += 1;
    }

    let mut out = String::new();
    out.push_str("# GPK Catalog Audit\n\n");
    out.push_str(&format!("- Catalog version: {}\n", catalog.version));
    out.push_str(&format!(
        "- Catalog updated_at: {}\n",
        escape_cell(&catalog.updated_at)
    ));
    out.push_str(&format!("- GPK rows audited: {}\n\n", rows.len()));

    out.push_str("## Summary by status\n\n");
    for (status, count) in by_status {
        out.push_str(&format!("- `{}`: {}\n", status.as_str(), count));
    }
    out.push_str("\n## Summary by arch\n\n");
    for (arch, count) in by_arch {
        out.push_str(&format!("- `{}`: {}\n", arch.as_str(), count));
    }

    out.push_str("\n## Audit rows\n\n");
    out.push_str(
        "| Status | Arch | Mod | Packages | Header | Required operations | Smoke test | Notes |\n",
    );
    out.push_str("|---|---|---|---|---|---|---|---|\n");

    let mut sorted = rows.to_vec();
    sorted.sort_by(|a, b| {
        a.migration_status
            .cmp(&b.migration_status)
            .then_with(|| a.id.cmp(&b.id))
    });
    for row in sorted {
        out.push_str(&format!(
            "| `{}` | `{}` | `{}`<br>{} | {} | {} | {} | {} | {} |\n",
            row.migration_status.as_str(),
            row.arch.as_str(),
            escape_cell(&row.id),
            escape_cell(&row.name),
            escape_cell(&join_or_dash(&row.package_hints)),
            escape_cell(&format_cached_header(row.cached_header.as_ref())),
            escape_cell(&join_or_dash(&row.required_operations)),
            escape_cell(&row.smoke_test),
            escape_cell(&join_or_dash(&row.notes)),
        ));
    }

    out
}

fn format_cached_header(header: Option<&CachedPackageHeader>) -> String {
    match header {
        Some(header) => format!(
            "FileVersion {}<br>LicenseVersion {}<br>header-arch {}",
            header.file_version,
            header.license_version,
            header.arch.as_str()
        ),
        None => "-".to_string(),
    }
}

fn join_or_dash(values: &[String]) -> String {
    if values.is_empty() {
        "-".to_string()
    } else {
        values.join("<br>")
    }
}

fn escape_cell(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('`', "&#96;")
        .replace('|', "\\|")
        .replace('\r', " ")
        .replace('\n', "<br>")
}

fn infer_arch(entry: &AuditCatalogEntry) -> AuditArch {
    if let Some(arch) = entry.compatible_arch.as_deref() {
        return match arch.trim().to_ascii_lowercase().as_str() {
            "x32" | "32" | "classic" => AuditArch::X32,
            "x64" | "64" | "modern" => AuditArch::X64,
            _ => AuditArch::Unknown,
        };
    }

    match entry.target_patch.as_deref().unwrap_or_default() {
        patch if patch.contains("v32") => AuditArch::X32,
        patch if patch.contains("v100") => AuditArch::X64,
        _ => AuditArch::Unknown,
    }
}

fn package_hints(entry: &AuditCatalogEntry) -> Vec<String> {
    if !entry.gpk_files.is_empty() {
        return entry.gpk_files.clone();
    }

    let trimmed = entry.download_url.split('?').next().unwrap_or_default();
    let filename = trimmed.rsplit('/').next().unwrap_or_default();
    if filename.to_ascii_lowercase().ends_with(".gpk") {
        vec![filename.replace("%20", " ")]
    } else {
        Vec::new()
    }
}

fn read_cached_header(entry: &AuditCatalogEntry, cache_dir: &Path) -> Option<CachedPackageHeader> {
    let sha256 = validated_sha256(entry)?;
    let path = cache_dir.join(format!("{sha256}.gpk"));
    let bytes = std::fs::read(path).ok()?;
    read_package_header(&bytes)
}

pub fn validated_sha256(entry: &AuditCatalogEntry) -> Option<String> {
    let sha256 = entry.sha256.trim().to_ascii_lowercase();
    if sha256.is_empty() {
        return None;
    }
    let is_valid = sha256.len() == 64 && sha256.bytes().all(|byte| byte.is_ascii_hexdigit());
    if is_valid {
        Some(sha256)
    } else {
        None
    }
}

fn read_package_header(bytes: &[u8]) -> Option<CachedPackageHeader> {
    if bytes.len() < 8 {
        return None;
    }
    let magic = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
    if magic != 0x9E2A83C1 {
        return None;
    }
    let file_version = u16::from_le_bytes([bytes[4], bytes[5]]);
    let license_version = u16::from_le_bytes([bytes[6], bytes[7]]);
    let arch = if file_version >= 0x381 {
        AuditArch::X64
    } else {
        AuditArch::X32
    };

    Some(CachedPackageHeader {
        file_version,
        license_version,
        arch,
    })
}

#[derive(Debug, Clone, Copy)]
struct KnownStructuralBlocker {
    required_operations: &'static [&'static str],
    note: &'static str,
}

fn known_structural_blocker(id: &str) -> Option<KnownStructuralBlocker> {
    match id {
        "foglio1024.ui-remover-flight-gauge" => Some(KnownStructuralBlocker {
            required_operations: &[
                "14x ObjectRedirector -> Texture2D class+payload replacements",
                "remove imports that only served the old redirector graph",
                "rebuild S1UI_ProgressBar as a v100.02 x64 composite-safe artifact",
            ],
            note: "Flight Gauge is a known structural blocker: payload-only patching leaves redirectors/imports wrong and may no-op or crash.",
        }),
        "foglio1024.ui-remover-bosswindow" => Some(KnownStructuralBlocker {
            required_operations: &[
                "replace boss gauge payload",
                "remove GageBoss_I1C redirector export",
            ],
            note: "Boss Window is a known structural blocker until export removal is validated against v100.02 vanilla bytes.",
        }),
        "saltymonkey.message-clean" | "foglio1024.restyle-community-window" => {
            Some(KnownStructuralBlocker {
                required_operations: &[
                    "object graph diff review",
                    "import table rewrite",
                    "name/export table validation",
                ],
                note: "This mod family has documented import/export table drift; publish only after structural manifest review.",
            })
        }
        _ => None,
    }
}

fn smoke_test_for(entry: &AuditCatalogEntry) -> String {
    let id = entry.id.as_str();
    let text = format!("{} {}", entry.name, entry.short_description).to_ascii_lowercase();

    if id == "foglio1024.ui-remover-flight-gauge" || text.contains("flight") {
        return "Mount a flying mount, spend/restore flight stamina, and verify the flight energy bar stays hidden without a client crash.".to_string();
    }
    if text.contains("boss") || text.contains("gageboss") {
        return "Enter a boss encounter or training scenario that shows the boss gauge; verify the advertised boss-gauge change and no client crash.".to_string();
    }
    if text.contains("chat") {
        return "Open chat, send/receive a message, and verify the advertised chat layout/style change without broken input or missing tabs.".to_string();
    }
    if text.contains("costume") || text.contains("mount") || text.contains("pet") {
        return "Preview or equip the affected model in game; verify the visual change, animations, and textures render without missing materials.".to_string();
    }

    "Install, launch the game, navigate to the affected UI/model/effect, and verify the catalog description is visibly true with no crash.".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gpk_entry(id: &str, target_patch: &str, download_url: &str) -> AuditCatalogEntry {
        AuditCatalogEntry {
            id: id.into(),
            kind: "gpk".into(),
            name: id.into(),
            author: "Tester".into(),
            short_description: "Test".into(),
            download_url: download_url.into(),
            sha256: String::new(),
            gpk_files: vec![],
            compatible_arch: None,
            target_patch: Some(target_patch.into()),
            composite_flag: Some(true),
        }
    }

    fn gpk_entry_with_sha(id: &str, target_patch: &str, sha256: &str) -> AuditCatalogEntry {
        AuditCatalogEntry {
            sha256: sha256.into(),
            ..gpk_entry(id, target_patch, "https://example.com/S1UI_Test.gpk")
        }
    }

    #[test]
    fn marks_v32_mods_as_publish_x64_rebuild_required() {
        let entry = gpk_entry(
            "pantypon.pink-chat-window",
            "v32.04",
            "https://example.com/S1UI_Chat2.gpk",
        );
        let row = audit_entry(&entry).expect("gpk row");

        assert_eq!(row.arch, AuditArch::X32);
        assert_eq!(
            row.migration_status,
            MigrationStatus::PublishX64RebuildRequired
        );
        assert!(row
            .notes
            .iter()
            .any(|note| note.contains("cannot be loaded directly")));
    }

    #[test]
    fn recognizes_flight_gauge_structural_blocker() {
        let entry = gpk_entry(
            "foglio1024.ui-remover-flight-gauge",
            "v100.02",
            "https://raw.githubusercontent.com/foglio1024/UI-Remover/master/remove_FlightGauge/S1UI_ProgressBar.gpk",
        );
        let row = audit_entry(&entry).expect("gpk row");

        assert_eq!(row.package_hints, vec!["S1UI_ProgressBar.gpk"]);
        assert_eq!(
            row.migration_status,
            MigrationStatus::StructuralManifestRequired
        );
        assert!(row
            .required_operations
            .iter()
            .any(|op| op.contains("ObjectRedirector -> Texture2D")));
    }

    #[test]
    fn escapes_untrusted_markdown_table_cells() {
        let catalog = AuditCatalog {
            version: 1,
            updated_at: "2026-04-30T00:00:00Z".into(),
            mods: vec![AuditCatalogEntry {
                id: "unsafe|id".into(),
                kind: "gpk".into(),
                name: "<script>alert(`x`)</script>".into(),
                author: "Tester".into(),
                short_description: "Test".into(),
                download_url: "https://example.com/S1UI_Test.gpk".into(),
                sha256: String::new(),
                gpk_files: vec!["S1UI_Test.gpk".into()],
                compatible_arch: Some("x64".into()),
                target_patch: Some("v100.02".into()),
                composite_flag: Some(true),
            }],
        };

        let report = render_markdown_report(&catalog, &audit_catalog(&catalog));

        assert!(report.contains("unsafe\\|id"));
        assert!(report.contains("&lt;script&gt;alert(&#96;x&#96;)&lt;/script&gt;"));
        assert!(!report.contains("<script>"));
    }

    #[test]
    fn attaches_cached_package_header_facts_by_sha256() {
        let cache = tempfile::tempdir().expect("cache dir");
        let sha256 = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        let cached_gpk = cache.path().join(format!("{sha256}.gpk"));
        std::fs::write(
            &cached_gpk,
            [
                0xC1, 0x83, 0x2A, 0x9E, // TERA package magic
                0x81, 0x03, // FileVersion 897 / x64
                0x00, 0x00, // LicenseVersion 0
            ],
        )
        .expect("write cached gpk");
        let catalog = AuditCatalog {
            version: 1,
            updated_at: String::new(),
            mods: vec![gpk_entry_with_sha(
                "tester.cached-header",
                "v100.02",
                sha256,
            )],
        };

        let rows = audit_catalog_with_cached_headers(&catalog, cache.path());

        assert_eq!(rows.len(), 1);
        assert_eq!(
            rows[0].cached_header.as_ref().map(|h| h.file_version),
            Some(897)
        );
        assert_eq!(
            rows[0].cached_header.as_ref().map(|h| h.license_version),
            Some(0)
        );
        assert_eq!(
            rows[0].cached_header.as_ref().map(|h| h.arch),
            Some(AuditArch::X64)
        );
    }

    #[test]
    fn cached_header_arch_overrides_catalog_patch_arch() {
        let cache = tempfile::tempdir().expect("cache dir");
        let legacy_sha = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
        let modern_sha = "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";
        std::fs::write(
            cache.path().join(format!("{legacy_sha}.gpk")),
            [
                0xC1, 0x83, 0x2A, 0x9E, // TERA package magic
                0x62, 0x02, // FileVersion 610 / x32
                0x0E, 0x00, // LicenseVersion 14
            ],
        )
        .expect("write legacy cached gpk");
        std::fs::write(
            cache.path().join(format!("{modern_sha}.gpk")),
            [
                0xC1, 0x83, 0x2A, 0x9E, // TERA package magic
                0x81, 0x03, // FileVersion 897 / x64
                0x11, 0x00, // LicenseVersion 17
            ],
        )
        .expect("write modern cached gpk");
        let catalog = AuditCatalog {
            version: 1,
            updated_at: String::new(),
            mods: vec![
                gpk_entry_with_sha("tester.catalog-modern-header-legacy", "v100.02", legacy_sha),
                gpk_entry_with_sha("tester.catalog-legacy-header-modern", "v32.04", modern_sha),
            ],
        };

        let rows = audit_catalog_with_cached_headers(&catalog, cache.path());
        let legacy_header = rows
            .iter()
            .find(|row| row.id == "tester.catalog-modern-header-legacy")
            .expect("legacy header row");
        let modern_header = rows
            .iter()
            .find(|row| row.id == "tester.catalog-legacy-header-modern")
            .expect("modern header row");

        assert_eq!(legacy_header.arch, AuditArch::X32);
        assert_eq!(
            legacy_header.migration_status,
            MigrationStatus::PublishX64RebuildRequired
        );
        assert_eq!(modern_header.arch, AuditArch::X64);
        assert_eq!(
            modern_header.migration_status,
            MigrationStatus::BinaryDiffAuditRequired
        );
        assert!(legacy_header
            .notes
            .iter()
            .any(|note| note.contains("artifact header x32")));
        assert!(modern_header
            .notes
            .iter()
            .any(|note| note.contains("artifact header x64")));
    }
}
