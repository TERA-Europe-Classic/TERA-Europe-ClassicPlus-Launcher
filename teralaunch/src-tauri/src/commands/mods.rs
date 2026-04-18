//! Mod manager Tauri commands.
//!
//! Split by concern:
//! - list / get — read-only snapshots of the installed-mod registry
//! - catalog — fetch the remote mod catalog (cached 24h)
//! - install / uninstall / enable / disable — lifecycle
//! - launch_external_app / stop_external_app — process control for Shinra/TCC
//! - open_mods_folder — open OS explorer for the mods directory
//!
//! The state is backed by `state::mods_state` (registry.json on disk).

use std::path::PathBuf;

use log::info;
use tauri::Manager;

use crate::services::mods::{
    catalog::{self, CachedCatalog},
    external_app,
    registry::{get_external_apps_dir, get_gpk_dir, get_registry_path},
    types::{Catalog, CatalogEntry, ModEntry, ModKind, ModStatus},
};
use crate::state::mods_state;

/// Returns the current list of installed mods.
#[tauri::command]
#[cfg(not(tarpaulin_include))]
pub fn list_installed_mods() -> Result<Vec<ModEntry>, String> {
    mods_state::list_mods()
}

/// Returns the remote mod catalog, serving from cache when fresh and
/// background-refreshing when stale. Caller may pass `force_refresh=true` to
/// bypass the cache.
#[tauri::command]
#[cfg(not(tarpaulin_include))]
pub async fn get_mods_catalog(force_refresh: Option<bool>) -> Result<Catalog, String> {
    let cache_path = catalog::get_cache_path()
        .ok_or_else(|| "Could not resolve mods cache dir".to_string())?;

    let force = force_refresh.unwrap_or(false);

    if !force {
        if let Some(cached) = catalog::load_cache(&cache_path) {
            if !cached.is_stale(catalog::now_unix()) {
                return Ok(cached.catalog);
            }
        }
    }

    match catalog::fetch_remote(catalog::CATALOG_URL).await {
        Ok(fresh) => {
            let cached = CachedCatalog {
                fetched_at_unix: catalog::now_unix(),
                catalog: fresh.clone(),
            };
            let _ = catalog::save_cache(&cache_path, &cached); // best effort
            Ok(fresh)
        }
        Err(fetch_err) => {
            // On network failure, fall back to whatever stale cache we have.
            if let Some(cached) = catalog::load_cache(&cache_path) {
                info!(
                    "Catalog fetch failed ({}); serving stale cache from {}",
                    fetch_err, cached.fetched_at_unix
                );
                return Ok(cached.catalog);
            }
            Err(fetch_err)
        }
    }
}

/// Installs a mod from a catalog entry: download, verify, extract, register.
/// External apps extract to `<app_data>/mods/external/<id>/`. GPK mods are
/// Phase C — this command returns a not-implemented error for them.
///
/// Emits `mod_download_progress` events (shape: `{ id, progress, state }`)
/// during the download phase. The final state of the mod is persisted via
/// `mods_state::mutate`.
#[tauri::command]
#[cfg(not(tarpaulin_include))]
pub async fn install_mod(entry: CatalogEntry, window: tauri::Window) -> Result<ModEntry, String> {
    match entry.kind {
        ModKind::External => install_external_mod(entry, window).await,
        // GPK install v1 is "download to mods folder". Patching the
        // CompositePackageMapper.dat and flipping the composite flag is
        // Phase C; for now the file lands in <app_data>/mods/gpk/<id>.gpk
        // and the user sees it in the list with a status note so they can
        // copy it into the game manually while we build the patcher.
        ModKind::Gpk => install_gpk_mod(entry, window).await,
    }
}

