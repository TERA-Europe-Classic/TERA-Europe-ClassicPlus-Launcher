//! Infrastructure layer providing abstractions for external dependencies.
//!
//! This module contains traits and implementations that abstract over:
//! - HTTP client operations (reqwest)
//! - Filesystem operations (std::fs)
//! - Hash caching
//! - Event emission (Tauri events)
//!
//! By using traits for these operations, the application code can be tested
//! with mock implementations, enabling comprehensive unit testing without
//! requiring actual network access, filesystem operations, or a Tauri runtime.

// Allow dead_code and unused_imports for now - these traits will be used in future refactoring phases
#![allow(dead_code)]
#![allow(unused_imports)]

pub mod cache;
pub mod events;
pub mod filesystem;
pub mod http;

// Re-export commonly used types
pub use cache::{HashCache, InMemoryHashCache};
pub use events::{
    EventEmitter, MockEventEmitter, RecordedEvent, TauriAppEmitter, TauriWindowEmitter,
};
#[cfg(test)]
pub use filesystem::MockFileSystem;
pub use filesystem::{FileMetadata, FileSystem, StdFileSystem};
#[cfg(test)]
pub use http::MockHttpClient;
pub use http::{HttpClient, HttpResponse, ReqwestClient};
