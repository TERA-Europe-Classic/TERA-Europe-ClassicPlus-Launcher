//! On-disk registry of installed mods.
//!
//! Stored at `<app_data>/mods/registry.json`. This is the launcher-native
//! source of truth for installed external apps and any GPK mods installed
//! through the launcher. It does NOT replace TMM's `ModList.tmm` — for GPK
//! mods, the `gpk` service keeps both files in sync so TMM still recognises
//! our mods.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::types::ModEntry;

/// Registry file shape. `version` future-proofs on-disk format changes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Registry {
    pub version: u32,
    pub mods: Vec<ModEntry>,
}

impl Default for Registry {
    fn default() -> Self {
        Self {
            version: 1,
            mods: Vec::new(),
        }
    }
}

impl Registry {
    /// Load registry from disk; fall back to an empty registry if the file
    /// is absent. Corrupted files are an error — we'd rather surface the
    /// problem than silently discard the user's mod list.
    pub fn load(path: &Path) -> Result<Self, String> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let body = fs::read_to_string(path)
            .map_err(|e| format!("Failed to read mod registry at {}: {}", path.display(), e))?;
        serde_json::from_str(&body)
            .map_err(|e| format!("Mod registry at {} is corrupted: {}", path.display(), e))
    }

    /// Atomically persist the registry. Writes to `<path>.tmp` first, then
    /// renames. The directory is created if missing.
    pub fn save(&self, path: &Path) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create mods dir {}: {}", parent.display(), e))?;
        }
        let body = serde_json::to_string_pretty(self)
            .map_err(|e| format!("Failed to serialize registry: {}", e))?;
        let tmp = path.with_extension("json.tmp");
        fs::write(&tmp, body)
            .map_err(|e| format!("Failed to write registry tmp {}: {}", tmp.display(), e))?;
        fs::rename(&tmp, path)
            .map_err(|e| format!("Failed to rename registry {}: {}", path.display(), e))
    }

    pub fn find(&self, id: &str) -> Option<&ModEntry> {
        self.mods.iter().find(|m| m.id == id)
    }

    pub fn find_mut(&mut self, id: &str) -> Option<&mut ModEntry> {
        self.mods.iter_mut().find(|m| m.id == id)
    }

    /// Insert or replace by id.
    pub fn upsert(&mut self, entry: ModEntry) {
        if let Some(slot) = self.find_mut(&entry.id) {
            *slot = entry;
        } else {
            self.mods.push(entry);
        }
    }

    pub fn remove(&mut self, id: &str) -> Option<ModEntry> {
        let idx = self.mods.iter().position(|m| m.id == id)?;
        Some(self.mods.remove(idx))
    }
}

/// Returns `<app_data>/mods/registry.json`. Uses the same config-dir base as
/// the launcher's INI so everything lives together.
pub fn get_registry_path() -> Option<PathBuf> {
    dirs_next::config_dir().map(|d| {
        d.join("Crazy-eSports-ClassicPlus")
            .join("mods")
            .join("registry.json")
    })
}

/// Returns the directory external-app mods are extracted into.
pub fn get_external_apps_dir() -> Option<PathBuf> {
    dirs_next::config_dir().map(|d| {
        d.join("Crazy-eSports-ClassicPlus")
            .join("mods")
            .join("external")
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::mods::types::{ModKind, ModStatus};
    use tempfile::TempDir;

    fn sample_entry(id: &str, kind: ModKind) -> ModEntry {
        ModEntry {
            id: id.to_string(),
            kind,
            name: "Sample".into(),
            author: "Author".into(),
            description: "Desc".into(),
            version: "1.0".into(),
            status: ModStatus::Disabled,
            source_url: None,
            icon_url: None,
            progress: None,
            last_error: None,
            auto_launch: false,
            enabled: false,
            license: None,
            credits: None,
            long_description: None,
            screenshots: vec![],
        }
    }

    #[test]
    fn load_missing_file_returns_empty_registry() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("registry.json");
        let reg = Registry::load(&path).unwrap();
        assert!(reg.mods.is_empty());
        assert_eq!(reg.version, 1);
    }

    #[test]
    fn save_then_load_round_trips() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("nested").join("registry.json");
        let mut reg = Registry::default();
        reg.upsert(sample_entry("shinra", ModKind::External));
        reg.upsert(sample_entry("minimap", ModKind::Gpk));
        reg.save(&path).unwrap();

        let loaded = Registry::load(&path).unwrap();
        assert_eq!(loaded.mods.len(), 2);
        assert!(loaded.find("shinra").is_some());
        assert!(loaded.find("minimap").is_some());
    }

    #[test]
    fn upsert_replaces_existing_by_id() {
        let mut reg = Registry::default();
        reg.upsert(sample_entry("shinra", ModKind::External));
        let mut updated = sample_entry("shinra", ModKind::External);
        updated.version = "9.9".into();
        reg.upsert(updated);
        assert_eq!(reg.mods.len(), 1);
        assert_eq!(reg.find("shinra").unwrap().version, "9.9");
    }

    #[test]
    fn remove_returns_entry_and_drops_from_list() {
        let mut reg = Registry::default();
        reg.upsert(sample_entry("shinra", ModKind::External));
        let removed = reg.remove("shinra");
        assert!(removed.is_some());
        assert!(reg.find("shinra").is_none());
        assert!(reg.remove("shinra").is_none());
    }

    #[test]
    fn load_corrupted_file_errors() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("registry.json");
        fs::write(&path, b"{ not json").unwrap();
        let err = Registry::load(&path).unwrap_err();
        assert!(err.contains("corrupted"));
    }
}
