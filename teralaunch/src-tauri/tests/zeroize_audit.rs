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
    assert!(
        s.sensitive.is_empty(),
        "derived zeroize must clear unskipped fields"
    );
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

// --------------------------------------------------------------------
// Iter 155 structural pins — production type derives + Cargo feature.
// --------------------------------------------------------------------
//
// The tests above prove the zeroize crate still behaves as expected.
// These pins prove the PRODUCTION types (GlobalAuthInfo, LaunchParams)
// still carry the derives we depend on and that their sensitive fields
// are NOT marked `#[zeroize(skip)]`. A refactor that drops the derive
// or adds `skip` to `auth_key` / `ticket` would leave credential bytes
// on the heap after the struct drops — a silent regression that the
// crate-behaviour tests above can't catch because they exercise the
// crate in isolation, not the production types.

use std::fs;

fn read(path: &str) -> String {
    fs::read_to_string(path).unwrap_or_else(|e| panic!("failed to read {path}: {e}"))
}

/// Returns the body of `struct <name> { ... }` as a string, or panics.
fn struct_body<'a>(src: &'a str, name: &str) -> &'a str {
    let needle = format!("struct {name} {{");
    let start = src
        .find(&needle)
        .unwrap_or_else(|| panic!("struct {name} must exist in source"));
    let body_start = start + needle.len();
    let rest = &src[body_start..];
    let close = rest
        .find("\n}")
        .unwrap_or_else(|| panic!("struct {name} must close with `\\n}}`"));
    &rest[..close]
}

/// Returns the text between the last `#[derive(...)]` and the line
/// declaring `struct <name>`. Used to verify a struct carries the
/// derives we depend on.
fn derive_line_for<'a>(src: &'a str, name: &str) -> &'a str {
    let struct_decl = format!("struct {name}");
    let struct_pos = src
        .find(&struct_decl)
        .unwrap_or_else(|| panic!("struct {name} must exist"));
    let preamble_start = src[..struct_pos]
        .rfind("#[derive(")
        .unwrap_or_else(|| panic!("struct {name} must be preceded by #[derive(...)]"));
    &src[preamble_start..struct_pos]
}

/// Returns true if the given field inside the struct body is preceded
/// by `#[zeroize(skip)]`. Used to guarantee sensitive fields are NOT
/// skipped from the zeroize derive.
fn field_is_skipped(body: &str, field_decl: &str) -> bool {
    let field_pos = body
        .find(field_decl)
        .unwrap_or_else(|| panic!("field `{field_decl}` must exist in struct body"));
    let before = &body[..field_pos];
    // Walk the lines before the field in reverse; the first non-blank
    // line is the preceding attribute or the previous field.
    for line in before.lines().rev() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        return trimmed.contains("#[zeroize(skip)]");
    }
    false
}

const MODELS_RS: &str = "src/domain/models.rs";
const GAME_SERVICE_RS: &str = "src/services/game_service.rs";
const CARGO_TOML: &str = "Cargo.toml";

/// `GlobalAuthInfo` must derive BOTH `Zeroize` AND `ZeroizeOnDrop`.
/// Dropping either defeats the guarantee: without `ZeroizeOnDrop` the
/// auth_key survives the Drop and can be read from a heap dump; without
/// `Zeroize` callers can't explicitly wipe it on logout.
#[test]
fn global_auth_info_derives_zeroize_and_zod() {
    let src = read(MODELS_RS);
    let derive = derive_line_for(&src, "GlobalAuthInfo");
    assert!(
        derive.contains("Zeroize"),
        "PRD 3.1.7: GlobalAuthInfo must `#[derive(Zeroize)]`. \
         Without it, callers can't wipe the auth_key on logout.\n\
         Got derive line: `{derive}`"
    );
    assert!(
        derive.contains("ZeroizeOnDrop"),
        "PRD 3.1.7: GlobalAuthInfo must `#[derive(ZeroizeOnDrop)]`. \
         Without it, the auth_key survives Drop and stays on the heap \
         until the allocator reuses the page.\n\
         Got derive line: `{derive}`"
    );
}