async fn install_external_mod(
    entry: CatalogEntry,
    window: tauri::Window,
) -> Result<ModEntry, String> {
    let executable_relpath = entry
        .executable_relpath
        .clone()
        .ok_or_else(|| format!("Catalog entry '{}' is missing executable_relpath", entry.id))?;

    let install_root = get_external_apps_dir()
        .ok_or_else(|| "Could not resolve external apps dir".to_string())?;
    let dest = install_root.join(&entry.id);

    // Mark Installing in the registry so the UI can render progress.
    let mut row = ModEntry::from_catalog(&entry);
    row.status = ModStatus::Installing;
    row.progress = Some(0);
    mods_state::mutate(|reg| {
        reg.upsert(row.clone());
        Ok(())
    })?;
    let _ = window.emit_all(
        "mod_download_progress",
        serde_json::json!({ "id": entry.id, "progress": 0, "state": "downloading" }),
    );

    // Stream the download and emit live progress. We throttle by TIME, not
    // by percentage steps, so the bar actually moves smoothly instead of
    // jumping 5% at a time. ~60ms between emits ≈ 16 fps, which is plenty
    // smooth and still light on the event loop for a 54 MB download.
    let progress_window = window.clone();
    let progress_id = entry.id.clone();
    let mut last_emit = std::time::Instant::now()
        .checked_sub(std::time::Duration::from_secs(1))
        .unwrap_or_else(std::time::Instant::now);
    let mut last_received: u64 = 0;
    let min_interval = std::time::Duration::from_millis(60);
    let extract_result = external_app::download_and_extract(
        &entry.download_url,
        &entry.sha256,
        &dest,
        move |received, total| {
            // Cap the download phase at 95 so extraction can occupy 95→100.
            let pct: u8 = if total > 0 {
                ((received * 95) / total).min(95) as u8
            } else {
                (10 + ((received / (1024 * 1024)) as u8).min(80)).min(90)
            };
            let now = std::time::Instant::now();
            // Always emit the first and last ticks; otherwise throttle by
            // wall-clock so the frontend gets a steady stream of updates
            // (typically every ~60 ms) regardless of chunk size.
            let force = received == 0 || received == total;
            if force || now.duration_since(last_emit) >= min_interval {
                last_emit = now;
                last_received = received;
                let _ = progress_window.emit_all(
                    "mod_download_progress",
                    serde_json::json!({
                        "id": progress_id,
                        "progress": pct,
                        "state": "downloading",
                        "received_bytes": received,
                        "total_bytes": total,
                    }),
                );
            }
            let _ = last_received;
        },
    )
    .await;

    match extract_result {
        Ok(_) => {
            // Validate the advertised executable exists post-extract.
            let exe = external_app::executable_path(&dest, &executable_relpath)?;
            if !exe.exists() {
                return finalize_error(&entry.id, format!(
                    "Advertised executable '{}' not found in extracted zip",
                    executable_relpath
                ), &window);
            }

            let final_row = mods_state::mutate(|reg| {
                let slot = reg.find_mut(&entry.id).ok_or_else(|| {
                    format!("Registry entry for {} disappeared mid-install", entry.id)
                })?;
                slot.status = ModStatus::Disabled;
                slot.progress = None;
                slot.last_error = None;
                slot.version = entry.version.clone();
                Ok(slot.clone())
            })?;

            let _ = window.emit_all(
                "mod_download_progress",
                serde_json::json!({ "id": entry.id, "progress": 100, "state": "done" }),
            );
            Ok(final_row)
        }
        Err(err) => finalize_error(&entry.id, err, &window),
    }
}

