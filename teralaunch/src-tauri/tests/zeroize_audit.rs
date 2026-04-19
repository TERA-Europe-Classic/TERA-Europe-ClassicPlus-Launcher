//! PRD 3.1.7.zeroize-audit — integration-level pin on the zeroize invariants
//! we rely on in the bin crate. Because `tera-europe-classicplus-launcher` has
//! no lib target, integration tests can't import `GlobalAuthInfo` or
//! `LaunchParams` directly; the in-crate `#[cfg(test)]` modules own the
//! type-specific assertions (see `src/domain/models.rs::tests` and
//! `src/services/game_service.rs::tests`). This file pins the third-party
//! crate behaviours those derives depend on so a `zeroize` bump can't silently
//! change the drop semantics for our secrets.

use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};

#[test]
fn string_zeroize_truncates_and_overwrites() {
    let mut s = String::from("super-secret-auth-key-value");
    let orig = s.clone();
    s.zeroize();
    assert!(s.is_empty(), "String::zeroize must leave the string empty");
    assert_ne!(s, orig);
}

#[test]
fn zeroizing_string_wraps_transparently() {
    // Zeroizing<String> derefs to String, so call sites that only need &str
    // keep working (e.g. `serde_json::json!({ "password": password.as_str() })`
    // in commands::auth::login_with_client).
    let z = Zeroizing::new(String::from("s3cret"));
    assert_eq!(z.as_str(), "s3cret");
    // Explicitly observe the derefed behaviour — if zeroize ever removes the
    // Deref impl this test won't compile.
    let as_str_ref: &str = &z;
    assert_eq!(as_str_ref, "s3cret");
    // Drop runs zeroize; we can't safely observe the buffer post-drop (may
    // be reclaimed), so that invariant is covered by in-crate tests.
}

#[test]
fn zeroize_derives_compose_with_skip_attribute() {
    // Shape-mirror of GlobalAuthInfo / LaunchParams — verifies the derive-plus
    // -skip pattern those types use is still supported by the zeroize crate.
    #[derive(Default, Zeroize, ZeroizeOnDrop)]
    struct StandIn {
        #[zeroize(skip)]
        non_sensitive: String,
        #[zeroize(skip)]
        counter: i32,
        sensitive: String,
    }

    fn assert_zod<T: ZeroizeOnDrop>() {}
    assert_zod::<StandIn>();

    let mut s = StandIn {
        non_sensitive: "user@example.com".to_string(),
        counter: 7,
        sensitive: "real-secret".to_string(),
    };
    s.zeroize();
    assert!(s.sensitive.is_empty(), "derived zeroize must clear unskipped fields");
    // Skipped fields preserved.
    assert_eq!(s.non_sensitive, "user@example.com");
    assert_eq!(s.counter, 7);
}

#[test]
fn integer_zeroize_resets_to_zero() {
    // i32 implements Zeroize via the primitive blanket — included here so a
    // downgrade of the zeroize crate that drops primitive impls is caught.
    let mut n: i32 = 42;
    n.zeroize();
    assert_eq!(n, 0);
}
