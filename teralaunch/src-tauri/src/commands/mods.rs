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
use tauri::Emitter;

use serde::{Deserialize, Serialize};

use crate::services::mods::{
    catalog::{self, CachedCatalog},
    external_app,
    gpk::{self, ModConflict},
    gpk_patch_deploy, patch_manifest,
    registry::{get_external_apps_dir, get_gpk_dir, get_registry_path},
    types::{Catalog, CatalogEntry, ModEntry, ModKind, ModStatus},
};
use crate::state::mods_state;

/// One row of catalog-vs-installed version mismatch. Returned by
/// `check_mod_updates` and consumed by `auto_update_enabled_mods`.
///
/// `enabled` is the user's intent (registry.enabled flag) — auto-update
/// only re-installs rows whose `enabled == true`. `current_version` is
/// what's on disk, `available_version` is what the catalog advertises.
/// `kind` is included so the frontend can render different copy for
/// external-app vs GPK updates if it wants to.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ModUpdateInfo {
    pub id: String,
    pub kind: ModKind,
    pub current_version: String,
    pub available_version: String,
    pub enabled: bool,
}

/// Pure version comparator. Strings differ → update available.
/// Pinned in a helper so the unit tests can exercise it without touching
/// the registry; v1 keeps it stupid (string equality) since catalog
/// versions are author-managed labels (e.g. "3.0.9-classicplus") rather
/// than semver — author bump intent is the trigger, not version math.
fn versions_differ(installed: &str, catalog: &str) -> bool {
    let i = installed.trim();
    let c = catalog.trim();
    !i.is_empty() && !c.is_empty() && i != c
}

/// Pure update-detector. Walks installed rows, looks up their catalog
/// counterpart, returns the diff list and the set of ids whose status
/// should flip to `UpdateAvailable`. Status flips skip rows that are
/// mid-flight (`Installing`/`Running`/`Starting`/`Error`) so an in-
/// progress install isn't visually overridden.
///
/// Split out so unit tests can run without the global registry.
fn detect_updates(installed: &[ModEntry], catalog_mods: &[CatalogEntry]) -> Vec<ModUpdateInfo> {
    use std::collections::HashMap;
    let by_id: HashMap<&str, &CatalogEntry> = catalog_mods
        .iter()
        .map(|c| (c.id.as_str(), c))
        .collect();

    installed
        .iter()
        .filter_map(|m| {
            let cat = by_id.get(m.id.as_str())?;
            if !versions_differ(&m.version, &cat.version) {
                return None;
            }
            Some(ModUpdateInfo {
                id: m.id.clone(),
                kind: m.kind,
                current_version: m.version.clone(),
                available_version: cat.version.clone(),
                enabled: m.enabled,
            })
        })
        .collect()
}

/// Status values whose UpdateAvailable pill must NOT replace the
/// existing display: a mid-flight install or a live process should keep
/// its real-time status visible.
fn should_flip_to_update_available(status: ModStatus) -> bool {
    !matches!(
        status,
        ModStatus::Installing | ModStatus::Running | ModStatus::Starting | ModStatus::Error
    )
}

/// Rebuilds the on-disk GPK deploy state to match the registry. Migrates
/// any registry slots installed by older launcher versions (legacy whole-
/// file copies into CookedPC) before running the per-mod disable + enable
/// cycle. The composite mapper is hard-restored from `.clean` once up
/// front so a removed-from-registry-but-still-on-disk standalone redirect
/// doesn't survive into the rebuild.
fn rebuild_gpk_runtime_state() -> Result<(), String> {
    let game_root = resolve_game_root()?;
    let app_root = patch_manifest::get_manifest_root()
        .ok_or_else(|| "Could not resolve patch-manifest root directory".to_string())?;

    migrate_legacy_gpk_installs(&game_root, &app_root)?;

    let installed = mods_state::list_mods()?;
    let gpk_mods: Vec<&ModEntry> = installed
        .iter()
        .filter(|entry| matches!(entry.kind, ModKind::Gpk))
        .collect();

    gpk::restore_clean_mapper_state(&game_root)?;

    for entry in &gpk_mods {
        if let Err(err) = gpk_patch_deploy::disable_via_patch(&game_root, &app_root, &entry.id) {
            log::warn!(
                "rebuild_gpk_runtime_state: disable_via_patch({}) failed: {err}",
                entry.id
            );
        }
    }

    for entry in gpk_mods.iter().filter(|e| e.enabled) {
        gpk_patch_deploy::enable_via_patch(&game_root, &app_root, &entry.id)
            .map_err(|err| format!("Failed to re-enable GPK mod '{}': {err}", entry.id))?;
    }
    Ok(())
}

/// Detects legacy-installed GPK slots (deployed_filename set + no
/// manifest persisted) and runs the legacy uninstall path to clean
/// CookedPC, then flips the registry row to needs-reinstall. Idempotent
/// — once a row has been migrated its `deployed_filename` is None, so
/// subsequent calls skip it.
fn migrate_legacy_gpk_installs(
    game_root: &std::path::Path,
    app_root: &std::path::Path,
) -> Result<(), String> {
    let installed = mods_state::list_mods()?;
    let candidates: Vec<(String, String)> = installed
        .into_iter()
        .filter(|m| matches!(m.kind, ModKind::Gpk))
        .filter_map(|m| m.deployed_filename.clone().map(|f| (m.id, f)))
        .collect();
    if candidates.is_empty() {
        return Ok(());
    }

    for (mod_id, deployed_filename) in candidates {
        let outcome = gpk_patch_deploy::migrate_legacy_install(
            game_root,
            app_root,
            &mod_id,
            &deployed_filename,
        );
        if let Some(err) = outcome.error.as_deref() {
            log::warn!("legacy migration cleanup for {mod_id} reported: {err}");
        }

        // If a manifest already exists the slot was already on the new flow
        // — leave the row alone. Otherwise mark it needs-reinstall and
        // clear deployed_filename so the migration is idempotent.
        let manifest_exists = matches!(
            crate::services::mods::manifest_store::load_manifest_at_root(app_root, &mod_id),
            Ok(Some(_))
        );
        if manifest_exists {
            continue;
        }

        let _ = mods_state::mutate(|reg| {
            if let Some(slot) = reg.find_mut(&mod_id) {
                slot.deployed_filename = None;
                slot.enabled = false;
                slot.auto_launch = false;
                slot.status = ModStatus::Error;
                slot.last_error = Some(
                    "This mod was installed by an older launcher version that overwrote vanilla files. \
                     The legacy install has been cleaned up. Click Reinstall to redeploy via the new patch-based flow."
                        .into(),
                );
            }
            Ok(())
        });
    }
    Ok(())
}

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
    let cache_path =
        catalog::get_cache_path().ok_or_else(|| "Could not resolve mods cache dir".to_string())?;

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

/// Force-refreshes the catalog, walks installed mods, returns rows whose
/// installed version differs from the catalog version. Side effect: any
/// row whose status is safely flippable (`Disabled`/`Enabled`/`NotInstalled`/
/// `UpdateAvailable`) is set to `UpdateAvailable` so the next
/// `list_installed_mods` already reflects the new state.
///
/// Called both from the mod-manager open path (decoration only — the
/// frontend renders the pill) and from the Launch button path
/// (`auto_update_enabled_mods` consumes the diff list to drive
/// reinstalls). Emits one `mod_update_available` event per detected
/// update so any view subscribed to live updates can refresh without
/// re-querying the list.
#[tauri::command]
#[cfg(not(tarpaulin_include))]
pub async fn check_mod_updates(window: tauri::Window) -> Result<Vec<ModUpdateInfo>, String> {
    // Force-refresh so a freshly-pushed catalog (e.g. a Shinra release
    // that just landed) is picked up without waiting on the 24h TTL.
    let catalog_doc = get_mods_catalog(Some(true)).await?;
    let installed = mods_state::list_mods()?;

    let updates = detect_updates(&installed, &catalog_doc.mods);

    if !updates.is_empty() {
        let update_ids: std::collections::HashSet<&str> =
            updates.iter().map(|u| u.id.as_str()).collect();
        let _ = mods_state::mutate(|reg| {
            for slot in reg.mods.iter_mut() {
                if update_ids.contains(slot.id.as_str())
                    && should_flip_to_update_available(slot.status)
                {
                    slot.status = ModStatus::UpdateAvailable;
                }
            }
            Ok(())
        });
    }

    for u in &updates {
        let _ = window.emit(
            "mod_update_available",
            serde_json::json!({
                "id": u.id,
                "current_version": u.current_version,
                "available_version": u.available_version,
                "enabled": u.enabled,
            }),
        );
    }

    Ok(updates)
}