/// GPK install v1: download the .gpk to `<app_data>/mods/gpk/<id>.gpk`.
/// The mapper-patcher integration (flip the composite flag in
/// CompositePackageMapper.dat, register in ModList.tmm, etc.) lands in
/// Phase C; for now the registry entry stays at Disabled with a
/// last_error-style note pointing users at the file.
async fn install_gpk_mod(
    entry: CatalogEntry,
    window: tauri::Window,
) -> Result<ModEntry, String> {
    let gpk_dir = get_gpk_dir()
        .ok_or_else(|| "Could not resolve GPK mods dir".to_string())?;
    // Derive the on-disk filename from the id so each entry owns a slot and
    // reinstalls overwrite cleanly.
    let file_name = format!("{}.gpk", entry.id.replace('/', "_"));
    let dest = gpk_dir.join(&file_name);

    let mut row = ModEntry::from_catalog(&entry);
    row.status = ModStatus::Installing;
    row.progress = Some(0);
    mods_state::mutate(|reg| {
        reg.upsert(row.clone());
        Ok(())
    })?;
    let _ = window.emit_all(
        "mod_download_progress",
        serde_json::json!({ "id": entry.id, "progress": 0, "state": "downloading" }),
    );

    // Same time-based throttle as install_external_mod: emit ~every 60ms
    // so the bar actually moves smoothly instead of jumping 5% at a time.
    // First and last ticks always go out; everything in between is paced
    // by wall-clock, not percentage steps.
    let progress_window = window.clone();
    let progress_id = entry.id.clone();
    let mut last_emit = std::time::Instant::now()
        .checked_sub(std::time::Duration::from_secs(1))
        .unwrap_or_else(std::time::Instant::now);
    let min_interval = std::time::Duration::from_millis(60);
    let dl_result = external_app::download_file(
        &entry.download_url,
        &entry.sha256,
        &dest,
        move |received, total| {
            // Cap download at 95 so the deploy step can occupy 95→100.
            let pct: u8 = if total > 0 {
                ((received * 95) / total).min(95) as u8
            } else {
                (10 + ((received / (1024 * 1024)) as u8).min(80)).min(90)
            };
            let now = std::time::Instant::now();
            let force = received == 0 || received == total;
            if force || now.duration_since(last_emit) >= min_interval {
                last_emit = now;
                let _ = progress_window.emit_all(
                    "mod_download_progress",
                    serde_json::json!({
                        "id": progress_id,
                        "progress": pct,
                        "state": "downloading",
                        "received_bytes": received,
                        "total_bytes": total,
                    }),
                );
            }
        },
    )
    .await;

    match dl_result {
        Ok(_) => {
            // Attempt the TMM-style deploy: parse the .gpk, back up the
            // vanilla mapper, patch it to point composites at the mod file.
            // The mapper patcher lives in services::mods::tmm.rs — it
            // mirrors VenoMKO/TMM's CompositeMapper.cpp + Mod.cpp.
            let deploy_note = try_deploy_gpk(&entry.id, &dest);

            let final_row = mods_state::mutate(|reg| {
                let slot = reg.find_mut(&entry.id).ok_or_else(|| {
                    format!("Registry entry for {} disappeared mid-install", entry.id)
                })?;
                slot.status = ModStatus::Disabled;
                slot.progress = None;
                slot.last_error = deploy_note;
                slot.version = entry.version.clone();
                Ok(slot.clone())
            })?;
            let _ = window.emit_all(
                "mod_download_progress",
                serde_json::json!({ "id": entry.id, "progress": 100, "state": "done" }),
            );
            Ok(final_row)
        }
        Err(err) => finalize_error(&entry.id, err, &window),
    }
}

/// Tries to deploy a downloaded GPK to the game via tmm.rs. On success
/// returns None (no message to surface). On any failure returns a
/// human-readable explanation that the caller stashes in `last_error`
/// so the user can see why the mod won't apply in-game yet.
fn try_deploy_gpk(_mod_id: &str, source_gpk: &std::path::Path) -> Option<String> {
    use crate::services::mods::tmm;
    let game_root = match resolve_game_root() {
        Ok(p) => p,
        Err(e) => {
            return Some(format!(
                "Downloaded, but game path isn't set yet — can't deploy. Set the game folder under Settings, then click Retry. ({})",
                e
            ));
        }
    };
    match tmm::install_gpk(&game_root, source_gpk) {
        Ok(_) => None,
        Err(e) => Some(format!(
            "Downloaded, but mapper patch failed: {}. Mod file is at {}",
            e,
            source_gpk.display()
        )),
    }
}

