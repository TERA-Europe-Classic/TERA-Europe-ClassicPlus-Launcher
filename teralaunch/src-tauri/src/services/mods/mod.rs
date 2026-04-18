//! Mod manager services.
//!
//! Handles the two mod types supported by the launcher:
//!
//! - **External app mods** — separate executables (Shinra Meter, TCC) launched
//!   alongside the game. See [`external_app`].
//! - **GPK mods** — TMM-compatible game-asset packs patched into
//!   `CookedPC/CompositePackageMapper.dat`. Not yet implemented (Phase C).
//!
//! Cross-cutting pieces live here too: the remote catalog fetch
//! ([`catalog`]) and the on-disk registry of installed mods ([`registry`]).

pub mod catalog;
pub mod external_app;
pub mod registry;
pub mod types;