/// Result of `auto_update_enabled_mods`. `attempted` is every enabled
/// mod that had an update queued; `failed_ids` is the subset whose
/// reinstall returned an error. Empty `failed_ids` means the launch
/// flow can proceed cleanly. Non-empty does NOT block the caller — the
/// frontend decides whether to surface a warning or proceed.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AutoUpdateResult {
    pub attempted: Vec<String>,
    pub failed_ids: Vec<String>,
}

/// Launch-button companion: detects updates for enabled mods only,
/// then reinstalls each via the existing `install_mod` flow so the user
/// gets the same `mod_download_progress` feedback they'd see clicking
/// Install in the manager. Disabled mods are skipped — the user has
/// signalled they don't want them, so spending a download budget on
/// them is wasteful and surprising.
///
/// Failures are collected, not propagated, so the launcher can still
/// start the game with the last-known-good install. The caller can
/// surface the failed ids inline if it cares.
#[tauri::command]
#[cfg(not(tarpaulin_include))]
pub async fn auto_update_enabled_mods(
    window: tauri::Window,
) -> Result<AutoUpdateResult, String> {
    let catalog_doc = get_mods_catalog(Some(true)).await?;
    let installed = mods_state::list_mods()?;

    let updates: Vec<ModUpdateInfo> = detect_updates(&installed, &catalog_doc.mods)
        .into_iter()
        .filter(|u| u.enabled)
        .collect();

    let mut attempted = Vec::with_capacity(updates.len());
    let mut failed_ids = Vec::new();

    let by_id: std::collections::HashMap<&str, &CatalogEntry> = catalog_doc
        .mods
        .iter()
        .map(|c| (c.id.as_str(), c))
        .collect();

    for update in updates {
        let Some(catalog_entry) = by_id.get(update.id.as_str()) else {
            continue;
        };
        attempted.push(update.id.clone());
        match install_mod((*catalog_entry).clone(), window.clone()).await {
            Ok(_) => log::info!("auto_update_enabled_mods: updated {}", update.id),
            Err(err) => {
                log::warn!(
                    "auto_update_enabled_mods: failed to update {}: {}",
                    update.id,
                    err
                );
                failed_ids.push(update.id.clone());
            }
        }
    }

    Ok(AutoUpdateResult {
        attempted,
        failed_ids,
    })
}

/// Applies the fresh-install defaults to a registry slot: enabled + auto-launch
/// on, status Enabled, progress cleared, version synced to the catalog entry.
/// `last_error` carries through any non-fatal deploy note (e.g. the GPK path
/// where the mapper patch failed soft but the .gpk is on disk).
///
/// PRD 3.3.12: new installs default to enabled so the user gets the mod they
/// just picked without an extra click — they can untoggle from the Installed
/// tab if they change their mind. Kept as a single helper so both the external
/// and GPK install paths can't drift on defaults.
fn finalize_installed_slot(
    slot: &mut ModEntry,
    new_version: &str,
    last_error: Option<String>,
    deployed_filename: Option<String>,
) {
    slot.enabled = true;
    slot.auto_launch = true;
    slot.status = ModStatus::Enabled;
    slot.progress = None;
    slot.last_error = last_error;
    slot.version = new_version.to_string();
    slot.deployed_filename = deployed_filename;
}

fn finalize_blocked_gpk_slot(slot: &mut ModEntry, new_version: &str, last_error: String) {
    slot.enabled = false;
    slot.auto_launch = false;
    slot.status = ModStatus::Error;
    slot.progress = None;
    slot.last_error = Some(last_error);
    slot.version = new_version.to_string();
    slot.deployed_filename = None;
}

struct GpkDeployOutcome {
    last_error: Option<String>,
    deployed_filename: Option<String>,
    blocks_enable: bool,
}

fn gpk_unsupported_diff_fallback(mod_name: &str) -> String {
    format!(
        "Mod '{mod_name}' could not be deployed via the patch-based flow. The mod may target a diff shape the launcher does not support yet (added exports, name/import drift, compressed packages, or class changes)."
    )
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

    let install_root =
        get_external_apps_dir().ok_or_else(|| "Could not resolve external apps dir".to_string())?;
    let dest = install_root.join(&entry.id);

    stop_external_app_before_update(&entry.id, external_app::stop_process_by_name);
    // fix.kill-before-extract: graceful_stop returns when the PID exits,
    // but Windows can hold file locks briefly afterwards. Poll for the
    // exe-name to disappear from the process table for up to ~3s before
    // proceeding so the unzip doesn't race the file-handle release on a
    // mod that was running at update time.
    if let Some(exe_name) = external_executable_name(&entry.id) {
        wait_for_process_gone(&exe_name, std::time::Duration::from_secs(3)).await;
    }

    // Mark Installing in the registry so the UI can render progress. The
    // claim is atomic with the check — if a parallel install of the same
    // id is already in progress, mods_state::mutate sees Installing and
    // refuses this claim (PRD 3.2.7). Without this, two simultaneous
    // installs would both write to the same dest and corrupt each other.
    let mut row = ModEntry::from_catalog(&entry);
    row.status = ModStatus::Installing;
    row.progress = Some(0);
    mods_state::mutate(|reg| reg.try_claim_installing(row.clone()))?;
    let _ = window.emit(
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
                let _ = progress_window.emit(
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
                return finalize_error(
                    &entry.id,
                    format!(
                        "Advertised executable '{}' not found in extracted zip",
                        executable_relpath
                    ),
                    &window,
                );
            }

            let final_row = mods_state::mutate(|reg| {
                let slot = reg.find_mut(&entry.id).ok_or_else(|| {
                    format!("Registry entry for {} disappeared mid-install", entry.id)
                })?;
                finalize_installed_slot(slot, &entry.version, None, None);
                Ok(slot.clone())
            })?;

            let _ = window.emit(
                "mod_download_progress",
                serde_json::json!({ "id": entry.id, "progress": 100, "state": "done" }),
            );
            let _ = rebuild_gpk_runtime_state();
            Ok(final_row)
        }
        Err(err) => finalize_error(&entry.id, err, &window),
    }
}

/// fix.kill-before-extract: poll `is_process_running` for up to `timeout`
/// so the extraction step waits until Windows has released the exe's file
/// handle. graceful_stop already TerminateProcess'd the PID; this is a
/// belt-and-braces wait for the file lock to actually drop.
async fn wait_for_process_gone(exe_name: &str, timeout: std::time::Duration) {
    let deadline = std::time::Instant::now() + timeout;
    let poll_interval = std::time::Duration::from_millis(150);
    while std::time::Instant::now() < deadline {
        if !external_app::is_process_running(exe_name) {
            return;
        }
        tokio::time::sleep(poll_interval).await;
    }
}

fn stop_external_app_before_update(
    id: &str,
    mut stop_process: impl FnMut(&str) -> Result<u32, String>,
) {
    let Some(exe_name) = external_executable_name(id) else {
        return;
    };

    if let Err(err) = stop_process(&exe_name) {
        log::warn!("install_external_mod: could not stop {id} before update: {err}");
    }
}

