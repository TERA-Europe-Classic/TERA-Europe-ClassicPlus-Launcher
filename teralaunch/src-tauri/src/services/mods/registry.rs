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
    ///
    /// PRD 3.2.2.crash-recovery: if the launcher was SIGKILLed (or the host
    /// crashed) mid-install, a row can be stranded in `ModStatus::Installing`
    /// forever. On every load we sweep those rows to `Error` with a
    /// last_error note so the UI shows a recoverable state and the user can
    /// retry or remove.
    pub fn load(path: &Path) -> Result<Self, String> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let body = fs::read_to_string(path)
            .map_err(|e| format!("Failed to read mod registry at {}: {}", path.display(), e))?;
        let mut reg: Self = serde_json::from_str(&body)
            .map_err(|e| format!("Mod registry at {} is corrupted: {}", path.display(), e))?;
        reg.recover_stuck_installs();
        Ok(reg)
    }

    /// Flips every row still marked `Installing` to `Error` with a last_error
    /// note, returning the count of rows touched. Idempotent — a second call
    /// on the recovered registry is a no-op.
    pub fn recover_stuck_installs(&mut self) -> usize {
        use super::types::ModStatus;
        let mut touched = 0;
        for m in self.mods.iter_mut() {
            if m.status == ModStatus::Installing {
                m.status = ModStatus::Error;
                m.last_error = Some(
                    "Install was interrupted (launcher exited mid-install). \
                     Click retry to re-run the download."
                        .to_string(),
                );
                m.progress = None;
                touched += 1;
            }
        }
        touched
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

    /// PRD 3.2.7.parallel-install-serialised: try to claim the install slot
    /// for `entry.id`. If a slot already exists with `ModStatus::Installing`,
    /// refuses the claim so the second concurrent install can't race the
    /// first on disk (double write to the same dest, two zip extractions,
    /// two GPK deploys stepping on each other). Otherwise upserts the entry
    /// with `Installing` status and takes ownership.
    ///
    /// Serialisation is cooperative — enforced at the `mods_state::mutate`
    /// boundary which already serialises on a single `Mutex<Registry>`. Two
    /// `install_*` commands fired back-to-back will enter `mutate` one at a
    /// time; the first claims, the second sees `Installing` and returns Err.
    pub fn try_claim_installing(&mut self, row: ModEntry) -> Result<(), String> {
        use super::types::ModStatus;
        if let Some(slot) = self.find(&row.id) {
            if matches!(slot.status, ModStatus::Installing) {
                return Err(format!(
                    "Install for '{}' is already in progress. Wait for it to finish, or check the Installed tab for errors.",
                    row.id
                ));
            }
        }
        self.upsert(row);
        Ok(())
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

/// Returns the directory GPK mods are stored in. Until the mapper patcher
/// ships (Phase C), the launcher just deposits files here; users can copy
/// them into the game manually in the meantime.
pub fn get_gpk_dir() -> Option<PathBuf> {
    dirs_next::config_dir().map(|d| d.join("Crazy-eSports-ClassicPlus").join("mods").join("gpk"))
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
            tags: vec![],
            gpk_files: vec![],
            compatibility_notes: None,
            last_verified_patch: None,
            download_count: None,
            long_description: None,
            screenshots: vec![],
            compatible_arch: None,
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

    // --- PRD 3.2.2.crash-recovery -------------------------------------------

    #[test]
    fn recover_stuck_installs_flips_installing_to_error() {
        let mut reg = Registry::default();
        let mut stuck = sample_entry("shinra", ModKind::External);
        stuck.status = ModStatus::Installing;
        stuck.progress = Some(42);
        reg.upsert(stuck);

        let mut healthy = sample_entry("minimap", ModKind::Gpk);
        healthy.status = ModStatus::Enabled;
        reg.upsert(healthy);

        let touched = reg.recover_stuck_installs();
        assert_eq!(touched, 1);
        assert_eq!(reg.find("shinra").unwrap().status, ModStatus::Error);
        let last_err = reg.find("shinra").unwrap().last_error.as_deref().unwrap();
        assert!(last_err.contains("interrupted"), "got {last_err:?}");
        assert!(reg.find("shinra").unwrap().progress.is_none());
        // Healthy row untouched.
        assert_eq!(reg.find("minimap").unwrap().status, ModStatus::Enabled);
    }

    #[test]
    fn recover_stuck_installs_is_idempotent() {
        let mut reg = Registry::default();
        let mut stuck = sample_entry("shinra", ModKind::External);
        stuck.status = ModStatus::Installing;
        reg.upsert(stuck);

        assert_eq!(reg.recover_stuck_installs(), 1);
        // Second call: no rows remain in Installing, nothing to do.
        assert_eq!(reg.recover_stuck_installs(), 0);
    }

    #[test]
    fn mid_install_sigkill_recovers_to_error() {
        // Full scenario: launcher persists a row in Installing state, then
        // dies. On next boot Registry::load() should flip it to Error.
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("registry.json");

        let mut pre_crash = Registry::default();
        let mut stuck = sample_entry("shinra", ModKind::External);
        stuck.status = ModStatus::Installing;
        stuck.progress = Some(73);
        pre_crash.upsert(stuck);
        pre_crash.save(&path).unwrap();

        // Simulate the process being SIGKILLed by just... not calling save
        // again. The on-disk state is what we'd find on next boot.

        let post_boot = Registry::load(&path).unwrap();
        let recovered = post_boot.find("shinra").unwrap();
        assert_eq!(recovered.status, ModStatus::Error);
        assert!(
            recovered
                .last_error
                .as_deref()
                .unwrap_or("")
                .contains("interrupted"),
            "last_error should describe the interruption, got {:?}",
            recovered.last_error
        );
        assert!(
            recovered.progress.is_none(),
            "stale progress must be cleared after recovery"
        );
    }

    #[test]
    fn load_does_not_touch_non_installing_rows() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("registry.json");

        let mut reg = Registry::default();
        for (id, s) in [
            ("disabled", ModStatus::Disabled),
            ("enabled", ModStatus::Enabled),
            ("running", ModStatus::Running),
            ("error", ModStatus::Error),
            ("update_available", ModStatus::UpdateAvailable),
            ("starting", ModStatus::Starting),
        ] {
            let mut e = sample_entry(id, ModKind::External);
            e.status = s;
            reg.upsert(e);
        }
        reg.save(&path).unwrap();

        let loaded = Registry::load(&path).unwrap();
        for (id, expected) in [
            ("disabled", ModStatus::Disabled),
            ("enabled", ModStatus::Enabled),
            ("running", ModStatus::Running),
            ("error", ModStatus::Error),
            ("update_available", ModStatus::UpdateAvailable),
            ("starting", ModStatus::Starting),
        ] {
            assert_eq!(
                loaded.find(id).unwrap().status,
                expected,
                "row {id} must be untouched by recovery"
            );
        }
    }

    fn installing_entry(id: &str) -> ModEntry {
        let mut e = sample_entry(id, ModKind::External);
        e.status = ModStatus::Installing;
        e.progress = Some(0);
        e
    }

    /// PRD 3.2.7.parallel-install-serialised: first claim wins, second
    /// claim on the same id sees `Installing` and is refused. Prevents
    /// two concurrent installs from racing on the same dest dir.
    #[test]
    fn same_id_serialised_second_claim_refused() {
        let mut reg = Registry::default();

        reg.try_claim_installing(installing_entry("classicplus.shinra"))
            .expect("first claim must succeed");
        assert!(matches!(
            reg.find("classicplus.shinra").unwrap().status,
            ModStatus::Installing
        ));

        let err = reg
            .try_claim_installing(installing_entry("classicplus.shinra"))
            .unwrap_err();
        assert!(
            err.contains("already in progress"),
            "expected already-in-progress message, got: {err}"
        );
        assert!(err.contains("classicplus.shinra"), "error names the id");
    }

    /// A claim is only refused when the row is currently Installing. If a
    /// previous install flipped the row to Error (or any non-Installing
    /// state), re-claiming must succeed — that's the normal retry path.
    #[test]
    fn reclaim_after_error_succeeds() {
        let mut reg = Registry::default();
        reg.try_claim_installing(installing_entry("classicplus.shinra"))
            .unwrap();

        // Simulate: first install failed; row flipped to Error.
        reg.find_mut("classicplus.shinra").unwrap().status = ModStatus::Error;

        // Retry claim must succeed and re-flip the row to Installing.
        reg.try_claim_installing(installing_entry("classicplus.shinra"))
            .expect("retry after error must succeed");
        assert!(matches!(
            reg.find("classicplus.shinra").unwrap().status,
            ModStatus::Installing
        ));
    }

    /// Claims on different ids don't interact — two installs of different
    /// mods can genuinely overlap because they touch disjoint dest dirs.
    #[test]
    fn different_ids_do_not_block_each_other() {
        let mut reg = Registry::default();
        reg.try_claim_installing(installing_entry("classicplus.shinra"))
            .expect("shinra claim succeeds");
        reg.try_claim_installing(installing_entry("classicplus.tcc"))
            .expect("tcc claim succeeds — different id");

        for id in ["classicplus.shinra", "classicplus.tcc"] {
            assert!(matches!(
                reg.find(id).unwrap().status,
                ModStatus::Installing
            ));
        }
    }

    /// First claim on a fresh id upserts the entry. Verifies the row
    /// actually lands with the Installing status set by the caller.
    #[test]
    fn first_claim_upserts_installing_row() {
        let mut reg = Registry::default();
        assert!(reg.find("classicplus.shinra").is_none());
        reg.try_claim_installing(installing_entry("classicplus.shinra"))
            .unwrap();
        let slot = reg.find("classicplus.shinra").unwrap();
        assert!(matches!(slot.status, ModStatus::Installing));
        assert_eq!(slot.progress, Some(0));
    }
}
