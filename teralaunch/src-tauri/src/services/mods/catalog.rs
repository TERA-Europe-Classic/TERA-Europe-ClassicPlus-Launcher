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

use super::types::Catalog;

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
/// `reqwest`-based HTTP stack.
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

    response
        .json::<Catalog>()
        .await
        .map_err(|e| format!("Catalog JSON is malformed: {}", e))
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
}