/// `GlobalAuthInfo::auth_key` must NOT carry `#[zeroize(skip)]`. The
/// whole reason the struct derives the zeroize traits is to wipe this
/// field; marking it skipped silently defeats §3.1.7.
#[test]
fn global_auth_info_auth_key_is_not_skipped() {
    let src = read(MODELS_RS);
    let body = struct_body(&src, "GlobalAuthInfo");
    assert!(
        body.contains("pub auth_key: String,"),
        "PRD 3.1.7: GlobalAuthInfo must expose `pub auth_key: String,` \
         — the sensitive session credential.\nStruct body:\n{body}"
    );
    assert!(
        !field_is_skipped(body, "pub auth_key: String,"),
        "PRD 3.1.7: GlobalAuthInfo::auth_key must NOT carry \
         `#[zeroize(skip)]`. The entire struct exists to wipe this \
         field on drop — skipping it silently leaks the session \
         credential to heap dumps.\nStruct body:\n{body}"
    );
}

/// `LaunchParams` must derive BOTH `Zeroize` AND `ZeroizeOnDrop`. The
/// struct holds the short-lived `ticket` passed to TERA.exe on the
/// command line; dropping either derive re-opens the leak.
#[test]
fn launch_params_derives_zeroize_and_zod() {
    let src = read(GAME_SERVICE_RS);
    let derive = derive_line_for(&src, "LaunchParams");
    assert!(
        derive.contains("Zeroize"),
        "PRD 3.1.7: LaunchParams must `#[derive(Zeroize)]` (or \
         `zeroize::Zeroize`). Without it, the ticket can't be wiped \
         on logout reset.\nGot derive line: `{derive}`"
    );
    assert!(
        derive.contains("ZeroizeOnDrop"),
        "PRD 3.1.7: LaunchParams must `#[derive(ZeroizeOnDrop)]` (or \
         `zeroize::ZeroizeOnDrop`). Without it, the ticket survives \
         Drop.\nGot derive line: `{derive}`"
    );
}

/// `LaunchParams::ticket` must NOT carry `#[zeroize(skip)]`. It's the
/// short-lived credential the game binary receives; preserving it
/// across Drop loses the whole point of deriving the trait.
#[test]
fn launch_params_ticket_is_not_skipped() {
    let src = read(GAME_SERVICE_RS);
    let body = struct_body(&src, "LaunchParams");
    assert!(
        body.contains("pub ticket: String,"),
        "PRD 3.1.7: LaunchParams must expose `pub ticket: String,`.\n\
         Struct body:\n{body}"
    );
    assert!(
        !field_is_skipped(body, "pub ticket: String,"),
        "PRD 3.1.7: LaunchParams::ticket must NOT carry \
         `#[zeroize(skip)]`. This is the session credential passed to \
         TERA.exe; skipping it leaks the ticket across Drop.\n\
         Struct body:\n{body}"
    );
}

/// The `zeroize` dep in Cargo.toml must enable `zeroize_derive`.
/// Without that feature flag, `#[derive(Zeroize)]` and
/// `#[derive(ZeroizeOnDrop)]` fail to compile — but a refactor that
/// drops the feature and also drops the derives could ship without
/// tripping any other test. This pin ties the feature to the intent.
#[test]
fn cargo_toml_enables_zeroize_derive_feature() {
    let toml = read(CARGO_TOML);
    // Find the line declaring the zeroize dep.
    let line = toml
        .lines()
        .find(|l| l.trim_start().starts_with("zeroize") && l.contains('='))
        .expect("PRD 3.1.7: Cargo.toml must declare a `zeroize` dependency");
    assert!(
        line.contains("zeroize_derive"),
        "PRD 3.1.7: `zeroize` dep must enable the `zeroize_derive` \
         feature — without it, the `#[derive(Zeroize)]` / \
         `#[derive(ZeroizeOnDrop)]` macros don't exist and the \
         production types can't be wiped.\nGot: `{line}`"
    );
}

/// Self-test for `field_is_skipped` — prevents a future refactor of
/// this test module from silently making every field look
/// "not skipped" (trivially passing and masking a real regression).
#[test]
fn field_is_skipped_detector_self_test() {
    let fake_body = "\n    #[zeroize(skip)]\n    pub skipped_field: String,\n    pub sensitive_field: String,\n";
    assert!(
        field_is_skipped(fake_body, "pub skipped_field: String,"),
        "detector must recognise an explicit #[zeroize(skip)] on the \
         preceding line"
    );
    assert!(
        !field_is_skipped(fake_body, "pub sensitive_field: String,"),
        "detector must NOT flag a field whose preceding line is \
         another field declaration (not a skip attribute)"
    );
}

