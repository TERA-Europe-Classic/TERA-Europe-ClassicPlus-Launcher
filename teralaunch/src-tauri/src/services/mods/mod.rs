//! Mod manager services.
//!
//! Handles the two mod types supported by the launcher:
//!
//! - **External app mods** — separate executables (Shinra Meter, TCC) launched
//!   alongside the game. See [`external_app`].
//! - **GPK mods** — launcher-managed game-asset packs patched into
//!   `CookedPC/CompositePackageMapper.dat`.
//!
//! Cross-cutting pieces live here too: the remote catalog fetch
//! ([`catalog`]) and the on-disk registry of installed mods ([`registry`]).

pub mod catalog;
#[cfg(test)]
pub mod catalog_audit;
pub mod composite_author;
pub mod composite_extract;
pub mod dds;
pub mod external_app;
pub mod gpk;
pub mod gpk_package;
pub mod gpk_patch_applier;
pub mod gpk_patch_deploy;
pub mod gpk_resource_inspector;
pub mod manifest_store;
pub mod mapper_extend;
pub mod patch_derivation;
pub mod patch_manifest;
pub mod registry;
pub mod texture_encoder;
pub mod tmm_wrap;
pub mod types;
pub mod vanilla_resolver;

#[cfg(test)]
pub mod test_fixtures;
