//! PRD 3.1.11.self-integrity — integration-level pin.
//!
//! Bin crates can't export modules to integration tests, so the in-module
//! tests under `src/services/self_integrity.rs::tests` own the
//! IntegrityResult-specific assertions. This file pins the external
//! behaviour on the algorithm we depend on (sha256 over arbitrary bytes)
//! so a sha2 crate bump can't break the contract under us.

use std::io::Write;

use sha2::{Digest, Sha256};
use tempfile::NamedTempFile;

fn hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

#[test]
fn detects_tampered_exe() {
    // External-level counterpart to services::self_integrity::tests::detects_tampered_exe.
    // Builds a fake "launcher binary", records its baseline hash, tampers
    // it, and asserts the hash changes. If this test ever passes when it
    // shouldn't (hash unchanged after bytes changed), the integrity check
    // in main.rs is structurally broken regardless of how the in-module
    // tests look.
    let mut f = NamedTempFile::new().unwrap();
    f.write_all(b"release-v0.1.12-launcher.exe").unwrap();
    f.flush().unwrap();

    let baseline = {
        let bytes = std::fs::read(f.path()).unwrap();
        hex(&Sha256::digest(&bytes))
    };
    assert_eq!(baseline.len(), 64, "sha256 hex must be 64 chars");

    // Tamper.
    f.as_file_mut().write_all(b"attacker-appendix").unwrap();
    f.as_file_mut().flush().unwrap();

    let after = {
        let bytes = std::fs::read(f.path()).unwrap();
        hex(&Sha256::digest(&bytes))
    };
    assert_ne!(baseline, after, "tampered file must produce a different hash");
}

#[test]
fn identical_bytes_produce_identical_hash() {
    // Positive control: sha256 is deterministic, so two separate files with
    // the same contents hash to the same value. If this ever breaks, the
    // baseline comparison in self_integrity.rs silently accepts mismatches.
    let a = Sha256::digest(b"the-same-bytes");
    let b = Sha256::digest(b"the-same-bytes");
    assert_eq!(hex(&a), hex(&b));
}