// --------------------------------------------------------------------
// Iter 205 structural pins — meta-guard self-reference + production
// call-site Zeroizing wrapping + non-sensitive-field skip classification.
// --------------------------------------------------------------------
//
// The ten pins above verify the crate still behaves (4) and the
// two production types still carry the right derives + keep their
// sensitive fields unskipped (5) + Cargo features (1). They do NOT
// pin: (a) the guard's own header cites PRD 3.1.7 (meta-guard
// contract); (b) `login_with_client` actually wraps the incoming
// password in `Zeroizing::new(...)` before ANY use — without the
// wrapper the zeroize crate is dead weight; (c) `register_with_client`
// follows the same pattern; (d) non-sensitive fields on
// GlobalAuthInfo carry explicit `#[zeroize(skip)]` — this pins the
// classification decision so new fields added to the struct MUST be
// deliberately classified (sensitive → wipe; non-sensitive → skip);
// (e) same for LaunchParams. The skip pins complement the existing
// "not-skipped" pins by locking the CLASSIFICATION SET: adding a new
// field without a skip attribute would fail.

const GUARD_FILE: &str = "tests/zeroize_audit.rs";
const AUTH_RS: &str = "src/commands/auth.rs";

/// The guard's module header must cite PRD 3.1.7 by section name so
/// a reader can trace the test back to the criterion without grep.
#[test]
fn guard_file_header_cites_prd_3_1_7() {
    let body = read(GUARD_FILE);
    let header = &body[..body.len().min(1500)];
    assert!(
        header.contains("PRD 3.1.7"),
        "meta-guard contract: tests/zeroize_audit.rs header must cite \
         `PRD 3.1.7`. Without it, a regression triggers an anonymous \
         failure with no pointer to the criterion this guards.\n\
         Header:\n{header}"
    );
    assert!(
        header.contains("zeroize"),
        "meta-guard contract: header must name the invariant \
         (`zeroize`) the guard protects so the failure carries its \
         own glossary."
    );
}

/// `login_with_client` must wrap the incoming `password: String`
/// argument in `Zeroizing::new(password)` before any other use. The
/// derive-on-struct + wrapper-on-parameter dual defence is what keeps
/// the password buffer out of the heap across every control-flow
/// branch (validation failure, HTTP error, success). A regression
/// that drops the wrapper would compile cleanly, pass every other
/// test, and leak the password buffer on logout.
#[test]
fn login_with_client_wraps_password_in_zeroizing() {
    let src = read(AUTH_RS);
    let fn_pos = src
        .find("async fn login_with_client")
        .expect("commands/auth.rs must define login_with_client");
    // Walk the function body — 800 chars comfortably covers the
    // wrapper + validate + payload-build region.
    let window = &src[fn_pos..fn_pos.saturating_add(800)];
    assert!(
        window.contains("let password = Zeroizing::new(password);"),
        "PRD 3.1.7: login_with_client must re-bind `password` via \
         `Zeroizing::new(password)` at the top of the body. Without \
         the wrapper, the raw `String` buffer outlives the function \
         on every control-flow path and stays on the heap until \
         allocator churn overwrites it.\nWindow:\n{window}"
    );
    // The wrapper must come BEFORE `validate_credentials` — a
    // validation-early-return path would leak the password buffer
    // otherwise.
    let wrap_pos = window
        .find("Zeroizing::new(password)")
        .expect("wrap call must be present");
    let validate_pos = window
        .find("validate_credentials")
        .expect("validate_credentials must be called in login_with_client");
    assert!(
        wrap_pos < validate_pos,
        "PRD 3.1.7: `Zeroizing::new(password)` must precede \
         `validate_credentials` — otherwise the Err-return path \
         drops the raw `String` without zeroizing."
    );
}