/// GPK install v1: download the .gpk to `<app_data>/mods/gpk/<id>.gpk`.
/// The mapper-patcher integration (flip the composite flag in
/// CompositePackageMapper.dat, register in ModList.tmm, etc.) lands in
/// Phase C; for now the registry entry stays at Disabled with a
/// last_error-style note pointing users at the file.
async fn install_gpk_mod(entry: CatalogEntry, window: tauri::Window) -> Result<ModEntry, String> {
    let gpk_dir = get_gpk_dir().ok_or_else(|| "Could not resolve GPK mods dir".to_string())?;
    // Derive the on-disk filename from the id so each entry owns a slot and
    // reinstalls overwrite cleanly.
    let file_name = format!("{}.gpk", entry.id.replace('/', "_"));
    let dest = gpk_dir.join(&file_name);

    let mut row = ModEntry::from_catalog(&entry);
    row.status = ModStatus::Installing;
    row.progress = Some(0);
    // See install_external_mod — atomic claim refuses concurrent installs
    // of the same id (PRD 3.2.7).
    mods_state::mutate(|reg| reg.try_claim_installing(row.clone()))?;
    let _ = window.emit(
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
                let _ = progress_window.emit(
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
            // Attempt deploy through embedded metadata first, then fall back
            // to filename-based legacy install when the file lacks that
            // metadata but still maps cleanly to a known game package.
            let deploy = try_deploy_gpk(&entry.id, &entry.name, &dest, Some(&entry.download_url));

            let final_row = mods_state::mutate(|reg| {
                let slot = reg.find_mut(&entry.id).ok_or_else(|| {
                    format!("Registry entry for {} disappeared mid-install", entry.id)
                })?;
                if deploy.blocks_enable {
                    finalize_blocked_gpk_slot(
                        slot,
                        &entry.version,
                        deploy
                            .last_error
                            .clone()
                            .unwrap_or_else(|| gpk_unsupported_diff_fallback(&entry.name)),
                    );
                } else {
                    finalize_installed_slot(
                        slot,
                        &entry.version,
                        deploy.last_error,
                        deploy.deployed_filename,
                    );
                }
                Ok(slot.clone())
            })?;
            let _ = window.emit(
                "mod_download_progress",
                serde_json::json!({ "id": entry.id, "progress": 100, "state": "done" }),
            );
            Ok(final_row)
        }
        Err(err) => finalize_error(&entry.id, err, &window),
    }
}

/// Tries to deploy a downloaded GPK via the patch-based flow:
///   1. Resolve the target package name from the URL hint or GPK header.
///   2. Extract vanilla bytes via `composite_extract` + `.clean` mapper.
///   3. Diff vanilla vs modded → derive + persist a manifest.
///   4. Apply the manifest, write patched bytes to CookedPC, redirect
///      the composite mapper.
///
/// Diff shapes the Phase 1 applier doesn't support (added exports, name
/// or import drift, compressed packages, class changes) are refused at
/// step 3 with a clear error so the user sees the failure mode up front
/// instead of a silently broken client.
fn try_deploy_gpk(
    mod_id: &str,
    _mod_name: &str,
    source_gpk: &std::path::Path,
    download_url: Option<&str>,
) -> GpkDeployOutcome {
    let game_root = match resolve_game_root() {
        Ok(p) => p,
        Err(e) => {
            return GpkDeployOutcome {
                last_error: Some(format!(
                    "Downloaded, but game path isn't set yet — can't deploy. Set the game folder under Settings, then click Retry. ({})",
                    e
                )),
                deployed_filename: None,
                blocks_enable: false,
            };
        }
    };

    let app_root = match patch_manifest::get_manifest_root() {
        Some(p) => p,
        None => {
            return GpkDeployOutcome {
                last_error: Some(
                    "Could not resolve patch-manifest root directory — install aborted".into(),
                ),
                deployed_filename: None,
                blocks_enable: false,
            };
        }
    };

    let target_package_name = match resolve_target_package_name(source_gpk, download_url) {
        Some(name) => name,
        None => {
            return GpkDeployOutcome {
                    last_error: Some(
                        "Mod's target package name is not derivable from URL or GPK header — install aborted".into(),
                    ),
                    deployed_filename: None,
                    blocks_enable: false,
                };
        }
    };

    let outcome = match gpk_patch_deploy::install_via_patch(
        &game_root,
        &app_root,
        mod_id,
        source_gpk,
        &target_package_name,
    ) {
        Ok(o) => o,
        Err(err) => {
            return GpkDeployOutcome {
                last_error: Some(format!(
                    "Mod can't be installed via the patch-based deploy: {err}"
                )),
                deployed_filename: None,
                blocks_enable: true,
            };
        }
    };

    if let Err(err) = gpk_patch_deploy::enable_via_patch(&game_root, &app_root, mod_id) {
        // Roll the manifest back so disk and registry stay consistent.
        let _ = gpk_patch_deploy::uninstall_via_patch(&game_root, &app_root, mod_id);
        return GpkDeployOutcome {
            last_error: Some(format!("Patch applied OK but enable failed: {err}")),
            deployed_filename: None,
            blocks_enable: true,
        };
    }

    log::info!(
        "patch-based install succeeded for {} as {}",
        source_gpk.display(),
        outcome.target_filename
    );
    GpkDeployOutcome {
        last_error: None,
        deployed_filename: Some(outcome.target_filename),
        blocks_enable: false,
    }
}

/// Resolves the inner package name (e.g. `"S1UI_ProgressBar"`) the mod
/// targets. Catalog download URLs end in `<package>.gpk`; the URL hint is
/// the most reliable source for catalog mods whose on-disk filename is an
/// opaque catalog id. Falls back to the UE3 header's PackageName when no
/// usable URL hint is available (e.g. `add_mod_from_file`).
fn resolve_target_package_name(
    source_gpk: &std::path::Path,
    download_url: Option<&str>,
) -> Option<String> {
    let url_hint = download_url
        .and_then(|url| url::Url::parse(url).ok())
        .and_then(|u| u.path_segments().and_then(|s| s.last()).map(str::to_string));

    if let Some(hint) = url_hint {
        let stripped = hint.strip_suffix(".gpk").unwrap_or(&hint);
        let trimmed = stripped.trim();
        if !trimmed.is_empty() && !trimmed.eq_ignore_ascii_case("none") {
            return Some(trimmed.to_string());
        }
    }

    if let Ok(bytes) = std::fs::read(source_gpk) {
        if let Some(folder) = gpk::extract_package_folder_name(&bytes) {
            let trimmed = folder.trim();
            if !trimmed.is_empty() && !trimmed.eq_ignore_ascii_case("none") {
                return Some(trimmed.to_string());
            }
        }
    }
    None
}

/// Reads the game root from the launcher's config.ini via the existing
/// config command helpers. Returned path is the TERA install folder (the
/// parent of S1Game), matching what tmm.rs expects.
///
/// `services::config_service::parse_game_config` stores the install root
/// directly (e.g. `C:/Games/TERA`) — NOT a path to `TERA.exe`. Every
/// other caller (commands/download.rs, commands/hash.rs) treats
/// `game_path` that way. A previous version of this function stripped
/// two `parent()` levels assuming an exe-path shape and mis-resolved
/// every valid install, failing with "Configured game path has no
/// parent root" even when the user had set a correct path. See
/// fix.resolve-game-root-wrong-assumption (iter 83) for the bug history.
fn resolve_game_root() -> Result<std::path::PathBuf, String> {
    let (game_path, _lang) = crate::commands::config::load_config()?;
    validate_game_root(game_path)
}

