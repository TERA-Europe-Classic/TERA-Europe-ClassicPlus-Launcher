//! Add new logical→composite and composite→file rows to live mapper files.
//!
//! Used when installing a mod that introduces *new* resources (e.g. a custom
//! paperdoll that ships in its own composite GPK) rather than overriding rows
//! that already exist in the vanilla mappers.
//!
//! Preserves `.clean` files as the vanilla baseline used by the rollback path
//! (`gpk::restore_clean_mapper_state`). Only the live `.dat` files are touched.
//! Idempotent: re-running with the same `MapperAddition` set is a no-op.

use std::path::Path;

use super::gpk;

#[derive(Debug, Clone)]
pub struct MapperAddition {
    /// Logical path the game looks up, e.g. `S1UIRES_Skin.PaperDoll_AM`.
    pub logical_path: String,
    /// Composite UID (group key in CompositePackageMapper), e.g. `modres_skin_001`.
    pub composite_uid: String,
    /// Composite object path, e.g. `modres_skin_001.PaperDoll_AM_dup`.
    pub composite_object_path: String,
    /// Container filename (no `.gpk` extension), e.g. `modres_paperdoll_skin_001`.
    pub composite_filename: String,
    /// Byte offset of the slice inside the composite GPK.
    pub composite_offset: i64,
    /// Byte length of the slice inside the composite GPK.
    pub composite_size: i64,
}