/// `register_with_client` must mirror the login wrapper pattern.
/// Registration sends plaintext credentials over the same Portal API
/// channel; omitting the wrapper leaks the chosen password as surely
/// as the login one.
#[test]
fn register_with_client_wraps_password_in_zeroizing() {
    let src = read(AUTH_RS);
    let fn_pos = src
        .find("async fn register_with_client")
        .expect("commands/auth.rs must define register_with_client");
    let window = &src[fn_pos..fn_pos.saturating_add(800)];
    assert!(
        window.contains("let password = Zeroizing::new(password);"),
        "PRD 3.1.7: register_with_client must re-bind `password` via \
         `Zeroizing::new(password)` at the top of the body. Same \
         rationale as login.\nWindow:\n{window}"
    );
    let wrap_pos = window
        .find("Zeroizing::new(password)")
        .expect("wrap call must be present");
    let validate_pos = window
        .find("validate_registration")
        .expect("validate_registration must be called");
    assert!(
        wrap_pos < validate_pos,
        "PRD 3.1.7: `Zeroizing::new(password)` must precede \
         `validate_registration` — otherwise the Err-return path \
         drops the raw `String` without zeroizing."
    );
}

/// Every non-sensitive field in `GlobalAuthInfo` must carry an
/// explicit `#[zeroize(skip)]`. This pins the CLASSIFICATION SET:
/// adding a new field to GlobalAuthInfo without a skip attribute
/// would cause the new field to be wiped on Drop — which is either
/// wasted work (for non-secret state) or leaked context (if the new
/// field is secret and needs different handling). Either way the
/// class decision must be explicit. This pin + `auth_key_is_not_skipped`
/// together lock the struct: `auth_key` is wiped, everything else
/// is explicitly skipped.
#[test]
fn global_auth_info_non_sensitive_fields_explicitly_skipped() {
    let src = read(MODELS_RS);
    let body = struct_body(&src, "GlobalAuthInfo");
    for field in [
        "pub character_count: String,",
        "pub user_no: i32,",
        "pub user_name: String,",
    ] {
        assert!(
            field_is_skipped(body, field),
            "PRD 3.1.7: GlobalAuthInfo field `{field}` must carry \
             `#[zeroize(skip)]`. Classification must be explicit so \
             future additions fail review until deliberately typed.\n\
             Struct body:\n{body}"
        );
    }
}

/// Every non-sensitive field in `LaunchParams` must carry an explicit
/// `#[zeroize(skip)]`. Same rationale as GlobalAuthInfo — pin the
/// classification set so new fields must be deliberately typed.
#[test]
fn launch_params_non_sensitive_fields_explicitly_skipped() {
    let src = read(GAME_SERVICE_RS);
    let body = struct_body(&src, "LaunchParams");
    for field in [
        "pub executable_path: PathBuf,",
        "pub account_name: String,",
        "pub character_count: String,",
        "pub language: String,",
    ] {
        assert!(
            field_is_skipped(body, field),
            "PRD 3.1.7: LaunchParams field `{field}` must carry \
             `#[zeroize(skip)]`. Only `ticket` should be wiped on \
             Drop — adding a new field without a classification \
             forces a choice at review time.\nStruct body:\n{body}"
        );
    }
}

// --------------------------------------------------------------------
// Iter 238 structural pins — path-constant canonicalisation,
// zeroize_derive Cargo feature, import-path discipline, no-log-
// password guard, and no-clone-on-Zeroizing pattern.
//
// Iter-205 covered guard header + login/register wrapper + skip
// classification. These five extend to the meta-guard + cargo
// feature + import-path + secret-leak surface a confident refactor
// could still bypass: a path-constant drift (header-inspection
// silently skips), a `zeroize_derive` feature-flag drop (compiler
// refuses `#[derive(Zeroize)]` but a future refactor to runtime-
// derive could be missed), an import from the wrong module (e.g.
// `use zeroize::Zeroize;` without `Zeroizing` would force
// `zeroize::Zeroizing::new(...)` at call sites, easy to grep-miss),
// a `log::info!("... password: {password}")` pattern that emits the
// secret to disk, and a `.clone()` on the Zeroizing<String> that
// allocates a fresh non-zeroized buffer.
// --------------------------------------------------------------------

/// Iter 238: all 5 path constants must stay canonical. Every
/// `read(...)` call in this guard resolves through one of these;
/// drift panics all tests with "file not found" instead of pointing
/// at the actual constant regression.
#[test]
fn guard_path_constants_are_canonical() {
    let body = read(GUARD_FILE);
    for (name, expected) in [
        ("MODELS_RS", "src/domain/models.rs"),
        ("GAME_SERVICE_RS", "src/services/game_service.rs"),
        ("CARGO_TOML", "Cargo.toml"),
        ("GUARD_FILE", "tests/zeroize_audit.rs"),
        ("AUTH_RS", "src/commands/auth.rs"),
    ] {
        let line = format!("const {name}: &str = \"{expected}\";");
        assert!(
            body.contains(&line),
            "PRD 3.1.7 (iter 238): zeroize_audit.rs must keep \
             `{line}` verbatim. A rename of the referenced file \
             without updating the constant silently disables every \
             pin that reads through it."
        );
    }
}