/// Pure predicate split out for testability. `game_root` is the
/// configured install path as stored in config.ini. Returns Ok iff the
/// path has an `S1Game` child directory.
fn validate_game_root(game_root: std::path::PathBuf) -> Result<std::path::PathBuf, String> {
    if !game_root.join("S1Game").exists() {
        return Err(format!(
            "No S1Game folder under {} — path may be wrong",
            game_root.display()
        ));
    }
    Ok(game_root)
}

fn finalize_error(id: &str, err: String, window: &tauri::Window) -> Result<ModEntry, String> {
    let _ = mods_state::mutate(|reg| {
        if let Some(slot) = reg.find_mut(id) {
            slot.status = ModStatus::Error;
            slot.progress = None;
            slot.last_error = Some(err.clone());
        }
        Ok(())
    });
    let _ = window.emit(
        "mod_download_progress",
        serde_json::json!({ "id": id, "progress": 0, "state": "error", "error": err }),
    );
    Err(err)
}

/// PRD 3.3.4.add-mod-from-file-wire: user picks a local `.gpk`; we parse it,
/// compute its sha256, copy it into `mods/gpk/<id>.gpk`, attempt the GPK
/// mapper patch (best-effort), and upsert into the registry. Returns the
/// new registry entry.
///
/// The `id` is `local.<sha12>` so the same bytes always produce the same id
/// (re-importing the same file is idempotent — registry upsert handles it).
#[tauri::command]
#[cfg(not(tarpaulin_include))]
pub async fn add_mod_from_file(path: String) -> Result<ModEntry, String> {
    use sha2::{Digest, Sha256};

    let src = PathBuf::from(&path);
    let bytes =
        std::fs::read(&src).map_err(|e| format!("Failed to read {}: {e}", src.display()))?;

    let modfile = gpk::parse_mod_file(&bytes)?;
    let fallback_display_name = gpk::resolve_legacy_target_filename(&bytes, &src, None)
        .and_then(|name| name.strip_suffix(".gpk").map(str::to_string));
    if modfile.container.is_empty() && fallback_display_name.is_none() {
        return Err(
            "Imported file has no deployable override metadata and no usable target filename."
                .into(),
        );
    }
    if let Some(target_filename) = gpk::resolve_legacy_target_filename(&bytes, &src, None) {
        if !gpk::is_safe_gpk_container_filename(&target_filename) {
            return Err(format!(
                "Imported .gpk would deploy to an unsafe filename '{}' — refusing to import.",
                target_filename
            ));
        }
    } else if !gpk::is_safe_gpk_container_filename(&modfile.container) {
        return Err(format!(
            "Imported .gpk has an unsafe container filename '{}' — refusing to import.",
            modfile.container
        ));
    }

    let sha = {
        let digest = Sha256::digest(&bytes);
        let mut hex = String::with_capacity(64);
        for b in digest {
            hex.push_str(&format!("{b:02x}"));
        }
        hex
    };

    let mut entry = ModEntry::from_local_gpk(&sha, &modfile, fallback_display_name.as_deref());

    // Copy into our gpk slot so uninstall can find it.
    let gpk_dir = get_gpk_dir().ok_or_else(|| "Could not resolve GPK mods dir".to_string())?;
    std::fs::create_dir_all(&gpk_dir)
        .map_err(|e| format!("Failed to create {}: {e}", gpk_dir.display()))?;
    let dest = gpk_dir.join(format!("{}.gpk", entry.id.replace('/', "_")));
    std::fs::write(&dest, &bytes)
        .map_err(|e| format!("Failed to copy to {}: {e}", dest.display()))?;

    // Best-effort mapper deploy. If the game root isn't configured we still
    // persist the import so the user can see it; the deploy happens next
    // time they hit enable.
    //
    // Pass the original source filename as a synthetic URL hint so
    // resolve_target_package_name can derive the target package
    // (e.g. "S1UI_ProgressBar") when the GPK header has folderName="None"
    // (the v100.02 vanilla convention) and we've already copied the file
    // away from its original name.
    let url_hint = src
        .file_name()
        .and_then(|n| n.to_str())
        .map(|n| format!("file:///{n}"));
    let deploy = try_deploy_gpk(&entry.id, &entry.name, &dest, url_hint.as_deref());
    entry.deployed_filename = deploy.deployed_filename.clone();
    entry.last_error = deploy.last_error.clone();
    if deploy.blocks_enable {
        entry.enabled = false;
        entry.auto_launch = false;
        entry.status = ModStatus::Error;
    } else if deploy.last_error.is_none() {
        entry.enabled = true;
        entry.auto_launch = true;
        entry.status = ModStatus::Enabled;
    } else {
        entry.enabled = false;
        entry.auto_launch = false;
        entry.status = ModStatus::Error;
    }

    mods_state::mutate(|reg| {
        reg.upsert(entry.clone());
        Ok(())
    })?;

    let _ = rebuild_gpk_runtime_state();

    info!(
        "add_mod_from_file: imported {} (sha={}) status={:?}",
        entry.id,
        &sha[..12],
        entry.status
    );
    Ok(entry)
}

/// Uninstalls a mod: stops process if running, removes files, removes from registry.
/// For external apps with a configured settings folder, the frontend handles
/// the "also delete settings?" prompt and passes `delete_settings`.
#[tauri::command]
#[cfg(not(tarpaulin_include))]
pub async fn uninstall_mod(id: String, delete_settings: Option<bool>) -> Result<(), String> {
    let entry =
        mods_state::get_mod(&id)?.ok_or_else(|| format!("Mod '{}' is not installed", id))?;

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
            let gpk_dir =
                get_gpk_dir().ok_or_else(|| "Could not resolve GPK mods dir".to_string())?;
            let file = gpk_dir.join(format!("{}.gpk", entry.id.replace('/', "_")));
            let _ = delete_settings; // GPK has no per-mod settings folder

            mods_state::mutate(|reg| {
                reg.remove(&id);
                Ok(())
            })?;

            let rebuild_result = rebuild_gpk_runtime_state();

            if file.exists() {
                std::fs::remove_file(&file)
                    .map_err(|e| format!("Failed to remove {}: {}", file.display(), e))?;
            }

            rebuild_result?;
            return Ok(());
        }
    }

    mods_state::mutate(|reg| {
        reg.remove(&id);
        Ok(())
    })?;
    Ok(())
}

/// Applies the "enable" intent to a registry slot: flips the intent flags
/// on and updates the display status. Pure over `&mut ModEntry` — cannot
/// spawn a process or touch the filesystem. PRD 3.3.15: toggle is intent
/// only; actual spawn happens at Launch Game via
/// `spawn_auto_launch_external_apps`.
fn apply_enable_intent(slot: &mut ModEntry) {
    slot.enabled = true;
    slot.auto_launch = true;
    slot.status = ModStatus::Enabled;
    slot.last_error = None;
}

/// Applies the "disable" intent to a registry slot. PRD 3.3.15: toggle is
/// intent only — a mod whose process is still alive keeps running; use
/// `stop_external_app` if you want to terminate it. The status flip to
/// Disabled is a display label, not a process action.
fn apply_disable_intent(slot: &mut ModEntry) {
    slot.enabled = false;
    slot.auto_launch = false;
    slot.status = ModStatus::Disabled;
}

fn noctenium_tcc_maps_dir(game_root: &std::path::Path) -> std::path::PathBuf {
    game_root
        .join("Binaries")
        .join("noctenium")
        .join("interop")
        .join("tcc")
        .join("opcodes")
}

fn external_launch_args(id: &str) -> Result<Vec<String>, String> {
    match id {
        "classicplus.tcc" | "tera-europe-classic.tcc" => {
            let game_root = resolve_game_root()?;
            let maps_dir = noctenium_tcc_maps_dir(&game_root);
            Ok(vec![
                "--toolbox".to_string(),
                "--map_export_dir".to_string(),
                maps_dir.to_string_lossy().into_owned(),
            ])
        }
        "classicplus.shinra" | "tera-europe-classic.shinra" => Ok(vec!["--toolbox".to_string()]),
        _ => Ok(Vec::new()),
    }
}

