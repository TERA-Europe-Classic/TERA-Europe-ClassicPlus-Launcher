//! Remote mod catalog fetch + local cache.
//!
//! Catalog is a single JSON file hosted at
//! `https://raw.githubusercontent.com/TERA-Europe-Classic/mod-catalog/main/catalog.json`.
//! We cache the last successful fetch at `<app_data>/mods/catalog-cache.json`
//! with a timestamp; UI reads from the cache and refreshes in the background.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use super::types::{Catalog, CatalogEntry};

pub const CATALOG_URL: &str =
    "https://raw.githubusercontent.com/TERA-Europe-Classic/external-mod-catalog/main/catalog.json";

/// How long a cached catalog stays fresh before we background-refresh.
pub const CACHE_TTL_SECS: u64 = 24 * 60 * 60;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedCatalog {
    pub fetched_at_unix: u64,
    pub catalog: Catalog,
}

impl CachedCatalog {
    pub fn is_stale(&self, now_unix: u64) -> bool {
        now_unix.saturating_sub(self.fetched_at_unix) > CACHE_TTL_SECS
    }
}

pub fn get_cache_path() -> Option<PathBuf> {
    dirs_next::config_dir().map(|d| {
        d.join("Crazy-eSports-ClassicPlus")
            .join("mods")
            .join("catalog-cache.json")
    })
}

pub fn load_cache(path: &Path) -> Option<CachedCatalog> {
    let body = fs::read_to_string(path).ok()?;
    serde_json::from_str(&body).ok()
}

pub fn save_cache(path: &Path, cached: &CachedCatalog) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create catalog cache dir: {}", e))?;
    }
    let body = serde_json::to_string_pretty(cached)
        .map_err(|e| format!("Failed to serialize catalog cache: {}", e))?;
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, body).map_err(|e| format!("Failed to write catalog cache: {}", e))?;
    fs::rename(&tmp, path).map_err(|e| format!("Failed to commit catalog cache: {}", e))
}

/// Fetch the catalog from the remote URL. Reuses the launcher's existing
/// `reqwest`-based HTTP stack. Entries that fail to deserialise are
/// dropped with a WARN log; the rest of the catalog still surfaces so a
/// single bad entry can't brick the mods page — see `parse_catalog_tolerant`.
pub async fn fetch_remote(url: &str) -> Result<Catalog, String> {
    let client = reqwest::Client::builder()
        .user_agent("TERA-Europe-ClassicPlus-Launcher")
        .build()
        .map_err(|e| format!("Failed to build HTTP client: {}", e))?;

    let response = client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("Failed to fetch catalog: {}", e))?;

    if !response.status().is_success() {
        return Err(format!("Catalog fetch returned HTTP {}", response.status()));
    }

    let body = response
        .text()
        .await
        .map_err(|e| format!("Failed to read catalog body: {}", e))?;

    parse_catalog_tolerant(&body)
}

/// Parses the catalog JSON document, filtering out entries that fail to
/// deserialise as `CatalogEntry` while keeping the rest. The top-level
/// envelope (`version`, `updated_at`, `mods` array) is still required —
/// a malformed envelope is a hard error. Only individual `mods[]` entries
/// are skippable.
///
/// PRD 3.2.6.parse-error-filter: a single bad catalog entry must not
/// brick the entire mods page. Entry-level errors are logged at WARN so
/// catalog authors have something to grep.
pub fn parse_catalog_tolerant(body: &str) -> Result<Catalog, String> {
    let envelope: serde_json::Value = serde_json::from_str(body)
        .map_err(|e| format!("Catalog JSON envelope is malformed: {}", e))?;

    let version = envelope
        .get("version")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| "Catalog JSON missing 'version' (number)".to_string())?
        as u32;

    let updated_at = envelope
        .get("updated_at")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let mods_array = envelope
        .get("mods")
        .and_then(|v| v.as_array())
        .ok_or_else(|| "Catalog JSON missing 'mods' (array)".to_string())?;

    let mut mods = Vec::with_capacity(mods_array.len());
    for (idx, raw) in mods_array.iter().enumerate() {
        match serde_json::from_value::<CatalogEntry>(raw.clone()) {
            Ok(entry) => mods.push(entry),
            Err(err) => {
                let id_hint = raw
                    .get("id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("<no id>");
                log::warn!(
                    "Catalog entry #{} ('{}') dropped — {}",
                    idx,
                    id_hint,
                    err
                );
            }
        }
    }

    Ok(Catalog {
        version,
        updated_at,
        mods,
    })
}