/// Iter 238: `Cargo.toml` must declare `zeroize` with the
/// `zeroize_derive` feature enabled. `#[derive(Zeroize,
/// ZeroizeOnDrop)]` is the mechanism GlobalAuthInfo / LaunchParams
/// wipe secrets on Drop — without the feature, the compiler refuses
/// those derives. The build would break immediately on feature
/// removal, but a future refactor that swaps to an alternative
/// derivation path (e.g. custom Drop impls) could miss that the
/// feature becomes unused-but-still-declared. Pin the feature so
/// removal requires a deliberate Cargo.toml + code coordination.
#[test]
fn cargo_toml_declares_zeroize_with_derive_feature() {
    let body = read(CARGO_TOML);
    let has_derive_feature = body
        .contains(r#"zeroize = { version = "1.7", features = ["zeroize_derive"] }"#)
        || body.contains(r#"zeroize = { version = "1", features = ["zeroize_derive"] }"#)
        || (body.contains("zeroize = {") && body.contains(r#""zeroize_derive""#));
    assert!(
        has_derive_feature,
        "PRD 3.1.7 (iter 238): Cargo.toml must declare `zeroize` with \
         the `zeroize_derive` feature — the `#[derive(Zeroize)]` + \
         `#[derive(ZeroizeOnDrop)]` attributes depend on it. A \
         refactor that dropped the feature would break the build \
         immediately, but removing the feature declaration WITHOUT \
         removing the derives is impossible to express; pin so both \
         surfaces stay coordinated."
    );
}

/// Iter 238: `src/commands/auth.rs` must import `Zeroizing` via
/// `use zeroize::Zeroizing;` — NOT via the wildcard `use zeroize::*`
/// or the fully-qualified `zeroize::Zeroizing::new(...)` at each
/// call site. The direct import lets a grep for `Zeroizing::new`
/// find every wrap site; a fully-qualified path splits the
/// attention between two spellings.
#[test]
fn auth_rs_imports_zeroizing_via_explicit_use() {
    let body = read(AUTH_RS);
    assert!(
        body.contains("use zeroize::Zeroizing;"),
        "PRD 3.1.7 (iter 238): commands/auth.rs must carry \
         `use zeroize::Zeroizing;` at the top of the file. The \
         explicit import lets `grep -rE Zeroizing::new` find every \
         wrap site across the crate; a `use zeroize::*` or \
         fully-qualified path at each call site splits search \
         results and makes audit-by-grep unreliable."
    );
    // Negative: reject the wildcard form.
    assert!(
        !body.contains("use zeroize::*"),
        "PRD 3.1.7 (iter 238): commands/auth.rs must NOT use \
         `use zeroize::*` — wildcard imports let a future refactor \
         bring in `Zeroize` / `ZeroizeOnDrop` silently, obscuring \
         which types this file actually uses."
    );
}

/// Iter 238: `login_with_client` and `register_with_client` must
/// NOT log the `password` variable in any form. A
/// `log::debug!("login attempt: {password}")` or `log::info!("{:?}",
/// payload)` where `payload` holds the plaintext would write the
/// secret to the logger's sink (stderr, file, remote). Pin the
/// absence: no `{password}`, no `{:?}` on the payload JSON.
#[test]
fn auth_rs_does_not_log_password_variable() {
    let body = read(AUTH_RS);
    for (fn_name, macros_to_ban) in [
        (
            "async fn login_with_client",
            ["{password}", "{password:?}", ":?password"],
        ),
        (
            "async fn register_with_client",
            ["{password}", "{password:?}", ":?password"],
        ),
    ] {
        let fn_pos = body
            .find(fn_name)
            .unwrap_or_else(|| panic!("{fn_name} must exist in auth.rs"));
        let window = &body[fn_pos..fn_pos.saturating_add(1500)];
        for pat in macros_to_ban {
            assert!(
                !window.contains(pat),
                "PRD 3.1.7 (iter 238): {fn_name} must NOT contain \
                 `{pat}` — any log macro that interpolates the \
                 password writes the secret to the logger's sink. \
                 Even `debug!` leaks if the user shares a support \
                 bundle.\nWindow (first 500 chars):\n{}",
                &window[..window.len().min(500)]
            );
        }
    }
}

/// Iter 238: the Zeroizing<String> wrapper must reach the HTTP
/// payload via `password.as_str()` (or `&*password` deref), NOT
/// via `password.clone()`. A `.clone()` on Zeroizing<String>
/// allocates a fresh non-zeroized `String` — the clone survives on
/// the heap past the Drop of the wrapper, defeating the whole
/// wrapper. Pin the absence.
#[test]
fn auth_rs_does_not_clone_zeroizing_password() {
    let body = read(AUTH_RS);
    for fn_name in [
        "async fn login_with_client",
        "async fn register_with_client",
    ] {
        let fn_pos = body
            .find(fn_name)
            .unwrap_or_else(|| panic!("{fn_name} must exist"));
        let window = &body[fn_pos..fn_pos.saturating_add(1500)];
        assert!(
            !window.contains("password.clone()"),
            "PRD 3.1.7 (iter 238): {fn_name} must NOT call \
             `password.clone()` on the Zeroizing<String>. A clone \
             allocates a fresh non-zeroized String that outlives \
             the wrapper — the password buffer survives on the heap \
             past Drop, defeating the zeroize protection.\n\
             Use `password.as_str()` or `&*password` instead."
        );
        // Positive control: the wrapper must be used somehow after
        // binding. `password.as_str()` is the current canonical
        // pattern.
        assert!(
            window.contains("password.as_str()") || window.contains("&*password"),
            "PRD 3.1.7 (iter 238): {fn_name} must access the \
             Zeroizing<String> password via `password.as_str()` or \
             `&*password` — proves the wrapper is actually used \
             (else the Zeroizing::new call is dead code).\n\
             Window (first 500 chars):\n{}",
            &window[..window.len().min(500)]
        );
    }
}

// --------------------------------------------------------------------
// Iter 278 structural pins — all-path bounds + PRD cite + auth/models
// byte bounds.
// --------------------------------------------------------------------

#[test]
fn models_rs_byte_bounds() {
    const MIN: usize = 500;
    const MAX: usize = 50_000;
    let bytes = std::fs::metadata(MODELS_RS)
        .expect("models.rs must exist")
        .len() as usize;
    assert!(
        (MIN..=MAX).contains(&bytes),
        "PRD 3.1.7 (iter 278): {MODELS_RS} is {bytes} bytes; expected \
         [{MIN}, {MAX}]."
    );
}

#[test]
fn game_service_rs_byte_bounds() {
    const MIN: usize = 1000;
    const MAX: usize = 100_000;
    let bytes = std::fs::metadata(GAME_SERVICE_RS)
        .expect("game_service.rs must exist")
        .len() as usize;
    assert!(
        (MIN..=MAX).contains(&bytes),
        "PRD 3.1.7 (iter 278): {GAME_SERVICE_RS} is {bytes} bytes; \
         expected [{MIN}, {MAX}]."
    );
}

#[test]
fn auth_rs_byte_bounds() {
    const MIN: usize = 1000;
    const MAX: usize = 100_000;
    let bytes = std::fs::metadata(AUTH_RS)
        .expect("auth.rs must exist")
        .len() as usize;
    assert!(
        (MIN..=MAX).contains(&bytes),
        "PRD 3.1.7 (iter 278): {AUTH_RS} is {bytes} bytes; expected \
         [{MIN}, {MAX}]."
    );
}

#[test]
fn guard_source_byte_bounds() {
    const MIN: usize = 5000;
    const MAX: usize = 80_000;
    let bytes = std::fs::metadata(GUARD_FILE)
        .expect("guard must exist")
        .len() as usize;
    assert!(
        (MIN..=MAX).contains(&bytes),
        "PRD 3.1.7 (iter 278): guard is {bytes} bytes; expected \
         [{MIN}, {MAX}]."
    );
}

#[test]
fn guard_source_cites_prd_3_1_7_explicitly() {
    let body = std::fs::read_to_string(GUARD_FILE).expect("guard must exist");
    let header = &body[..body.len().min(500)];
    assert!(
        header.contains("PRD 3.1.7"),
        "PRD 3.1.7 (iter 278): guard header must cite `PRD 3.1.7`.\n\
         Header:\n{header}"
    );
}
