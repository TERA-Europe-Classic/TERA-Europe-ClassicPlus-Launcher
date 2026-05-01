//! Resolves the vanilla bytes for a target GPK package across the two
//! shapes catalog mods actually use:
//!
//! - **Composite-routed** (Type A): vanilla bytes live inside a composite
//!   container at a `(filename, offset, size)` recorded in
//!   `CompositePackageMapper.clean`.
//! - **Standalone-file** (Type B): vanilla `<package>.gpk` is a real file
//!   somewhere under `S1Game/CookedPC/`. Found via filesystem walk.
//!
//! Resolution priority is composite-first, standalone-second — composite
//! routing takes precedence in TERA's loader, so if both happen to exist
//! we pick composite to match what the engine actually loads.

use std::fs;
use std::path::{Path, PathBuf};

use super::composite_extract;
use super::gpk::COOKED_PC_DIR;
use super::gpk_package;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VanillaSource {
    /// Vanilla bytes were extracted from a composite container at a
    /// mapper-resolved offset/size. Apply path is "write standalone +
    /// redirect mapper".
    Composite,
    /// Vanilla bytes were read from a standalone `.gpk` at a filesystem
    /// path under CookedPC. Apply path is "in-place edit at this path,
    /// with a `.vanilla-bak` backup".
    Standalone {
        /// Absolute path to the vanilla file the patch should be applied to.
        path: PathBuf,
    },
}

#[derive(Debug, Clone)]
pub struct VanillaResolution {
    pub source: VanillaSource,
    pub bytes: Vec<u8>,
}

/// Resolve a *single* composite slice by full logical path
/// (e.g. `"S1UI_PaperDoll.PaperDoll"`). Used when the catalog entry
/// provides a `target_object_path` qualifier — required for multi-object
/// widget packages where `resolve_vanilla_for_package_name` errors with
/// "maps to multiple vanilla composite byte ranges".
pub fn resolve_vanilla_for_logical_path(
    game_root: &Path,
    logical_path: &str,
) -> Result<VanillaResolution, String> {
    let raw_bytes =
        composite_extract::extract_vanilla_for_logical_path(game_root, logical_path)?;
    let bytes = gpk_package::extract_uncompressed_package_bytes(&raw_bytes).map_err(|e| {
        format!(
            "Failed to decompress composite-resolved vanilla for logical path '{logical_path}': {e}"
        )
    })?;
    Ok(VanillaResolution {
        source: VanillaSource::Composite,
        bytes,
    })
}

pub fn resolve_vanilla_for_package_name(
    game_root: &Path,
    package_name: &str,
) -> Result<VanillaResolution, String> {
    // 1. Composite first — matches engine's lookup priority.
    match composite_extract::extract_vanilla_for_package_name(game_root, package_name) {
        Ok(raw_bytes) => {
            // The slice carved out of the composite container preserves
            // whatever compression the package was cooked with. Phase 1
            // patch derivation + applier require uncompressed input, so
            // normalize here before returning.
            let bytes =
                gpk_package::extract_uncompressed_package_bytes(&raw_bytes).map_err(|e| {
                    format!(
                        "Failed to decompress composite-resolved vanilla for '{package_name}': {e}"
                    )
                })?;
            return Ok(VanillaResolution {
                source: VanillaSource::Composite,
                bytes,
            });
        }
        Err(err) if err.contains("not present") || err.contains("not found") => {
            // Fall through to standalone search.
        }
        Err(other) => {
            // Unexpected error (mapper missing, container missing). Return
            // it directly — it's a real install-blocking problem the user
            // needs to see, not a "package not in mapper" signal.
            return Err(other);
        }
    }

    // 2. Filesystem walk under CookedPC for `<package_name>.gpk`.
    let cooked_pc = game_root.join(COOKED_PC_DIR);
    if !cooked_pc.exists() {
        return Err(format!(
            "CookedPC directory not found at {} — cannot resolve vanilla file for '{}'",
            cooked_pc.display(),
            package_name
        ));
    }

    let target_filename = format!("{package_name}.gpk");
    let standalone_path = find_standalone_vanilla(&cooked_pc, &target_filename)?
        .ok_or_else(|| {
            format!(
                "Vanilla '{}' not found under {} — neither composite mapper nor filesystem has a baseline for this mod. \
                 The mod may be a Type D \"new package\" mod (no vanilla baseline exists for it).",
                target_filename,
                cooked_pc.display()
            )
        })?;

    let raw_bytes = fs::read(&standalone_path).map_err(|e| {
        format!(
            "Failed to read vanilla standalone file {}: {e}",
            standalone_path.display()
        )
    })?;
    let bytes = gpk_package::extract_uncompressed_package_bytes(&raw_bytes).map_err(|e| {
        format!(
            "Failed to decompress standalone vanilla {}: {e}",
            standalone_path.display()
        )
    })?;

    Ok(VanillaResolution {
        source: VanillaSource::Standalone {
            path: standalone_path,
        },
        bytes,
    })
}

