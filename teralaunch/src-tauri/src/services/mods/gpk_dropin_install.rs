//! Drop-in install path for Type-D GPK mods whose target package isn't
//! in v100 vanilla's PkgMapper. Writes the (already-x64) GPK directly to
//! S1Game/CookedPC/<target_filename>.gpk and removes it on uninstall.
//!
//! Type-D detection happens at the catalog level (deploy_strategy=Dropin);
//! this module trusts its caller and does not introspect the GPK itself
//! beyond a minimal sanity check.

use std::fs;
use std::path::Path;

use super::gpk::{is_safe_gpk_container_filename, COOKED_PC_DIR};

/// Install a payload as `S1Game/CookedPC/<target_filename>`.
///
/// Refuses to overwrite an existing file (Type-D mods by definition target
/// packages that don't exist in vanilla; an existing file means either a
/// data-quality issue in the catalog or a stale install).
pub fn install_dropin(
    game_root: &Path,
    mod_id: &str,
    target_filename: &str,
    payload: &[u8],
) -> Result<(), String> {
    if !is_safe_gpk_container_filename(target_filename) {
        return Err(format!(
            "drop-in install of '{mod_id}': unsafe target filename '{target_filename}'"
        ));
    }
    let cooked = game_root.join(COOKED_PC_DIR);
    fs::create_dir_all(&cooked)
        .map_err(|e| format!("create CookedPC dir {}: {e}", cooked.display()))?;
    let target = cooked.join(target_filename);
    if target.exists() {
        return Err(format!(
            "drop-in install of '{mod_id}': refusing to overwrite existing {} — Type-D mods must target a non-vanilla filename",
            target.display()
        ));
    }
    fs::write(&target, payload).map_err(|e| format!("write {}: {e}", target.display()))?;
    Ok(())
}

/// Remove a previously-dropin-installed file. Idempotent: missing file is OK.
pub fn uninstall_dropin(
    game_root: &Path,
    _mod_id: &str,
    target_filename: &str,
) -> Result<(), String> {
    if !is_safe_gpk_container_filename(target_filename) {
        return Err(format!(
            "drop-in uninstall: unsafe target filename '{target_filename}'"
        ));
    }
    let target = game_root.join(COOKED_PC_DIR).join(target_filename);
    if target.exists() {
        fs::remove_file(&target)
            .map_err(|e| format!("remove {}: {e}", target.display()))?;
    }
    Ok(())
}