pub fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::mods::types::ModKind;
    use tempfile::TempDir;

    fn empty_catalog() -> Catalog {
        Catalog {
            version: 1,
            updated_at: "2026-04-18T00:00:00Z".into(),
            mods: vec![],
        }
    }

    #[test]
    fn cache_round_trip() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("nested").join("catalog-cache.json");
        let cached = CachedCatalog {
            fetched_at_unix: 1_700_000_000,
            catalog: empty_catalog(),
        };
        save_cache(&path, &cached).unwrap();
        let loaded = load_cache(&path).unwrap();
        assert_eq!(loaded.fetched_at_unix, 1_700_000_000);
    }

    #[test]
    fn load_missing_cache_returns_none() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("catalog-cache.json");
        assert!(load_cache(&path).is_none());
    }

    #[test]
    fn is_stale_true_after_ttl() {
        let cached = CachedCatalog {
            fetched_at_unix: 100,
            catalog: empty_catalog(),
        };
        assert!(cached.is_stale(100 + CACHE_TTL_SECS + 1));
        assert!(!cached.is_stale(100 + CACHE_TTL_SECS - 1));
    }

    #[test]
    fn is_stale_handles_clock_skew_gracefully() {
        let cached = CachedCatalog {
            fetched_at_unix: 10_000,
            catalog: empty_catalog(),
        };
        // "now" before "fetched_at" (clock rewound). Should treat as fresh,
        // not panic via underflow.
        assert!(!cached.is_stale(5_000));
    }

    #[test]
    fn catalog_parses_sample_with_one_entry() {
        let json = r#"{
            "version": 1,
            "updated_at": "2026-04-18T00:00:00Z",
            "mods": [{
                "id": "shinra",
                "kind": "external",
                "name": "Shinra Meter",
                "author": "neowutran",
                "short_description": "Damage meter",
                "version": "3.0.0",
                "download_url": "https://example.com/s.zip",
                "sha256": "deadbeef",
                "executable_relpath": "ShinraMeter.exe"
            }]
        }"#;
        let catalog: Catalog = serde_json::from_str(json).unwrap();
        assert_eq!(catalog.mods.len(), 1);
        assert_eq!(catalog.mods[0].kind, ModKind::External);
    }

    fn valid_entry_json(id: &str) -> String {
        format!(
            r#"{{
                "id": "{}",
                "kind": "external",
                "name": "Example",
                "author": "Someone",
                "short_description": "desc",
                "version": "1.0.0",
                "download_url": "https://example.com/x.zip",
                "sha256": "abcd",
                "executable_relpath": "x.exe"
            }}"#,
            id
        )
    }

    /// PRD 3.2.6.parse-error-filter: a bad entry (e.g. wrong type on a
    /// required field) drops just that entry. The rest of the catalog
    /// surfaces normally.
    #[test]
    fn malformed_entries_filtered() {
        let good = valid_entry_json("good.one");
        let good_two = valid_entry_json("good.two");
        // Bad entry: `kind` is a number instead of the required enum string.
        let bad = r#"{
            "id": "bad.entry",
            "kind": 42,
            "name": "Malformed",
            "author": "nobody",
            "short_description": "broken",
            "version": "0.0.0",
            "download_url": "https://example.com/m.zip",
            "sha256": "ff"
        }"#;
        let body = format!(
            r#"{{"version": 1, "updated_at": "2026-04-19T00:00:00Z",
                 "mods": [{}, {}, {}]}}"#,
            good, bad, good_two
        );

        let catalog = parse_catalog_tolerant(&body).expect("envelope valid");
        assert_eq!(catalog.mods.len(), 2, "bad entry filtered; 2 good survive");
        let ids: Vec<_> = catalog.mods.iter().map(|m| m.id.as_str()).collect();
        assert!(ids.contains(&"good.one"));
        assert!(ids.contains(&"good.two"));
        assert!(!ids.contains(&"bad.entry"));
    }

    /// An empty `mods` array is valid — returns an empty catalog without
    /// error. The envelope itself is still required.
    #[test]
    fn empty_mods_array_yields_empty_catalog() {
        let body = r#"{"version": 1, "updated_at": "2026-04-19T00:00:00Z", "mods": []}"#;
        let catalog = parse_catalog_tolerant(body).unwrap();
        assert!(catalog.mods.is_empty());
        assert_eq!(catalog.version, 1);
    }

    /// Envelope errors are hard errors — a broken `version` or missing
    /// `mods` field cannot be recovered by dropping entries.
    #[test]
    fn malformed_envelope_is_hard_error() {
        // Missing `mods` array.
        let body = r#"{"version": 1, "updated_at": "2026-04-19T00:00:00Z"}"#;
        let err = parse_catalog_tolerant(body).unwrap_err();
        assert!(err.contains("'mods'"), "got: {}", err);

        // `version` missing entirely.
        let body = r#"{"updated_at": "2026-04-19T00:00:00Z", "mods": []}"#;
        let err = parse_catalog_tolerant(body).unwrap_err();
        assert!(err.contains("'version'"), "got: {}", err);

        // Not even valid JSON.
        let err = parse_catalog_tolerant("not json").unwrap_err();
        assert!(err.contains("envelope"), "got: {}", err);
    }

    /// Every entry being malformed drops them all but still surfaces a
    /// valid (empty) catalog — the page renders an empty browse tab
    /// instead of an error banner, matching the reliability goal.
    #[test]
    fn every_entry_malformed_returns_empty_catalog() {
        let body = r#"{
            "version": 1,
            "updated_at": "2026-04-19T00:00:00Z",
            "mods": [
                {"id": "a", "kind": 42, "short_description": "broken"},
                {"not_even_an_entry": true}
            ]
        }"#;
        let catalog = parse_catalog_tolerant(body).unwrap();
        assert!(catalog.mods.is_empty());
    }
}