/// Reads the game root from the launcher's config.ini via the existing
/// config command helpers. Returned path is the TERA install folder (the
/// parent of S1Game), matching what tmm.rs expects.
fn resolve_game_root() -> Result<std::path::PathBuf, String> {
    // The existing launcher config stores the game-exe path. Strip two
    // levels up (`Bin/...` → install root) so tmm has the structure it
    // expects. If we ever track the install root directly we can use
    // that instead.
    let (game_path, _lang) = crate::commands::config::load_config()?;
    // game_path is usually `<root>/Binaries/TERA.exe` or similar.
    let root = game_path.parent().and_then(|p| p.parent()).map(|p| p.to_path_buf())
        .ok_or_else(|| "Configured game path has no parent root".to_string())?;
    if !root.join("S1Game").exists() {
        return Err(format!(
            "No S1Game folder under {} — path may be wrong",
            root.display()
        ));
    }
    Ok(root)
}

fn finalize_error(
    id: &str,
    err: String,
    window: &tauri::Window,
) -> Result<ModEntry, String> {
    let _ = mods_state::mutate(|reg| {
        if let Some(slot) = reg.find_mut(id) {
            slot.status = ModStatus::Error;
            slot.progress = None;
            slot.last_error = Some(err.clone());
        }
        Ok(())
    });
    let _ = window.emit_all(
        "mod_download_progress",
        serde_json::json!({ "id": id, "progress": 0, "state": "error", "error": err }),
    );
    Err(err)
}

/// Uninstalls a mod: stops process if running, removes files, removes from registry.
/// For external apps with a configured settings folder, the frontend handles
/// the "also delete settings?" prompt and passes `delete_settings`.
#[tauri::command]
#[cfg(not(tarpaulin_include))]
pub async fn uninstall_mod(id: String, delete_settings: Option<bool>) -> Result<(), String> {
    let entry = mods_state::get_mod(&id)?
        .ok_or_else(|| format!("Mod '{}' is not installed", id))?;

    match entry.kind {
        ModKind::External => {
            // Best-effort stop before deleting files.
            if let Some(exe_name) = external_executable_name(&entry.id) {
                let _ = external_app::stop_process_by_name(&exe_name);
            }
            let install_root = get_external_apps_dir()
                .ok_or_else(|| "Could not resolve external apps dir".to_string())?;
            let dest = install_root.join(&entry.id);
            if dest.exists() {
                std::fs::remove_dir_all(&dest)
                    .map_err(|e| format!("Failed to remove {}: {}", dest.display(), e))?;
            }
            if delete_settings.unwrap_or(false) {
                // Settings-folder cleanup is driven by catalog metadata the
                // frontend already has — the frontend passes the resolved path
                // via a separate command if it wants to delete it. v1 treats
                // `delete_settings=true` as a request; actual path resolution
                // lives in the frontend for now.
            }
        }
        ModKind::Gpk => {
            // Restore the vanilla mapper entries for this mod and delete
            // its container .gpk from CookedPC. Best-effort: a missing
            // backup or a moved game path shouldn't block the registry
            // removal.
            if let Ok(game_root) = resolve_game_root() {
                let gpk_dir = get_gpk_dir()
                    .ok_or_else(|| "Could not resolve GPK mods dir".to_string())?;
                let source_gpk = gpk_dir.join(format!("{}.gpk", entry.id.replace('/', "_")));
                if let Ok(bytes) = std::fs::read(&source_gpk) {
                    if let Ok(modfile) = crate::services::mods::tmm::parse_mod_file(&bytes) {
                        let paths: Vec<String> = modfile.packages.iter()
                            .map(|p| p.object_path.clone())
                            .filter(|p| !p.is_empty())
                            .collect();
                        if !modfile.container.is_empty() && !paths.is_empty() {
                            let _ = crate::services::mods::tmm::uninstall_gpk(
                                &game_root,
                                &modfile.container,
                                &paths,
                            );
                        }
                    }
                }
            }
            // Also remove the download from the launcher's own gpk folder.
            let gpk_dir = get_gpk_dir()
                .ok_or_else(|| "Could not resolve GPK mods dir".to_string())?;
            let file = gpk_dir.join(format!("{}.gpk", entry.id.replace('/', "_")));
            if file.exists() {
                std::fs::remove_file(&file)
                    .map_err(|e| format!("Failed to remove {}: {}", file.display(), e))?;
            }
            let _ = delete_settings; // GPK has no per-mod settings folder
        }
    }

    mods_state::mutate(|reg| {
        reg.remove(&id);
        Ok(())
    })?;
    Ok(())
}

