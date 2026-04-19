//! Runtime state for the mod manager.
//!
//! The on-disk registry (`services/mods/registry.rs`) is the persistent source
//! of truth. This module wraps it in a process-global `RwLock` so Tauri
//! commands can read/mutate it concurrently.
//!
//! The state is lazy-initialised on first access — the registry file is
//! loaded from disk once, then held in memory. Saves are always write-through:
//! every mutation persists before releasing the lock.

use std::path::PathBuf;
use std::sync::RwLock;

use lazy_static::lazy_static;

use crate::services::mods::registry::{get_registry_path, Registry};
use crate::services::mods::types::ModEntry;

lazy_static! {
    static ref MODS_STATE: RwLock<Option<MemoryState>> = RwLock::new(None);
}

struct MemoryState {
    registry: Registry,
    registry_path: PathBuf,
}

fn ensure_loaded() -> Result<(), String> {
    {
        let guard = MODS_STATE
            .read()
            .map_err(|e| format!("Mods state poisoned: {}", e))?;
        if guard.is_some() {
            return Ok(());
        }
    }

    let path = get_registry_path()
        .ok_or_else(|| "Could not resolve mods registry directory".to_string())?;
    let registry = Registry::load(&path)?;

    let mut guard = MODS_STATE
        .write()
        .map_err(|e| format!("Mods state poisoned: {}", e))?;
    // Double-checked init: another thread may have loaded while we waited.
    if guard.is_none() {
        *guard = Some(MemoryState {
            registry,
            registry_path: path,
        });
    }
    Ok(())
}

/// Reads the list of installed mods. Always returns a snapshot — safe to
/// serialize across the Tauri boundary.
pub fn list_mods() -> Result<Vec<ModEntry>, String> {
    ensure_loaded()?;
    let guard = MODS_STATE
        .read()
        .map_err(|e| format!("Mods state poisoned: {}", e))?;
    let state = guard
        .as_ref()
        .ok_or_else(|| "Mods state not initialised".to_string())?;
    Ok(state.registry.mods.clone())
}

/// Returns the entry for a single id, or `None` if it isn't installed.
pub fn get_mod(id: &str) -> Result<Option<ModEntry>, String> {
    ensure_loaded()?;
    let guard = MODS_STATE
        .read()
        .map_err(|e| format!("Mods state poisoned: {}", e))?;
    let state = guard
        .as_ref()
        .ok_or_else(|| "Mods state not initialised".to_string())?;
    Ok(state.registry.find(id).cloned())
}

/// Applies a mutation closure against the registry and persists the result.
/// Any error from the closure aborts the save.
pub fn mutate<F, T>(f: F) -> Result<T, String>
where
    F: FnOnce(&mut Registry) -> Result<T, String>,
{
    ensure_loaded()?;
    let mut guard = MODS_STATE
        .write()
        .map_err(|e| format!("Mods state poisoned: {}", e))?;
    let state = guard
        .as_mut()
        .ok_or_else(|| "Mods state not initialised".to_string())?;
    let result = f(&mut state.registry)?;
    state.registry.save(&state.registry_path)?;
    Ok(result)
}

/// Test-only: swap the in-memory state for a freshly-constructed one
/// backed by `path`. Lets integration tests use a temp registry file
/// without leaking into the process-global.
#[cfg(test)]
pub fn reset_for_test(path: PathBuf) {
    let mut guard = MODS_STATE.write().expect("mods state poisoned");
    *guard = Some(MemoryState {
        registry: Registry::load(&path).unwrap_or_default(),
        registry_path: path,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::mods::types::{ModKind, ModStatus};
    use tempfile::TempDir;

    fn sample(id: &str) -> ModEntry {
        ModEntry {
            id: id.into(),
            kind: ModKind::External,
            name: id.into(),
            author: "a".into(),
            description: "d".into(),
            version: "1".into(),
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
    fn mutate_then_list_returns_entry() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("registry.json");
        reset_for_test(path.clone());

        mutate(|reg| {
            reg.upsert(sample("shinra"));
            Ok(())
        })
        .unwrap();

        let mods = list_mods().unwrap();
        assert!(mods.iter().any(|m| m.id == "shinra"));

        // Verify it persisted to disk.
        let reloaded = Registry::load(&path).unwrap();
        assert!(reloaded.find("shinra").is_some());
    }

    #[test]
    fn get_mod_returns_none_for_missing() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("registry.json");
        reset_for_test(path);

        let result = get_mod("does-not-exist").unwrap();
        assert!(result.is_none());
    }
}
