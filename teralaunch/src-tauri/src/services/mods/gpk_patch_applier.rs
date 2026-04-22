//! Placeholder seam for launcher-side curated patch application.
//!
//! The actual write path is intentionally not implemented yet. This module
//! exists so converter/package-analysis work can grow toward a dedicated applier
//! instead of being mixed into the legacy TMM-footer deployer.

use super::patch_manifest::PatchManifest;

pub fn apply_manifest(_package_bytes: &[u8], _manifest: &PatchManifest) -> Result<Vec<u8>, String> {
    Err("Launcher-side curated GPK patch application is not implemented yet".into())
}