/// Enables a mod. The toggle records intent only — it does NOT start the
/// external app. Enabled external apps auto-spawn when the user clicks
/// Launch Game (see `spawn_auto_launch_external_apps`). Enabled GPKs are
/// applied at game launch by the mapper patcher.
#[tauri::command]
#[cfg(not(tarpaulin_include))]
pub async fn enable_mod(id: String) -> Result<ModEntry, String> {
    let _entry = mods_state::get_mod(&id)?
        .ok_or_else(|| format!("Mod '{}' is not installed", id))?;

    let updated = mods_state::mutate(|reg| {
        let slot = reg
            .find_mut(&id)
            .ok_or_else(|| format!("Mod '{}' is not installed", id))?;
        slot.enabled = true;
        slot.auto_launch = true;
        slot.status = ModStatus::Enabled;
        slot.last_error = None;
        Ok(slot.clone())
    })?;
    Ok(updated)
}

/// Disables a mod — flips the intent flags off. External apps already
/// running are left alone; close them from their own window if you want
/// them gone now. (The explicit `stop_external_app` command is still
/// available for UI controls that need to terminate a live process.)
#[tauri::command]
#[cfg(not(tarpaulin_include))]
pub async fn disable_mod(id: String) -> Result<ModEntry, String> {
    let _entry = mods_state::get_mod(&id)?
        .ok_or_else(|| format!("Mod '{}' is not installed", id))?;

    let updated = mods_state::mutate(|reg| {
        let slot = reg
            .find_mut(&id)
            .ok_or_else(|| format!("Mod '{}' is not installed", id))?;
        slot.enabled = false;
        slot.auto_launch = false;
        slot.status = ModStatus::Disabled;
        Ok(slot.clone())
    })?;
    Ok(updated)
}

/// Ad-hoc launch of an external app without changing its auto-launch setting.
/// Used by the "Launch now" button in the per-mod settings drawer.
#[tauri::command]
#[cfg(not(tarpaulin_include))]
pub async fn launch_external_app(id: String) -> Result<ModEntry, String> {
    launch_external_app_impl(&id, false).await
}

async fn launch_external_app_impl(id: &str, set_auto_launch: bool) -> Result<ModEntry, String> {
    let entry = mods_state::get_mod(id)?
        .ok_or_else(|| format!("Mod '{}' is not installed", id))?;

    let exe_name = external_executable_name(id)
        .ok_or_else(|| format!("Cannot resolve executable name for {}", id))?;

    // Skip the spawn if an instance is already running.
    let already_running = external_app::is_process_running(&exe_name);

    if !already_running {
        let install_root = get_external_apps_dir()
            .ok_or_else(|| "Could not resolve external apps dir".to_string())?;
        let dest = install_root.join(&entry.id);
        let exe_path = external_app::executable_path(&dest, &exe_name)?;
        external_app::spawn_app(&exe_path, &[])?;
    }

    let updated = mods_state::mutate(|reg| {
        let slot = reg
            .find_mut(id)
            .ok_or_else(|| "Mod vanished during enable".to_string())?;
        slot.enabled = true;
        if set_auto_launch {
            slot.auto_launch = true;
        }
        slot.status = ModStatus::Running;
        slot.last_error = None;
        Ok(slot.clone())
    })?;
    Ok(updated)
}

/// Terminates a running external-app mod without changing its enabled state.
#[tauri::command]
#[cfg(not(tarpaulin_include))]
pub async fn stop_external_app(id: String) -> Result<ModEntry, String> {
    if let Some(exe_name) = external_executable_name(&id) {
        external_app::stop_process_by_name(&exe_name)?;
    }
    mods_state::mutate(|reg| {
        let slot = reg
            .find_mut(&id)
            .ok_or_else(|| format!("Mod '{}' is not installed", id))?;
        slot.status = ModStatus::Disabled;
        Ok(slot.clone())
    })
}