pub fn extend_mappers(game_root: &Path, additions: &[MapperAddition]) -> Result<(), String> {
    let cooked = game_root.join(gpk::COOKED_PC_DIR);
    if !cooked.is_dir() {
        return Err(format!(
            "CookedPC dir does not exist: {}",
            cooked.display()
        ));
    }

    // PkgMapper: REPLACE any existing row with the same logical_path, then append ours.
    // TERA's engine uses first-match resolution; an appended override is silently
    // shadowed by an existing vanilla row. Removing the conflicting row first
    // lets our override take effect.
    let pm_path = cooked.join(gpk::PKG_MAPPER_FILE);
    let pm_enc = std::fs::read(&pm_path).map_err(|e| format!("read PkgMapper: {e}"))?;
    let pm_text = String::from_utf8_lossy(&gpk::decrypt_mapper(&pm_enc)).to_string();
    let mut pm_dirty = false;
    let mut rows: Vec<String> = pm_text
        .split('|')
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect();
    for add in additions {
        let key_prefix = format!("{},", add.logical_path);
        let new_row = format!("{},{}", add.logical_path, add.composite_object_path);
        let existed_with_same_target = rows.iter().any(|r| *r == new_row);
        let any_with_same_key = rows.iter().any(|r| r.starts_with(&key_prefix));
        if existed_with_same_target {
            // Already present and pointing where we want; ensure no other row with
            // the same key shadows it (defensive).
            let before = rows.len();
            rows.retain(|r| *r == new_row || !r.starts_with(&key_prefix));
            if rows.len() != before {
                pm_dirty = true;
            }
            continue;
        }
        if any_with_same_key {
            rows.retain(|r| !r.starts_with(&key_prefix));
            pm_dirty = true;
        }
        rows.push(new_row);
        pm_dirty = true;
    }
    if pm_dirty {
        let mut new_text = String::with_capacity(pm_text.len() + 256);
        for r in &rows {
            new_text.push_str(r);
            new_text.push('|');
        }
        let pm_new = gpk::encrypt_mapper(new_text.as_bytes());
        gpk::write_atomic_file(&pm_path, &pm_new)?;
    }

    // CompositePackageMapper: ADD a new row group only when our composite_uid
    // is not already present. We DO NOT do REPLACE-by-uid here.
    //
    // Why: an earlier attempt to REPLACE existing vanilla rows by composite_uid
    // (so that a vanilla uid would resolve to our modded file at offset 0)
    // caused the v100 engine to fatal-error at startup. The cause is not yet
    // fully understood but is likely tied to the engine's bulk-loading
    // assumptions about which slices live in which containers — moving a uid
    // out of its vanilla container appears to break a startup invariant.
    //
    // The known-working pattern (used by silhouettes) is to allocate a FRESH
    // composite_uid that vanilla doesn't know about, route the logical name
    // to that fresh uid via PkgMapper REPLACE, and add a new
    // CompositePackageMapper row for it. That keeps every vanilla composite
    // entry untouched in CompositePackageMapper.
    //
    // Append-if-row-text-not-already-present is the original behavior we
    // restore here.
    let cm_path = cooked.join(gpk::MAPPER_FILE);
    let cm_enc = std::fs::read(&cm_path).map_err(|e| format!("read CompositeMapper: {e}"))?;
    let mut cm_text = String::from_utf8_lossy(&gpk::decrypt_mapper(&cm_enc)).to_string();
    let mut cm_dirty = false;
    for add in additions {
        let row = format!(
            "{}?{},{},{},{},|!",
            add.composite_filename,
            add.composite_object_path,
            add.composite_uid,
            add.composite_offset,
            add.composite_size
        );
        if !cm_text.contains(&row) {
            cm_text.push_str(&row);
            cm_dirty = true;
        }
    }
    if cm_dirty {
        let cm_new = gpk::encrypt_mapper(cm_text.as_bytes());
        gpk::write_atomic_file(&cm_path, &cm_new)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::gpk;
    use super::*;

    #[test]
    fn extends_pkg_and_composite_mappers_atomically() {
        let tmp = tempfile::TempDir::new().unwrap();
        let cooked = tmp.path().join("S1Game/CookedPC");
        std::fs::create_dir_all(&cooked).unwrap();

        // Seed minimal mapper files (encrypted via gpk::encrypt_mapper).
        let pkg_text = "S1UI_X.X,modres_baseline_0.X_dup|";
        let comp_text = "modres_baseline_0?modres_baseline_0.X_dup,modres_baseline_0,0,100,|!";
        std::fs::write(
            cooked.join("PkgMapper.dat"),
            gpk::encrypt_mapper(pkg_text.as_bytes()),
        )
        .unwrap();
        std::fs::write(
            cooked.join("PkgMapper.clean"),
            gpk::encrypt_mapper(pkg_text.as_bytes()),
        )
        .unwrap();
        std::fs::write(
            cooked.join("CompositePackageMapper.dat"),
            gpk::encrypt_mapper(comp_text.as_bytes()),
        )
        .unwrap();
        std::fs::write(
            cooked.join("CompositePackageMapper.clean"),
            gpk::encrypt_mapper(comp_text.as_bytes()),
        )
        .unwrap();

        let new_rows = vec![MapperAddition {
            logical_path: "S1UIRES_Skin.PaperDoll_AM".into(),
            composite_uid: "modres_skin_001".into(),
            composite_object_path: "modres_skin_001.PaperDoll_AM_dup".into(),
            composite_filename: "modres_paperdoll_skin_001".into(),
            composite_offset: 0,
            composite_size: 524441,
        }];

        extend_mappers(tmp.path(), &new_rows).unwrap();

        // PkgMapper.dat now contains the new row.
        let pm = std::fs::read(cooked.join("PkgMapper.dat")).unwrap();
        let pm_text = String::from_utf8_lossy(&gpk::decrypt_mapper(&pm)).to_string();
        assert!(
            pm_text.contains("S1UIRES_Skin.PaperDoll_AM,modres_skin_001.PaperDoll_AM_dup"),
            "PkgMapper plaintext is: {pm_text}"
        );

        // CompositePackageMapper.dat has the new row keyed by filename.
        let cm = std::fs::read(cooked.join("CompositePackageMapper.dat")).unwrap();
        let cm_text = String::from_utf8_lossy(&gpk::decrypt_mapper(&cm)).to_string();
        assert!(
            cm_text.contains(
                "modres_paperdoll_skin_001?modres_skin_001.PaperDoll_AM_dup,modres_skin_001,0,524441"
            ),
            "CompositeMapper plaintext is: {cm_text}"
        );

        // .clean files unchanged (preserved as vanilla baseline for rollback).
        let pmc = std::fs::read(cooked.join("PkgMapper.clean")).unwrap();
        let pmc_text = String::from_utf8_lossy(&gpk::decrypt_mapper(&pmc)).to_string();
        assert!(
            !pmc_text.contains("PaperDoll_AM"),
            "PkgMapper.clean must NOT contain new row: {pmc_text}"
        );
        let cmc = std::fs::read(cooked.join("CompositePackageMapper.clean")).unwrap();
        let cmc_text = String::from_utf8_lossy(&gpk::decrypt_mapper(&cmc)).to_string();
        assert!(
            !cmc_text.contains("PaperDoll_AM"),
            "CompositeMapper.clean must NOT contain new row: {cmc_text}"
        );
    }

    #[test]
    fn idempotent_when_row_already_present() {
        let tmp = tempfile::TempDir::new().unwrap();
        let cooked = tmp.path().join("S1Game/CookedPC");
        std::fs::create_dir_all(&cooked).unwrap();
        let pkg_text = "S1UI_X.X,modres_baseline_0.X_dup|S1UIRES_Skin.PaperDoll_AM,modres_skin_001.PaperDoll_AM_dup|";
        let comp_text = "modres_baseline_0?modres_baseline_0.X_dup,modres_baseline_0,0,100,|!modres_paperdoll_skin_001?modres_skin_001.PaperDoll_AM_dup,modres_skin_001,0,524441,|!";
        std::fs::write(
            cooked.join("PkgMapper.dat"),
            gpk::encrypt_mapper(pkg_text.as_bytes()),
        )
        .unwrap();
        std::fs::write(
            cooked.join("CompositePackageMapper.dat"),
            gpk::encrypt_mapper(comp_text.as_bytes()),
        )
        .unwrap();
        std::fs::write(
            cooked.join("PkgMapper.clean"),
            gpk::encrypt_mapper(pkg_text.as_bytes()),
        )
        .unwrap();
        std::fs::write(
            cooked.join("CompositePackageMapper.clean"),
            gpk::encrypt_mapper(comp_text.as_bytes()),
        )
        .unwrap();

        let row = MapperAddition {
            logical_path: "S1UIRES_Skin.PaperDoll_AM".into(),
            composite_uid: "modres_skin_001".into(),
            composite_object_path: "modres_skin_001.PaperDoll_AM_dup".into(),
            composite_filename: "modres_paperdoll_skin_001".into(),
            composite_offset: 0,
            composite_size: 524441,
        };
        extend_mappers(tmp.path(), &[row]).unwrap();

        // Should not duplicate the row. Match the full unique row body so we
        // count the row itself (not the substring `PaperDoll_AM`, which the
        // seed contains twice — once in the logical path, once in the
        // composite object path).
        let pm = std::fs::read(cooked.join("PkgMapper.dat")).unwrap();
        let pm_text = String::from_utf8_lossy(&gpk::decrypt_mapper(&pm)).to_string();
        let unique_row = "S1UIRES_Skin.PaperDoll_AM,modres_skin_001.PaperDoll_AM_dup|";
        assert_eq!(
            pm_text.matches(unique_row).count(),
            1,
            "row appeared {}x: {}",
            pm_text.matches(unique_row).count(),
            pm_text
        );

        let cm = std::fs::read(cooked.join("CompositePackageMapper.dat")).unwrap();
        let cm_text = String::from_utf8_lossy(&gpk::decrypt_mapper(&cm)).to_string();
        let unique_cm_row =
            "modres_paperdoll_skin_001?modres_skin_001.PaperDoll_AM_dup,modres_skin_001,0,524441,|!";
        assert_eq!(
            cm_text.matches(unique_cm_row).count(),
            1,
            "composite row appeared {}x: {}",
            cm_text.matches(unique_cm_row).count(),
            cm_text
        );
    }

    #[test]
    fn errors_when_cookedpc_missing() {
        let tmp = tempfile::TempDir::new().unwrap();
        let err = extend_mappers(tmp.path(), &[]).unwrap_err();
        assert!(err.contains("CookedPC"), "got: {err}");
    }

    /// Critical regression: TERA's engine uses first-match in PkgMapper. If a
    /// vanilla row with the same logical_path already exists, our APPENDED
    /// override is silently shadowed. extend_mappers must REPLACE such rows.
    #[test]
    fn replaces_existing_pkgmapper_row_with_same_logical_path() {
        let tmp = tempfile::TempDir::new().unwrap();
        let cooked = tmp.path().join("S1Game/CookedPC");
        std::fs::create_dir_all(&cooked).unwrap();
        // Seed a vanilla-shaped row that would shadow our override.
        let pkg_text =
            "S1UI_Other.Other,uid_other.Other_dup|S1UIRES_Component.Component_I35A,vanilla_uid.Component_I35A_dup|";
        let comp_text = "uid_other?uid_other.Other_dup,uid_other,0,100,|!";
        std::fs::write(
            cooked.join("PkgMapper.dat"),
            gpk::encrypt_mapper(pkg_text.as_bytes()),
        )
        .unwrap();
        std::fs::write(
            cooked.join("PkgMapper.clean"),
            gpk::encrypt_mapper(pkg_text.as_bytes()),
        )
        .unwrap();
        std::fs::write(
            cooked.join("CompositePackageMapper.dat"),
            gpk::encrypt_mapper(comp_text.as_bytes()),
        )
        .unwrap();
        std::fs::write(
            cooked.join("CompositePackageMapper.clean"),
            gpk::encrypt_mapper(comp_text.as_bytes()),
        )
        .unwrap();

        extend_mappers(
            tmp.path(),
            &[MapperAddition {
                logical_path: "S1UIRES_Component.Component_I35A".into(),
                composite_uid: "modres_comp_004f".into(),
                composite_object_path: "modres_comp_004f.Component_I35A_dup".into(),
                composite_filename: "modres_paperdoll_comp_004f".into(),
                composite_offset: 0,
                composite_size: 6962,
            }],
        )
        .unwrap();

        let pm = std::fs::read(cooked.join("PkgMapper.dat")).unwrap();
        let pm_text = String::from_utf8_lossy(&gpk::decrypt_mapper(&pm)).to_string();

        // EXACTLY ONE row for the logical path, and it must be ours.
        let occurrences: Vec<&str> = pm_text
            .split('|')
            .filter(|r| r.starts_with("S1UIRES_Component.Component_I35A,"))
            .collect();
        assert_eq!(
            occurrences.len(),
            1,
            "must have exactly 1 row for the logical path; got {occurrences:?}"
        );
        assert_eq!(
            occurrences[0],
            "S1UIRES_Component.Component_I35A,modres_comp_004f.Component_I35A_dup",
            "row must point at our composite, not vanilla"
        );

        // Other rows preserved.
        assert!(pm_text.contains("S1UI_Other.Other,uid_other.Other_dup"));
    }
}