/// Enables a mod. The toggle records intent only — it does NOT start the
/// external app. Enabled external apps auto-spawn when the user clicks
/// Launch Game (see `spawn_auto_launch_external_apps`). Enabled GPKs are
/// applied at game launch by the mapper patcher.
#[tauri::command]
#[cfg(not(tarpaulin_include))]
pub async fn enable_mod(id: String) -> Result<ModEntry, String> {
    let entry =
        mods_state::get_mod(&id)?.ok_or_else(|| format!("Mod '{}' is not installed", id))?;

    let updated = mods_state::mutate(|reg| {
        let slot = reg
            .find_mut(&id)
            .ok_or_else(|| format!("Mod '{}' is not installed", id))?;
        apply_enable_intent(slot);
        Ok(slot.clone())
    })?;

    if matches!(entry.kind, ModKind::Gpk) {
        rebuild_gpk_runtime_state()?;
    }
    Ok(updated)
}

/// Disables a mod — flips the intent flags off. External apps already
/// running are left alone; close them from their own window if you want
/// them gone now. (The explicit `stop_external_app` command is still
/// available for UI controls that need to terminate a live process.)
#[tauri::command]
#[cfg(not(tarpaulin_include))]
pub async fn disable_mod(id: String) -> Result<ModEntry, String> {
    let entry =
        mods_state::get_mod(&id)?.ok_or_else(|| format!("Mod '{}' is not installed", id))?;

    let updated = mods_state::mutate(|reg| {
        let slot = reg
            .find_mut(&id)
            .ok_or_else(|| format!("Mod '{}' is not installed", id))?;
        apply_disable_intent(slot);
        Ok(slot.clone())
    })?;

    if matches!(entry.kind, ModKind::Gpk) {
        rebuild_gpk_runtime_state()?;
    }
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
    let entry = mods_state::get_mod(id)?.ok_or_else(|| format!("Mod '{}' is not installed", id))?;

    let exe_name = external_executable_name(id)
        .ok_or_else(|| format!("Cannot resolve executable name for {}", id))?;

    // Attach-once semantics (PRD 3.2.11): if the process is already running
    // we skip the spawn so a 2nd TERA.exe launch doesn't duplicate Shinra/TCC.
    if external_app::check_spawn_decision(&exe_name) == external_app::SpawnDecision::Spawn {
        let install_root = get_external_apps_dir()
            .ok_or_else(|| "Could not resolve external apps dir".to_string())?;
        let dest = install_root.join(&entry.id);
        let exe_path = external_app::executable_path(&dest, &exe_name)?;
        let args = external_launch_args(id)?;
        external_app::spawn_app(&exe_path, &args)?;
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

/// Previews (composite, object)-slot collisions between the incoming mod
/// and any other GPK mod already patched into `CompositePackageMapper.dat`.
/// Returns one `ModConflict` per dirty slot; an empty vec means "safe to
/// install, no last-install-wins overwrite."
///
/// The frontend calls this before `install_mod`; on a non-empty result it
/// renders the last-install-wins disclaimer modal (existing Playwright
/// spec `mod-conflict-warning.spec.js`).
///
/// Scope: only GPK mods can produce conflicts. External-app entries are
/// out-of-band and return `Ok([])`. If the catalog GPK hasn't been
/// downloaded yet (no file under `mods/gpk/<id>.gpk`), also returns
/// `Ok([])` — the real install path will still run `detect_conflicts`
/// again post-download, so this is a best-effort UX preview, not a
/// safety gate.
///
/// Wiring for fix.conflict-modal-wiring (PRD §3.3.3).
#[tauri::command]
#[cfg(not(tarpaulin_include))]
pub async fn preview_mod_install_conflicts(
    entry: CatalogEntry,
) -> Result<Vec<ModConflict>, String> {
    if !matches!(entry.kind, ModKind::Gpk) {
        return Ok(Vec::new());
    }

    let gpk_dir = get_gpk_dir().ok_or_else(|| "Could not resolve GPK mods dir".to_string())?;
    let file_name = format!("{}.gpk", entry.id.replace('/', "_"));
    let source_gpk = gpk_dir.join(&file_name);
    if !source_gpk.exists() {
        return Ok(Vec::new());
    }

    let bytes = std::fs::read(&source_gpk)
        .map_err(|e| format!("Failed to read {}: {e}", source_gpk.display()))?;
    let game_root = resolve_game_root()?;
    gpk::preview_conflicts_from_bytes(&game_root, &bytes)
}

/// User-invoked recovery path for a missing `CompositePackageMapper.clean`
/// backup. Resolves the configured game root, then defers to
/// `gpk::recover_missing_clean` which:
///   - no-ops when `.clean` already exists,
///   - copies the current (vanilla) mapper to `.clean` when safe,
///   - refuses with a `verify-game-files` instruction when the current
///     mapper is already TMM-modded (capturing modded bytes as the
///     "vanilla" baseline would silently break uninstall forever).
///
/// Wiring for fix.clean-recovery-wiring (PRD §3.3 reliability). The
/// frontend calls this when the user clicks the Settings-panel Recovery
/// button; the button is typically shown after mapper/backup errors
/// surfaced in the Troubleshoot entries (§7-8 in TROUBLESHOOT.md).
#[tauri::command]
#[cfg(not(tarpaulin_include))]
pub async fn recover_clean_mapper() -> Result<(), String> {
    let game_root = resolve_game_root()?;
    gpk::recover_missing_clean(&game_root)
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
        std::fs::create_dir_all(&dir).map_err(|e| format!("Failed to create mods dir: {}", e))?;
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
        // Attach-once: skip spawn when an instance is already running so a
        // 2nd TERA.exe auto-launch doesn't duplicate Shinra/TCC (PRD 3.2.11).
        if external_app::check_spawn_decision(&exe_name) == external_app::SpawnDecision::Attach {
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
        let args = match external_launch_args(&entry.id) {
            Ok(v) => v,
            Err(e) => {
                log::warn!(
                    "Auto-launch: could not resolve args for {}: {}",
                    entry.id,
                    e
                );
                continue;
            }
        };
        match external_app::spawn_app(&exe_path, &args) {
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

/// Called when the game client closes: terminates every installed
/// external mod whose process is still alive, so Shinra/TCC don't
/// linger after the game is gone. Runs regardless of the current
/// enabled flag — a user who untoggled mid-session still expects the
/// overlay to exit with the client.
///
/// Best-effort — logs failures, never propagates.
pub fn stop_auto_launched_external_apps() {
    let entries = match mods_state::list_mods() {
        Ok(v) => v,
        Err(e) => {
            log::warn!("Auto-stop: could not read mods registry: {}", e);
            return;
        }
    };
    for entry in entries {
        if !matches!(entry.kind, ModKind::External) {
            continue;
        }
        let exe_name = match external_executable_name(&entry.id) {
            Some(n) => n,
            None => continue,
        };
        if !external_app::is_process_running(&exe_name) {
            continue;
        }
        match external_app::stop_process_by_name(&exe_name) {
            Ok(_) => {
                log::info!("Auto-stop: terminated {}", entry.id);
                let _ = mods_state::mutate(|reg| {
                    if let Some(slot) = reg.find_mut(&entry.id) {
                        // Keep the toggle state as-is; just clear the
                        // "Running" live-status overlay.
                        slot.status = if slot.enabled {
                            ModStatus::Enabled
                        } else {
                            ModStatus::Disabled
                        };
                    }
                    Ok(())
                });
            }
            Err(e) => log::warn!("Auto-stop: failed to stop {}: {}", entry.id, e),
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

#[cfg(test)]
mod tests {
    use super::*;

    fn disabled_slot() -> ModEntry {
        ModEntry {
            id: "classicplus.shinra".into(),
            kind: ModKind::External,
            name: "Shinra".into(),
            author: "Foglio1024".into(),
            description: "DPS meter".into(),
            version: "0.0.0".into(),
            status: ModStatus::Installing,
            source_url: None,
            deployed_filename: None,
            icon_url: None,
            progress: Some(42),
            last_error: Some("stale error from previous attempt".into()),
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
        }
    }

    /// PRD 3.3.12.fresh-install-defaults: finalising a slot after a clean
    /// install flips it to enabled + auto_launch with Enabled status. Pins
    /// all six fields — the whole contract lives in one helper, so any drift
    /// shows up here.
    #[test]
    fn fresh_install_defaults_enabled() {
        let mut slot = disabled_slot();
        finalize_installed_slot(&mut slot, "1.2.3", None, None);

        assert!(slot.enabled, "fresh install must be enabled by default");
        assert!(
            slot.auto_launch,
            "fresh install must auto-launch by default"
        );
        assert!(matches!(slot.status, ModStatus::Enabled));
        assert_eq!(slot.progress, None, "progress must be cleared on finalize");
        assert_eq!(
            slot.last_error, None,
            "last_error clears when no deploy note"
        );
        assert_eq!(slot.version, "1.2.3");
    }

    /// GPK path passes a deploy note through as `last_error` (non-fatal:
    /// bytes landed but mapper patch was skipped). Finalize must preserve it.
    #[test]
    fn fresh_install_preserves_deploy_note() {
        let mut slot = disabled_slot();
        let note = Some("mapper not patched: backup missing".to_string());
        finalize_installed_slot(&mut slot, "2.0.0", note.clone(), None);

        assert!(slot.enabled);
        assert!(slot.auto_launch);
        assert!(matches!(slot.status, ModStatus::Enabled));
        assert_eq!(slot.progress, None);
        assert_eq!(slot.last_error, note);
        assert_eq!(slot.version, "2.0.0");
    }

    #[test]
    fn external_update_preflight_stops_known_running_apps() {
        let mut stopped = Vec::new();

        stop_external_app_before_update("classicplus.shinra", |exe| {
            stopped.push(exe.to_string());
            Ok(1)
        });
        stop_external_app_before_update("tera-europe-classic.tcc", |exe| {
            stopped.push(exe.to_string());
            Ok(1)
        });
        stop_external_app_before_update("classicplus.unknown", |exe| {
            stopped.push(exe.to_string());
            Ok(1)
        });

        assert_eq!(stopped, vec!["ShinraMeter.exe", "TCC.exe"]);
    }

    #[test]
    fn external_update_preflight_does_not_block_update_when_stop_fails() {
        stop_external_app_before_update("classicplus.shinra", |_| Err("access denied".into()));
    }

    /// Re-installing over an existing slot must re-enable it: a user who
    /// previously untoggled the mod and then clicks Install again expects
    /// the same fresh-install behaviour, not their old disabled state.
    #[test]
    fn reinstall_reenables_previously_disabled_slot() {
        let mut slot = disabled_slot();
        slot.status = ModStatus::Disabled;
        slot.version = "0.9.0".into();

        finalize_installed_slot(&mut slot, "1.0.0", None, None);

        assert!(slot.enabled);
        assert!(slot.auto_launch);
        assert!(matches!(slot.status, ModStatus::Enabled));
        assert_eq!(slot.version, "1.0.0");
    }

    /// PRD 3.3.15.toggle-intent-only: the enable toggle must only mutate
    /// the intent flags on the slot. The helper takes `&mut ModEntry` and
    /// nothing else — it structurally cannot spawn a process or touch the
    /// filesystem. This test pins the flag-level contract.
    #[test]
    fn toggle_intent_only() {
        let mut slot = disabled_slot();
        slot.status = ModStatus::Disabled;
        slot.last_error = Some("previous crash reason".into());

        apply_enable_intent(&mut slot);

        assert!(slot.enabled, "enable flips enabled=true");
        assert!(slot.auto_launch, "enable flips auto_launch=true");
        assert!(matches!(slot.status, ModStatus::Enabled));
        assert_eq!(
            slot.last_error, None,
            "enable clears stale last_error so the UI doesn't re-surface it"
        );
    }

    /// Disable toggle is also intent-only. `&mut ModEntry` signature proves
    /// no process side effects; this test pins the flag flip contract.
    #[test]
    fn toggle_disable_intent_only() {
        let mut slot = disabled_slot();
        apply_enable_intent(&mut slot);
        assert!(slot.enabled);

        apply_disable_intent(&mut slot);

        assert!(!slot.enabled, "disable flips enabled=false");
        assert!(!slot.auto_launch, "disable flips auto_launch=false");
        assert!(matches!(slot.status, ModStatus::Disabled));
    }

    /// A running external app stays alive when the user untoggles the
    /// enable switch. The intent helper is not responsible for killing
    /// the process — `stop_external_app` is the explicit kill path. The
    /// `&mut ModEntry` signature is the proof: the helper has no access
    /// to process state, so it structurally cannot terminate anything.
    #[test]
    fn disable_while_running_does_not_kill() {
        let mut slot = disabled_slot();
        slot.enabled = true;
        slot.auto_launch = true;
        slot.status = ModStatus::Running;

        apply_disable_intent(&mut slot);

        assert!(!slot.enabled);
        assert!(!slot.auto_launch);
        // The status flip to Disabled is a display label, not a process
        // action — the actual child process is unaffected (if it was
        // running, it continues to run). This test documents that
        // invariant at the type level: the helper cannot kill because it
        // only sees `&mut ModEntry`.
        assert!(matches!(slot.status, ModStatus::Disabled));
    }

    /// Source-inspection guard. The enable/disable toggle bodies must not
    /// reference process-level operations. If someone adds
    /// `external_app::spawn_app(...)` or `stop_process_by_name(...)` to
    /// `enable_mod` or `disable_mod` (the `#[cfg(not(tarpaulin_include))]`
    /// Tauri commands that can't be unit-tested directly), this grep-style
    /// assertion fails.
    ///
    /// PRD 3.3.15 says toggles are intent only. The pure helpers above
    /// cannot spawn/kill by their signature, but the Tauri command bodies
    /// could — that's what this test watches.
    #[test]
    fn toggle_command_bodies_do_not_spawn_or_kill() {
        let source = include_str!("mods.rs");

        let enable_start = source
            .find("pub async fn enable_mod")
            .expect("enable_mod present");
        let enable_body = &source[enable_start..];
        let enable_body = &enable_body[..enable_body
            .find("\npub async fn disable_mod")
            .expect("disable_mod follows")];
        assert!(
            !enable_body.contains("spawn_app"),
            "enable_mod must not spawn — PRD 3.3.15"
        );
        assert!(
            !enable_body.contains("stop_process_by_name"),
            "enable_mod must not kill — PRD 3.3.15"
        );

        let disable_start = source
            .find("pub async fn disable_mod")
            .expect("disable_mod present");
        let disable_body = &source[disable_start..];
        let disable_body = &disable_body[..disable_body
            .find("\n/// Ad-hoc launch of an external app")
            .expect("launch_external_app follows")];
        assert!(
            !disable_body.contains("spawn_app"),
            "disable_mod must not spawn — PRD 3.3.15"
        );
        assert!(
            !disable_body.contains("stop_process_by_name"),
            "disable_mod must not kill — PRD 3.3.15"
        );
    }

    #[test]
    fn gpk_unsupported_diff_fallback_message_calls_out_phase1_limits() {
        let note = gpk_unsupported_diff_fallback("UI Remover: Flight Gauge");
        assert!(note.contains("UI Remover: Flight Gauge"));
        assert!(note.contains("patch-based"));
        assert!(
            note.contains("added exports")
                && note.contains("name/import drift")
                && note.contains("compressed packages")
                && note.contains("class changes"),
            "fallback must enumerate the diff shapes the Phase 1 applier refuses so the user knows what's unsupported"
        );
    }

    #[test]
    fn try_deploy_gpk_routes_through_patch_based_install() {
        let source = include_str!("mods.rs");
        let fn_start = source
            .find("fn try_deploy_gpk")
            .expect("try_deploy_gpk present");
        // Scope the assertion window to just the body of try_deploy_gpk
        // (up to the next top-level fn) so it doesn't get confused by tests
        // or comments that mention legacy symbols by name.
        let fn_tail = &source[fn_start..];
        let body_end = fn_tail[1..]
            .find("\nfn ")
            .map(|i| i + 1)
            .unwrap_or(fn_tail.len());
        let body = &fn_tail[..body_end];

        let install_idx = body
            .find("gpk_patch_deploy::install_via_patch")
            .expect("try_deploy_gpk must call install_via_patch");
        let enable_idx = body.find("gpk_patch_deploy::enable_via_patch").expect(
            "try_deploy_gpk must enable after install so the patched bytes land in CookedPC",
        );
        let rollback_idx = body
            .find("gpk_patch_deploy::uninstall_via_patch")
            .expect("try_deploy_gpk must roll back the manifest when enable fails so disk and registry stay consistent");

        assert!(
            install_idx < enable_idx,
            "install must run before enable in try_deploy_gpk"
        );
        assert!(
            enable_idx < rollback_idx,
            "uninstall fallback must come after the enable attempt"
        );
        assert!(
            !body.contains("install_legacy_gpk") && !body.contains("gpk::install_gpk("),
            "try_deploy_gpk must not call the legacy whole-file install paths"
        );
    }

    #[test]
    fn both_gpk_install_entry_points_flow_through_shared_deploy_gate() {
        let source = include_str!("mods.rs");
        assert!(
            source.contains("let deploy = try_deploy_gpk(&entry.id, &entry.name, &dest, Some(&entry.download_url));"),
            "install_gpk_mod must route through try_deploy_gpk with id + display name so manifest gating is centralized"
        );
        assert!(
            source.contains("let deploy = try_deploy_gpk(&entry.id, &entry.name, &dest, None);"),
            "add_mod_from_file must route through the same shared deploy gate so local imports cannot bypass manifest checks"
        );
    }

    #[test]
    fn blocked_gpk_deploy_does_not_mark_slot_enabled() {
        let mut slot = disabled_slot();
        slot.kind = ModKind::Gpk;
        slot.status = ModStatus::Installing;

        finalize_blocked_gpk_slot(
            &mut slot,
            "1.0.0",
            "patch-based deploy refused: unsupported diff shape".to_string(),
        );

        assert!(!slot.enabled);
        assert!(!slot.auto_launch);
        assert!(matches!(slot.status, ModStatus::Error));
        assert_eq!(slot.version, "1.0.0");
        assert_eq!(slot.deployed_filename, None);
        assert!(slot
            .last_error
            .unwrap_or_default()
            .contains("patch-based deploy refused"));
    }

    #[test]
    fn rebuild_gpk_runtime_state_uses_per_mod_disable_then_enable() {
        let source = include_str!("mods.rs");
        let fn_start = source
            .find("fn rebuild_gpk_runtime_state")
            .expect("rebuild_gpk_runtime_state present");
        let fn_body = &source[fn_start..];

        // Function body ends at the first top-level `fn` after our function start.
        let body_end = fn_body[1..]
            .find("\nfn ")
            .or_else(|| fn_body[1..].find("\npub fn "))
            .map(|i| i + 1)
            .unwrap_or(fn_body.len());
        let body = &fn_body[..body_end];

        assert!(
            body.contains("restore_clean_mapper_state"),
            "rebuild must hard-restore the mapper from .clean before per-mod operations"
        );
        let disable_idx = body
            .find("gpk_patch_deploy::disable_via_patch")
            .expect("rebuild must per-mod disable before enabling");
        let enable_idx = body
            .find("gpk_patch_deploy::enable_via_patch")
            .expect("rebuild must per-mod enable after disabling");
        assert!(
            disable_idx < enable_idx,
            "per-mod disable must run before per-mod enable in rebuild"
        );
        assert!(
            !body.contains("rebuild_gpk_state(&game_root"),
            "rebuild_gpk_runtime_state must not call the legacy rebuild_gpk_state"
        );
    }

    #[test]
    fn add_mod_from_file_respects_blocks_enable_explicitly() {
        let source = include_str!("mods.rs");
        let fn_start = source
            .find("pub async fn add_mod_from_file")
            .expect("add_mod_from_file present");
        let fn_body = &source[fn_start..];
        let blocks_idx = fn_body
            .find("if deploy.blocks_enable {")
            .expect("add_mod_from_file must branch on blocks_enable");
        let legacy_idx = fn_body
            .find("else if deploy.last_error.is_none() {")
            .expect("add_mod_from_file must still handle legacy non-blocking success");
        assert!(
            blocks_idx < legacy_idx,
            "local file import must treat blocks_enable as an explicit stronger gate than last_error-derived success"
        );
    }

    /// fix.resolve-game-root-wrong-assumption (iter 83).
    ///
    /// `validate_game_root` must treat the stored game path AS the install
    /// root — not as a path to `TERA.exe`. An earlier version of
    /// `resolve_game_root()` stripped two `parent()` levels assuming an
    /// exe-path shape, which turned valid configs like `C:/Games/TERA`
    /// into `C:/` and then failed the `S1Game` check. That blocked every
    /// GPK install for users with a normal install layout, surfacing as
    /// the error "Configured game path has no parent root".
    #[test]
    fn validate_game_root_accepts_install_root_with_s1game() {
        let tmp = tempfile::TempDir::new().unwrap();
        let install = tmp.path().to_path_buf();
        std::fs::create_dir(install.join("S1Game")).unwrap();

        let out = validate_game_root(install.clone()).expect("valid install");
        assert_eq!(
            out, install,
            "validate_game_root must return the stored path unchanged \
             (NOT a parent) when S1Game exists underneath"
        );
    }

    /// Negative: a path without an S1Game subdirectory is rejected with
    /// a clear "path may be wrong" message — the only legitimate reason
    /// `validate_game_root` should error.
    #[test]
    fn validate_game_root_rejects_missing_s1game() {
        let tmp = tempfile::TempDir::new().unwrap();
        let err =
            validate_game_root(tmp.path().to_path_buf()).expect_err("missing S1Game must err");
        assert!(
            err.contains("No S1Game folder under"),
            "error must name S1Game as the missing folder, got: {err}"
        );
    }

    /// Regression guard: `validate_game_root` must not call `.parent()` at
    /// all. The old bug was a `.parent().and_then(|p| p.parent())` chain
    /// that silently truncated valid paths. Source-inspection keeps the
    /// fix honest — if a refactor brings parent-walking back (even in a
    /// different shape like `strip_prefix` or `components().take(n)`), the
    /// next person to touch this code has to explicitly justify it.
    #[test]
    fn validate_game_root_source_has_no_parent_walk() {
        let src = std::fs::read_to_string("src/commands/mods.rs").expect("mods.rs must exist");
        let fn_pos = src
            .find("fn validate_game_root")
            .expect("validate_game_root must exist");
        let fn_end = src[fn_pos..]
            .find("\n}")
            .map(|p| fn_pos + p)
            .expect("function body must close");
        let body = &src[fn_pos..fn_end];
        assert!(
            !body.contains(".parent()"),
            "validate_game_root must not call .parent() — see iter-83 bug \
             history. game_path from config_service IS the install root."
        );
    }

    // --- auto-update on launch / mod-manager open --------------------------

    fn enabled_slot(id: &str, version: &str, kind: ModKind) -> ModEntry {
        let mut slot = disabled_slot();
        slot.id = id.into();
        slot.kind = kind;
        slot.version = version.into();
        slot.enabled = true;
        slot.status = ModStatus::Enabled;
        slot.last_error = None;
        slot.progress = None;
        slot
    }

    fn catalog_entry(id: &str, version: &str, kind: ModKind) -> CatalogEntry {
        CatalogEntry {
            id: id.into(),
            kind,
            name: id.into(),
            author: "test".into(),
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
            version: version.into(),
            download_url: format!("https://example.com/{id}.zip"),
            sha256: "deadbeef".into(),
            size_bytes: 0,
            source_url: None,
            icon_url: None,
            screenshots: vec![],
            executable_relpath: Some(format!("{id}.exe")),
            auto_launch_default: None,
            settings_folder: None,
            target_patch: None,
            composite_flag: None,
            compatible_arch: None,
            updated_at: "".into(),
        }
    }

    /// Strict string equality is the contract. Author bumps the version
    /// label, comparator triggers — no semver math, no parsing.
    #[test]
    fn versions_differ_basic_cases() {
        assert!(versions_differ("3.0.8-classicplus", "3.0.9-classicplus"));
        assert!(!versions_differ("1.0.0", "1.0.0"));
    }

    /// Empty/whitespace strings are not actionable — refuse to flag an
    /// update so we don't reinstall a mod whose registry version got
    /// truncated by a bad write.
    #[test]
    fn versions_differ_skips_empty_or_whitespace() {
        assert!(!versions_differ("", "1.0.0"));
        assert!(!versions_differ("1.0.0", ""));
        assert!(!versions_differ("   ", "1.0.0"));
        assert!(!versions_differ("1.0.0", "   "));
    }

    /// Mismatched whitespace shouldn't cause a phantom update — trim
    /// before compare.
    #[test]
    fn versions_differ_ignores_surrounding_whitespace() {
        assert!(!versions_differ(" 1.0.0 ", "1.0.0"));
    }

    /// Happy path: a single enabled mod with a newer catalog version
    /// surfaces as one update entry that carries the kind through.
    #[test]
    fn detect_updates_flags_enabled_mod_with_newer_catalog() {
        let installed = vec![enabled_slot(
            "classicplus.shinra",
            "3.0.8-classicplus",
            ModKind::External,
        )];
        let catalog = vec![catalog_entry(
            "classicplus.shinra",
            "3.0.9-classicplus",
            ModKind::External,
        )];

        let updates = detect_updates(&installed, &catalog);
        assert_eq!(updates.len(), 1);
        assert_eq!(updates[0].id, "classicplus.shinra");
        assert_eq!(updates[0].current_version, "3.0.8-classicplus");
        assert_eq!(updates[0].available_version, "3.0.9-classicplus");
        assert_eq!(updates[0].kind, ModKind::External);
        assert!(updates[0].enabled);
    }

    /// Same version → no update. Pins the catalog-only-flips-on-bump
    /// contract that drives the "I just updated, don't redownload"
    /// behaviour.
    #[test]
    fn detect_updates_skips_same_version() {
        let installed = vec![enabled_slot("a", "1.0.0", ModKind::External)];
        let catalog = vec![catalog_entry("a", "1.0.0", ModKind::External)];
        assert!(detect_updates(&installed, &catalog).is_empty());
    }

    /// A disabled mod with an out-of-date version still appears in the
    /// diff list — the LIST is "everything that has an update". The
    /// `enabled` flag is carried through so the auto-update path can
    /// filter it out without re-querying the registry.
    #[test]
    fn detect_updates_includes_disabled_mods_in_diff_list() {
        let mut installed = enabled_slot("a", "1.0.0", ModKind::External);
        installed.enabled = false;
        installed.status = ModStatus::Disabled;
        let installed = vec![installed];
        let catalog = vec![catalog_entry("a", "2.0.0", ModKind::External)];

        let updates = detect_updates(&installed, &catalog);
        assert_eq!(updates.len(), 1);
        assert!(!updates[0].enabled);
    }

    /// A mod present in the registry but not the catalog is left alone
    /// — it might be a local import or a removed catalog entry. We
    /// can't tell from here, and quietly ignoring is the safest action.
    #[test]
    fn detect_updates_ignores_mods_missing_from_catalog() {
        let installed = vec![enabled_slot("local.aa", "1.0.0", ModKind::Gpk)];
        let catalog: Vec<CatalogEntry> = vec![];
        assert!(detect_updates(&installed, &catalog).is_empty());
    }

    /// New catalog entry that the user hasn't installed yet is not an
    /// "update" — only installed mods generate update rows.
    #[test]
    fn detect_updates_ignores_uninstalled_catalog_entries() {
        let installed: Vec<ModEntry> = vec![];
        let catalog = vec![catalog_entry("a", "1.0.0", ModKind::External)];
        assert!(detect_updates(&installed, &catalog).is_empty());
    }

    /// A version with no version string in the registry (legacy
    /// records) doesn't trigger an update — versions_differ refuses
    /// empty values, so the slot is left to be reconciled by an
    /// explicit reinstall instead of an auto-clobber.
    #[test]
    fn detect_updates_skips_blank_installed_version() {
        let mut row = enabled_slot("a", "", ModKind::External);
        row.version = "".into();
        let installed = vec![row];
        let catalog = vec![catalog_entry("a", "1.0.0", ModKind::External)];
        assert!(detect_updates(&installed, &catalog).is_empty());
    }

    /// Mid-flight statuses keep their real-time display — flipping
    /// them to UpdateAvailable would visually halt a live download.
    #[test]
    fn flip_to_update_available_skips_midflight_statuses() {
        assert!(!should_flip_to_update_available(ModStatus::Installing));
        assert!(!should_flip_to_update_available(ModStatus::Running));
        assert!(!should_flip_to_update_available(ModStatus::Starting));
        // Error rows hold the user's last failure message; clobbering
        // them with UpdateAvailable hides the troubleshooting context.
        assert!(!should_flip_to_update_available(ModStatus::Error));
    }

    /// Stable rest-states are safe to flip — Disabled, Enabled,
    /// NotInstalled (browse rows whose registry slot was upserted),
    /// and the idempotent UpdateAvailable case.
    #[test]
    fn flip_to_update_available_allowed_for_stable_states() {
        assert!(should_flip_to_update_available(ModStatus::Disabled));
        assert!(should_flip_to_update_available(ModStatus::Enabled));
        assert!(should_flip_to_update_available(ModStatus::NotInstalled));
        assert!(should_flip_to_update_available(ModStatus::UpdateAvailable));
    }

    /// AutoUpdateResult round-trips through serde_json — the frontend
    /// reads `attempted` and `failed_ids` by name so a rename or
    /// alias-strip would be a contract break.
    #[test]
    fn auto_update_result_serializes_with_expected_field_names() {
        let r = AutoUpdateResult {
            attempted: vec!["a".into(), "b".into()],
            failed_ids: vec!["b".into()],
        };
        let json = serde_json::to_string(&r).unwrap();
        assert!(json.contains("\"attempted\""));
        assert!(json.contains("\"failed_ids\""));
        let back: AutoUpdateResult = serde_json::from_str(&json).unwrap();
        assert_eq!(back, r);
    }

    /// ModUpdateInfo round-trips with the keys the frontend reads.
    /// Keeps the cross-language contract pinned.
    #[test]
    fn mod_update_info_serializes_with_expected_field_names() {
        let info = ModUpdateInfo {
            id: "a".into(),
            kind: ModKind::External,
            current_version: "1.0".into(),
            available_version: "2.0".into(),
            enabled: true,
        };
        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains("\"id\""));
        assert!(json.contains("\"kind\""));
        assert!(json.contains("\"current_version\""));
        assert!(json.contains("\"available_version\""));
        assert!(json.contains("\"enabled\""));
    }
}