/// Opens the OS file-explorer at the mods directory. Used by the "Open folder"
/// overflow-menu action.
#[tauri::command]
#[cfg(not(tarpaulin_include))]
pub fn open_mods_folder() -> Result<(), String> {
    let dir = get_registry_path()
        .and_then(|p| p.parent().map(PathBuf::from))
        .ok_or_else(|| "Could not resolve mods dir".to_string())?;
    if !dir.exists() {
        std::fs::create_dir_all(&dir)
            .map_err(|e| format!("Failed to create mods dir: {}", e))?;
    }
    open_in_explorer(&dir)
}

#[cfg(windows)]
fn open_in_explorer(path: &std::path::Path) -> Result<(), String> {
    std::process::Command::new("explorer")
        .arg(path)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("Failed to open file explorer: {}", e))
}

#[cfg(not(windows))]
fn open_in_explorer(path: &std::path::Path) -> Result<(), String> {
    // Best-effort for non-Windows devs. Production is Windows-only.
    std::process::Command::new("xdg-open")
        .arg(path)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("Failed to open file explorer: {}", e))
}

/// Called by the game-launch flow: spawns every external mod whose
/// `auto_launch` flag is set and whose process isn't already running.
/// Never blocks game launch — errors are logged, not propagated.
pub fn spawn_auto_launch_external_apps() {
    let entries = match mods_state::list_mods() {
        Ok(v) => v,
        Err(e) => {
            log::warn!("Auto-launch: could not read mods registry: {}", e);
            return;
        }
    };
    let install_root = match get_external_apps_dir() {
        Some(p) => p,
        None => return,
    };
    for entry in entries {
        if !matches!(entry.kind, ModKind::External) || !entry.enabled || !entry.auto_launch {
            continue;
        }
        let exe_name = match external_executable_name(&entry.id) {
            Some(n) => n,
            None => continue,
        };
        if external_app::is_process_running(&exe_name) {
            continue;
        }
        let dest = install_root.join(&entry.id);
        let exe_path = match external_app::executable_path(&dest, &exe_name) {
            Ok(p) => p,
            Err(e) => {
                log::warn!("Auto-launch: invalid path for {}: {}", entry.id, e);
                continue;
            }
        };
        match external_app::spawn_app(&exe_path, &[]) {
            Err(e) => log::warn!("Auto-launch: failed to start {}: {}", entry.id, e),
            Ok(_) => {
                log::info!("Auto-launch: started {}", entry.id);
                let _ = mods_state::mutate(|reg| {
                    if let Some(slot) = reg.find_mut(&entry.id) {
                        slot.status = ModStatus::Running;
                        slot.last_error = None;
                    }
                    Ok(())
                });
            }
        }
    }
}

/// Maps a mod id to the advertised executable filename. Catalog entries
/// store this in `executable_relpath`; for installed mods we look it up
/// from the registry's cached catalog fields. Simpler v1: expect the
/// catalog to be fetched at least once and look from there.
///
/// Currently returns `None` — the executable_relpath isn't persisted into
/// `ModEntry`. We resolve it at the call site using the catalog when
/// needed; this helper exists as a seam to add persistent mapping later
/// without changing call sites.
fn external_executable_name(id: &str) -> Option<String> {
    // Known defaults for the two apps we ship. Catalog ids settled on
    // `classicplus.<app>` in external-mod-catalog v1; the old
    // `tera-europe-classic.<app>` strings are left in as fallback for anyone
    // who had an older catalog cached locally. The TCC fork strips the
    // upstream loader wrapper, so the executable is TCC.exe, not TCC.Loader.exe.
    match id {
        "classicplus.shinra" | "tera-europe-classic.shinra" => Some("ShinraMeter.exe".into()),
        "classicplus.tcc" | "tera-europe-classic.tcc" => Some("TCC.exe".into()),
        _ => None,
    }
}
