//! Services layer for the TERA launcher.
//!
//! This module provides pure business logic functions organized by domain:
//!
//! - [`auth_service`]: Authentication and session management
//! - [`config_service`]: Configuration file management
//! - [`download_service`]: Download orchestration and progress tracking
//! - [`game_service`]: Game launching and validation
//! - [`hash_service`]: File hash calculation and verification
//!
//! ## Design Philosophy
//!
//! The services layer contains **pure functions** that are easy to test without
//! mocking external dependencies. Each service focuses on business logic that
//! can be verified with unit tests.
//!
//! For functions that need HTTP/filesystem access, the pattern is:
//! 1. Accept dependencies as parameters (e.g., `impl Read` instead of file path)
//! 2. Return data structures instead of performing side effects
//! 3. Let the caller in main.rs handle the actual I/O
//!
//! ## Future Integration
//!
//! These service functions will gradually replace implementations in main.rs.
//! The migration path:
//! 1. Extract pure logic to services (DONE)
//! 2. Add comprehensive tests (DONE)
//! 3. Update main.rs to use service functions (TODO)

pub mod auth_service;
pub mod config_service;
pub mod download_service;
pub mod game_service;
pub mod hash_service;
pub mod mods;
pub mod self_integrity;
