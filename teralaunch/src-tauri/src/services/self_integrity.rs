//! Self-integrity check for the launcher executable.
//!
//! PRD 3.1.11.self-integrity. On startup the launcher hashes its own `.exe`
//! and compares the digest against a baseline. A mismatch means the binary
//! has been modified since release; the launcher shows a reinstall prompt
//! and refuses to continue.
//!
//! ## Baseline strategy (v1)
//!
//! The baseline hash is loaded from a sidecar file (`self_hash.sha256`)
//! shipped next to the exe by the release pipeline. The sidecar is signed
//! by the updater's minisign key so a local attacker who tampers with the
//! exe cannot trivially also rewrite a matching baseline without the
//! private key. In dev builds the sidecar is absent and the integrity
//! check is skipped (logged at WARN level — never silently).
//!
//! Future hardening (not yet wired):
//! - Embed baseline as a build-time constant (`build.rs` reads a signed
//!   file and injects via `env!`), so tampering requires patching two
//!   places in the shipped binary.
//! - Strict mode: refuse to launch if the sidecar is missing in a release
//!   build. Currently warn-and-continue to avoid breaking dev iteration.
//!
//! ## Why sha256
//!
//! Matches `external_app.rs::download_file`'s hash algo (consistency with
//! catalog entries) and is fast enough for a ~20 MB launcher binary to
//! hash at startup (< 50 ms on any reasonable disk).

use std::fs;
use std::path::Path;

use sha2::{Digest, Sha256};

/// Result of a hash check. Distinguishes "the file matched the baseline",
/// "the file did NOT match" (with diagnostic), and "we couldn't read the
/// file at all" (caller decides: fail-open for dev, fail-closed for
/// production).
#[derive(Debug, PartialEq, Eq)]
pub enum IntegrityResult {
    Match,
    Mismatch {
        actual_sha256: String,
        expected_sha256: String,
    },
    Unreadable(String),
}

impl IntegrityResult {
    #[allow(dead_code)]
    pub fn is_match(&self) -> bool {
        matches!(self, IntegrityResult::Match)
    }

    #[allow(dead_code)]
    pub fn is_mismatch(&self) -> bool {
        matches!(self, IntegrityResult::Mismatch { .. })
    }
}

/// Hashes `path` and compares against `expected_sha256` (hex, case-insensitive).
pub fn verify_file(path: &Path, expected_sha256: &str) -> IntegrityResult {
    let bytes = match fs::read(path) {
        Ok(b) => b,
        Err(e) => {
            return IntegrityResult::Unreadable(format!(
                "failed to read {}: {}",
                path.display(),
                e
            ));
        }
    };
    let actual = hex_lower(&Sha256::digest(&bytes));
    if actual.eq_ignore_ascii_case(expected_sha256) {
        IntegrityResult::Match
    } else {
        IntegrityResult::Mismatch {
            actual_sha256: actual,
            expected_sha256: expected_sha256.to_ascii_lowercase(),
        }
    }
}

/// Hashes `std::env::current_exe()` against `expected_sha256`.
#[allow(dead_code)]
pub fn verify_self(expected_sha256: &str) -> IntegrityResult {
    match std::env::current_exe() {
        Ok(exe) => verify_file(&exe, expected_sha256),
        Err(e) => IntegrityResult::Unreadable(format!("current_exe() failed: {e}")),
    }
}

/// Human-readable prompt shown when the integrity check fails. Used by
/// `main.rs` to display a dialog via `tauri-plugin-dialog` (or `MessageBox`
/// on Windows before Tauri is initialised). Does not include raw hash
/// values — users don't need them and they add social-engineering attack
/// surface.
pub const REINSTALL_PROMPT: &str = "\
The TERA Europe Classic+ launcher failed its integrity check: the \
executable has been modified since release. Running a tampered \
launcher can steal your account credentials. Please reinstall from \
https://web.tera-germany.de/classic/classicplus/ and run the fresh \
copy.";

fn hex_lower(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn make_file_with_contents(contents: &[u8]) -> (NamedTempFile, String) {
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(contents).unwrap();
        f.flush().unwrap();
        let hash = hex_lower(&Sha256::digest(contents));
        (f, hash)
    }

    #[test]
    fn match_when_bytes_equal_expected_hash() {
        let (f, hash) = make_file_with_contents(b"genuine-launcher-bytes");
        assert_eq!(verify_file(f.path(), &hash), IntegrityResult::Match);
    }

    #[test]
    fn mismatch_when_bytes_differ() {
        let (f, _orig_hash) = make_file_with_contents(b"genuine-launcher-bytes");
        // Baseline says something different — simulates a tampered exe
        // being checked against the signed baseline.
        let other_hash = hex_lower(&Sha256::digest(b"tampered-launcher-bytes"));
        let r = verify_file(f.path(), &other_hash);
        assert!(matches!(r, IntegrityResult::Mismatch { .. }), "got {r:?}");
    }

    #[test]
    fn detects_tampered_exe() {
        // Full scenario: we hashed a known-good file and have the baseline.
        // Something modifies the file on disk. Next check must report the
        // modification — and the diagnostic must carry the actual hash so
        // support can investigate.
        let good_contents: &[u8] = b"shipped-launcher-v0.1.12-bytes";
        let (mut f, good_hash) = make_file_with_contents(good_contents);

        // First call: file matches baseline.
        assert_eq!(verify_file(f.path(), &good_hash), IntegrityResult::Match);

        // Tamper: append an attacker payload.
        f.as_file_mut().write_all(b"malicious-appendix").unwrap();
        f.as_file_mut().flush().unwrap();

        let r = verify_file(f.path(), &good_hash);
        match r {
            IntegrityResult::Mismatch {
                actual_sha256,
                expected_sha256,
            } => {
                assert_eq!(expected_sha256, good_hash);
                assert_ne!(actual_sha256, good_hash);
                assert_eq!(actual_sha256.len(), 64); // sha256 hex length
            }
            other => panic!("tampered file must report Mismatch, got {other:?}"),
        }
    }

    #[test]
    fn unreadable_when_file_missing() {
        let r = verify_file(Path::new("/definitely/does/not/exist.bin"), "deadbeef");
        assert!(matches!(r, IntegrityResult::Unreadable(_)), "got {r:?}");
    }

    #[test]
    fn hash_comparison_is_case_insensitive() {
        let (f, hash_lower) = make_file_with_contents(b"case-test");
        let hash_upper = hash_lower.to_ascii_uppercase();
        assert_eq!(
            verify_file(f.path(), &hash_upper),
            IntegrityResult::Match,
            "verify must accept uppercase baseline"
        );
    }

    #[test]
    fn reinstall_prompt_is_user_safe() {
        // No raw hashes (confuses users + gives attackers a copy-paste target
        // for spoofed support messages). Includes the canonical URL and the
        // word "reinstall" so any dialog that shows this gives the user a
        // clear next step.
        assert!(!REINSTALL_PROMPT.contains("sha"));
        assert!(!REINSTALL_PROMPT.contains("SHA"));
        assert!(REINSTALL_PROMPT.contains("reinstall"));
        assert!(REINSTALL_PROMPT.contains("web.tera-germany.de/classic/classicplus"));
    }
}