/// Walks `cooked_pc` recursively for a file matching `target_filename`
/// (case-insensitive on Windows; exact on POSIX since TERA assets are
/// authored on Windows). Skips any directory whose final segment is
/// prefixed `_` — that's the legacy "override folder" convention and a
/// patched mod we previously deployed could live there; we want the
/// *vanilla* baseline, not a previously-patched copy.
///
/// Returns `Ok(Some(path))` on first match, `Ok(None)` if nothing matched,
/// `Err` on filesystem error.
fn find_standalone_vanilla(
    cooked_pc: &Path,
    target_filename: &str,
) -> Result<Option<PathBuf>, String> {
    let target_lower = target_filename.to_ascii_lowercase();
    let walker = walkdir::WalkDir::new(cooked_pc)
        .follow_links(false)
        .max_depth(12)
        .into_iter()
        .filter_entry(|entry| {
            // Skip override folders (prefix `_`). Their contents are mods,
            // not vanilla. Always traverse the cooked_pc root.
            if entry.depth() == 0 {
                return true;
            }
            if entry.file_type().is_dir() {
                let leaf = entry.file_name().to_str().unwrap_or_default();
                return !leaf.starts_with('_');
            }
            true
        });

    for entry in walker {
        let entry = match entry {
            Ok(e) => e,
            Err(err) => {
                // A single unreadable directory shouldn't tank the walk.
                log::warn!("vanilla_resolver: walkdir error: {err}");
                continue;
            }
        };
        if !entry.file_type().is_file() {
            continue;
        }
        let name = match entry.file_name().to_str() {
            Some(s) => s,
            None => continue,
        };
        if name.to_ascii_lowercase() == target_lower {
            return Ok(Some(entry.path().to_path_buf()));
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::mods::gpk::{encrypt_mapper, BACKUP_FILE};
    use crate::services::mods::test_fixtures::build_boss_window_test_package;
    use tempfile::TempDir;

    #[test]
    fn resolves_composite_when_mapper_has_entry() {
        let tmp = TempDir::new().unwrap();
        let game_root = tmp.path();
        let cooked_pc = game_root.join(COOKED_PC_DIR);
        fs::create_dir_all(&cooked_pc).unwrap();

        let vanilla_pkg = build_boss_window_test_package([0x10, 0x11, 0x12, 0x13], false);
        fs::write(cooked_pc.join("S1Common.gpk"), &vanilla_pkg).unwrap();

        let mapper_text = format!(
            "S1Common.gpk?Owner.S1UI_GageBoss,Comp,0,{},|!",
            vanilla_pkg.len()
        );
        fs::write(
            cooked_pc.join(BACKUP_FILE),
            encrypt_mapper(mapper_text.as_bytes()),
        )
        .unwrap();

        let resolution = resolve_vanilla_for_package_name(game_root, "S1UI_GageBoss").unwrap();
        assert_eq!(resolution.source, VanillaSource::Composite);
        assert_eq!(resolution.bytes, vanilla_pkg);
    }

    #[test]
    fn falls_back_to_standalone_when_mapper_has_no_entry() {
        let tmp = TempDir::new().unwrap();
        let game_root = tmp.path();
        let cooked_pc = game_root.join(COOKED_PC_DIR);
        let deep_dir = cooked_pc.join("Art_Data").join("Packages").join("S1UI");
        fs::create_dir_all(&deep_dir).unwrap();

        // Empty mapper backup — nothing matches in composite.
        fs::write(cooked_pc.join(BACKUP_FILE), encrypt_mapper(b"")).unwrap();

        let vanilla_pkg = build_boss_window_test_package([0xAA, 0xBB, 0xCC, 0xDD], false);
        let vanilla_path = deep_dir.join("S1UI_GageBoss.gpk");
        fs::write(&vanilla_path, &vanilla_pkg).unwrap();

        let resolution = resolve_vanilla_for_package_name(game_root, "S1UI_GageBoss").unwrap();
        match resolution.source {
            VanillaSource::Standalone { path } => {
                // Compare canonicalized to avoid temp-dir quirks
                assert_eq!(
                    fs::canonicalize(path).unwrap(),
                    fs::canonicalize(&vanilla_path).unwrap()
                );
            }
            other => panic!("expected standalone, got {other:?}"),
        }
        assert_eq!(resolution.bytes, vanilla_pkg);
    }

    #[test]
    fn standalone_walk_skips_underscore_override_folders() {
        let tmp = TempDir::new().unwrap();
        let game_root = tmp.path();
        let cooked_pc = game_root.join(COOKED_PC_DIR);
        let s1ui = cooked_pc.join("Art_Data").join("Packages").join("S1UI");
        fs::create_dir_all(&s1ui).unwrap();

        // Mapper backup with no matches.
        fs::write(cooked_pc.join(BACKUP_FILE), encrypt_mapper(b"")).unwrap();

        // A previously-deployed patched copy in `_mods` should be skipped.
        let mods_dir = s1ui.join("_mods");
        fs::create_dir_all(&mods_dir).unwrap();
        let modded = build_boss_window_test_package([0xFF, 0xFF, 0xFF, 0xFF], false);
        fs::write(mods_dir.join("S1UI_GageBoss.gpk"), &modded).unwrap();

        // The actual vanilla in the canonical location.
        let vanilla = build_boss_window_test_package([0x10, 0x10, 0x10, 0x10], false);
        let vanilla_path = s1ui.join("S1UI_GageBoss.gpk");
        fs::write(&vanilla_path, &vanilla).unwrap();

        let resolution = resolve_vanilla_for_package_name(game_root, "S1UI_GageBoss").unwrap();
        assert_eq!(
            resolution.bytes, vanilla,
            "must pick vanilla, not the mod in _mods"
        );
    }

    #[test]
    fn errors_when_neither_composite_nor_standalone_has_baseline() {
        let tmp = TempDir::new().unwrap();
        let game_root = tmp.path();
        let cooked_pc = game_root.join(COOKED_PC_DIR);
        fs::create_dir_all(&cooked_pc).unwrap();
        fs::write(cooked_pc.join(BACKUP_FILE), encrypt_mapper(b"")).unwrap();

        let err = resolve_vanilla_for_package_name(game_root, "Nonexistent_Pkg").unwrap_err();
        assert!(
            err.contains("not found under") || err.contains("Type D"),
            "got: {err}"
        );
    }

    #[test]
    fn standalone_walk_is_case_insensitive() {
        let tmp = TempDir::new().unwrap();
        let game_root = tmp.path();
        let cooked_pc = game_root.join(COOKED_PC_DIR);
        let dir = cooked_pc.join("Sub");
        fs::create_dir_all(&dir).unwrap();
        fs::write(cooked_pc.join(BACKUP_FILE), encrypt_mapper(b"")).unwrap();

        let pkg = build_boss_window_test_package([0x01, 0x02, 0x03, 0x04], false);
        // Filename uses different case than what we'll search for.
        fs::write(dir.join("s1ui_gageboss.GPK"), &pkg).unwrap();

        let resolution = resolve_vanilla_for_package_name(game_root, "S1UI_GageBoss").unwrap();
        assert_eq!(resolution.bytes, pkg);
    }
}
