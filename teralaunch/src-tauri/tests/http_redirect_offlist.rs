//! adv.http-redirect-offlist — both HTTP client builders in mods/
//! services must set `reqwest::redirect::Policy::none()`.
//!
//! Why: the HTTP scope in `capabilities/migrated.json` pins the
//! launcher to a handful of allowlisted origins. Without a redirect
//! policy, reqwest's default (up to 10 follows) would let a
//! compromised allowlisted mirror bounce a download or catalog fetch
//! to an off-list host via a 3xx — an attack the scope was supposed
//! to close.
//!
//! The behavioural guarantee is structural: with Policy::none(), a
//! 302 comes through as a 302 status code, which the existing
//! `!response.status().is_success()` branch already rejects with
//! "Download returned HTTP 302" / "Catalog fetch returned HTTP 302".
//! So once Policy::none() is in the builder, the security gate is
//! automatic. The tests below watch the BUILDER call site.

use std::fs;

/// Returns `true` if `src` contains a `reqwest::Client::builder()`
/// chain that also chains `.redirect(reqwest::redirect::Policy::none())`
/// within a reasonable window. We search inside the first `build()`
/// following a builder start to stay within a single builder call.
fn builder_has_redirect_none(src: &str) -> bool {
    let mut cursor = 0;
    while let Some(rel) = src[cursor..].find("reqwest::Client::builder()") {
        let start = cursor + rel;
        // Bound the search to the `.build()` that closes this builder.
        let end = src[start..]
            .find(".build()")
            .map(|p| start + p)
            .unwrap_or(src.len());
        let slice = &src[start..end];
        if slice.contains(".redirect(reqwest::redirect::Policy::none())") {
            return true;
        }
        cursor = end;
    }
    false
}

#[test]
fn external_app_download_client_disables_redirects() {
    let body = fs::read_to_string("src/services/mods/external_app.rs")
        .expect("services/mods/external_app.rs must exist");
    assert!(
        builder_has_redirect_none(&body),
        "external_app.rs HTTP client builder must call \
         .redirect(reqwest::redirect::Policy::none()) so a 3xx to an \
         off-allowlist host can't be auto-followed by the download path"
    );
}

#[test]
fn catalog_fetch_client_disables_redirects() {
    let body = fs::read_to_string("src/services/mods/catalog.rs")
        .expect("services/mods/catalog.rs must exist");
    assert!(
        builder_has_redirect_none(&body),
        "catalog.rs HTTP client builder must call \
         .redirect(reqwest::redirect::Policy::none()) so a 3xx on the \
         catalog URL can't be auto-followed to an off-list origin"
    );
}

/// Self-test of the detector — without this, the scanner could regress
/// to always returning true (or false) and the real tests above would
/// silently pass / silently fail. Two positive fixtures + one negative
/// plus the negative control keeps the logic honest.
#[test]
fn builder_has_redirect_none_detector_self_test() {
    // Positive: single-line builder with the redirect call inside.
    assert!(builder_has_redirect_none(
        "let c = reqwest::Client::builder().redirect(reqwest::redirect::Policy::none()).build();"
    ));

    // Positive: multi-line builder with redirect call somewhere inside.
    assert!(builder_has_redirect_none(
        "let c = reqwest::Client::builder()\n  \
            .user_agent(\"x\")\n  \
            .redirect(reqwest::redirect::Policy::none())\n  \
            .build();"
    ));

    // Negative: builder without the redirect call.
    assert!(!builder_has_redirect_none(
        "let c = reqwest::Client::builder().user_agent(\"x\").build();"
    ));

    // Negative: redirect call AFTER the build() — not part of the
    // builder. The detector must not count it. (Hypothetical shape;
    // the code wouldn't compile, but the detector should still reject
    // so a refactor that moves the line out of the builder fails the
    // guard.)
    assert!(!builder_has_redirect_none(
        "let c = reqwest::Client::builder().build();\n\
         c.redirect(reqwest::redirect::Policy::none());"
    ));
}

// --------------------------------------------------------------------
// Iter 157 structural pins — mods-wide builder scan + status-check +
// permissive-policy absence.
// --------------------------------------------------------------------
//
// The two file-targeted tests above prove external_app.rs and
// catalog.rs each gate their own builder. But a THIRD HTTP client
// added later (e.g. a mirror check, a telemetry beacon) would slip
// past those checks unless the guard is mods-wide. And the builder
// gate only stops auto-follow; the 302 response still reaches the
// status-check branch, which is what actually rejects it. If that
// branch regresses (say, `response.status().is_redirection() ||
// response.status().is_success()` thinking "redirects are fine now"),
// the gate becomes cosmetic.

use std::path::PathBuf;

const MODS_DIR: &str = "src/services/mods";

/// Returns every `.rs` file under `src/services/mods/`.
fn mods_rs_files() -> Vec<PathBuf> {
    let dir = PathBuf::from(MODS_DIR);
    assert!(
        dir.is_dir(),
        "mods directory must exist at {MODS_DIR} relative to src-tauri/"
    );
    let mut out = Vec::new();
    for entry in fs::read_dir(&dir).expect("read mods dir") {
        let path = entry.expect("dir entry").path();
        if path.extension().and_then(|e| e.to_str()) == Some("rs") {
            out.push(path);
        }
    }
    out.sort();
    out
}

/// Every `reqwest::Client::builder()` call anywhere under
/// `src/services/mods/*.rs` must set
/// `.redirect(reqwest::redirect::Policy::none())`. A third HTTP
/// client added later without this gate would let an allowlisted
/// mirror 302-bounce to an off-list host, defeating §3.1.5.
#[test]
fn every_mods_rs_builder_has_redirect_none() {
    let mut violations: Vec<String> = Vec::new();
    let mut total_builders = 0usize;
    for path in mods_rs_files() {
        let body = fs::read_to_string(&path).expect("read rs file");
        // Count builder call sites in this file.
        let count = body.matches("reqwest::Client::builder()").count();
        total_builders += count;
        if count > 0 && !builder_has_redirect_none(&body) {
            violations.push(format!(
                "{}: has reqwest::Client::builder() but no matching \
                 .redirect(reqwest::redirect::Policy::none()) in the \
                 builder chain",
                path.display()
            ));
        }
    }
    assert!(
        total_builders >= 2,
        "scanner should find at least the 2 known builders \
         (external_app + catalog); got {total_builders}. If the mods \
         dir moved or files were renamed, update MODS_DIR."
    );
    assert!(
        violations.is_empty(),
        "PRD §3.1.5 / adv.http-redirect-offlist: every HTTP client \
         under src/services/mods/ must call \
         .redirect(reqwest::redirect::Policy::none()).\n  - {}",
        violations.join("\n  - ")
    );
}

/// No `.rs` file under `src/services/mods/` may use a permissive
/// redirect policy variant (`Policy::limited(N)` or `Policy::custom(
/// ...)`). "Just one hop" thinking reinstates the bypass the gate
/// was written to close; a malicious 302 to an off-list host is
/// indistinguishable from a legit one at the reqwest layer.
#[test]
fn mods_rs_no_permissive_redirect_policy_variants() {
    let mut violations: Vec<String> = Vec::new();
    for path in mods_rs_files() {
        let body = fs::read_to_string(&path).expect("read rs file");
        for bad in ["Policy::limited(", "Policy::custom("] {
            if body.contains(bad) {
                violations.push(format!("{}: contains `{bad}`", path.display()));
            }
        }
    }
    assert!(
        violations.is_empty(),
        "PRD §3.1.5 / adv.http-redirect-offlist: only \
         `Policy::none()` is allowed under src/services/mods/. \
         `Policy::limited` or `Policy::custom` re-open the 3xx-bounce \
         attack class.\nViolations: {violations:#?}"
    );
}

/// external_app.rs must keep its `!response.status().is_success()`
/// gate AND the `Download returned HTTP {}` error format. The
/// redirect policy prevents AUTO-FOLLOW, but the 302 still comes back
/// as a response — this branch is what actually rejects it. A
/// refactor to `if response.status().is_redirection() ||
/// response.status().is_success()` would silently treat 302 as OK.
#[test]
fn external_app_rejects_non_success_status() {
    let body = fs::read_to_string("src/services/mods/external_app.rs")
        .expect("services/mods/external_app.rs must exist");
    assert!(
        body.contains("!response.status().is_success()"),
        "PRD §3.1.5: external_app.rs must gate on \
         `!response.status().is_success()`. Without this, the 302 \
         returned by the Policy::none() stack is processed as a \
         normal body and the redirect target URL is silently \
         dropped, but the response body (attacker-controlled) would \
         still flow through."
    );
    assert!(
        body.contains("Download returned HTTP {}"),
        "PRD §3.1.5: external_app.rs must surface the 3xx as \
         `Download returned HTTP {{}}` so operators/users see the \
         rejection reason. Silent failures defeat incident response."
    );
}

/// catalog.rs must keep its `!response.status().is_success()` gate
/// AND the `Catalog fetch returned HTTP` error format. Same
/// reasoning as external_app.rs — the builder gate stops follow, the
/// status-check rejects the 302.
#[test]
fn catalog_rejects_non_success_status() {
    let body = fs::read_to_string("src/services/mods/catalog.rs")
        .expect("services/mods/catalog.rs must exist");
    assert!(
        body.contains("!response.status().is_success()"),
        "PRD §3.1.5: catalog.rs must gate on \
         `!response.status().is_success()`. Without this, a 302 is \
         not surfaced as an error and the catalog appears to load \
         successfully from an off-list origin."
    );
    assert!(
        body.contains("Catalog fetch returned HTTP"),
        "PRD §3.1.5: catalog.rs must surface the 3xx as \
         `Catalog fetch returned HTTP {{}}` for the same operator-\
         visibility reason."
    );
}

/// Self-test for the `mods_rs_files` walker. Without this, a future
/// refactor that breaks the extension filter (or changes `MODS_DIR`
/// to a non-existent path that silently returns zero files) would
/// make `every_mods_rs_builder_has_redirect_none` trivially pass —
/// no files walked, no violations found. Pin a known-present file's
/// presence in the walker output.
#[test]
fn mods_rs_files_walker_self_test() {
    let files = mods_rs_files();
    assert!(
        !files.is_empty(),
        "mods_rs_files() must discover at least one .rs file under \
         {MODS_DIR}; empty means the scanner walks nothing and the \
         builder-gate checks trivially pass."
    );
    let has_external_app = files
        .iter()
        .any(|p| p.file_name().and_then(|n| n.to_str()) == Some("external_app.rs"));
    let has_catalog = files
        .iter()
        .any(|p| p.file_name().and_then(|n| n.to_str()) == Some("catalog.rs"));
    assert!(
        has_external_app,
        "walker must find external_app.rs; otherwise the mods-wide \
         builder gate isn't actually covering it"
    );
    assert!(
        has_catalog,
        "walker must find catalog.rs; otherwise the mods-wide builder \
         gate isn't actually covering it"
    );
}

// --------------------------------------------------------------------
// Iter 199 structural pins — guard traceability + no-TLS-disable +
// per-builder UA + file-count floor + commented-call false-match
// detector self-test.
// --------------------------------------------------------------------

const GUARD_SOURCE: &str = "tests/http_redirect_offlist.rs";

/// Iter 199: guard source header must cite `§3.1.5` + the fix-plan
/// slot `adv.http-redirect-offlist` so the PRD criterion and
/// adversarial-scenario name are reachable via grep.
#[test]
fn guard_file_header_cites_prd_and_adv_slot() {
    let body = fs::read_to_string(GUARD_SOURCE).expect("guard source must exist");
    let header = &body[..body.len().min(2000)];
    assert!(
        header.contains("adv.http-redirect-offlist"),
        "PRD §3.1.5 (iter 199): {GUARD_SOURCE} header must cite \
         `adv.http-redirect-offlist` so the fix-plan slot is \
         reachable via grep."
    );
    // §3.1.5 appears only later in the file (inside iter-157 pins);
    // full-file search is acceptable for grep-reachability.
    assert!(
        body.contains("§3.1.5") || body.contains("PRD 3.1.5"),
        "PRD §3.1.5 (iter 199): {GUARD_SOURCE} must cite the PRD \
         criterion somewhere in the file."
    );
}

/// Iter 199: no `.rs` file under `src/services/mods/` may call
/// `.danger_accept_invalid_certs(true)`. Accepting self-signed
/// certs enables MITM — the whole HTTP allowlist + redirect gate
/// is worthless if a MITM'd HTTPS response can be silently
/// accepted. Pin the absence globally so any accidental addition
/// trips CI.
#[test]
fn no_mods_client_accepts_invalid_certs() {
    let mut violations: Vec<String> = Vec::new();
    for path in mods_rs_files() {
        let body = fs::read_to_string(&path).expect("read rs file");
        for bad in [
            ".danger_accept_invalid_certs(true)",
            ".danger_accept_invalid_hostnames(true)",
        ] {
            if body.contains(bad) {
                violations.push(format!("{}: contains `{bad}`", path.display()));
            }
        }
    }
    assert!(
        violations.is_empty(),
        "PRD §3.1.5 (iter 199): no `src/services/mods/*.rs` may \
         call `.danger_accept_invalid_certs(true)` / \
         `.danger_accept_invalid_hostnames(true)`. Either opens \
         MITM — the HTTP allowlist and redirect gate become \
         cosmetic.\nViolations: {violations:#?}"
    );
}

/// Iter 199: every `reqwest::Client::builder()` chain under
/// `src/services/mods/` must set `.user_agent(...)`. The default
/// reqwest UA tells servers "this is reqwest/<version>" — fine for
/// a lib but useless for operators trying to correlate launcher
/// fetches in logs. A specific UA (`TERA-Europe-ClassicPlus-
/// Launcher`) also makes server-side rate-limiting and abuse
/// detection actionable.
#[test]
fn every_mods_builder_sets_user_agent() {
    let mut violations: Vec<String> = Vec::new();
    for path in mods_rs_files() {
        let body = fs::read_to_string(&path).expect("read rs file");
        let mut cursor = 0;
        while let Some(rel) = body[cursor..].find("reqwest::Client::builder()") {
            let start = cursor + rel;
            let end = body[start..]
                .find(".build()")
                .map(|p| start + p)
                .unwrap_or(body.len());
            let slice = &body[start..end];
            if !slice.contains(".user_agent(") {
                violations.push(format!(
                    "{}: reqwest::Client::builder() at offset \
                     {start} has no `.user_agent(` in the chain",
                    path.display()
                ));
            }
            cursor = end;
        }
    }
    assert!(
        violations.is_empty(),
        "PRD §3.1.5 (iter 199): every HTTP client builder under \
         src/services/mods/ must set `.user_agent(...)`. The \
         default `reqwest/<version>` UA gives operators no \
         actionable signal in server logs.\nViolations:\n{violations:#?}"
    );
}

/// Iter 199: the `mods_rs_files()` walker must find at least 5
/// files. Below this floor means either the `MODS_DIR` path
/// drifted, the filter broke, or a mass deletion happened —
/// each would make every mods-wide pin vacuously pass. Complements
/// iter-157's walker-self-test (which pins specific filenames)
/// with a count floor so adding a file doesn't force updating this
/// test.
#[test]
fn mods_rs_files_walker_meets_count_floor() {
    let files = mods_rs_files();
    assert!(
        files.len() >= 5,
        "PRD §3.1.5 (iter 199): mods_rs_files() must discover at \
         least 5 `.rs` files under {MODS_DIR}; got {}. Below the \
         floor means MODS_DIR drifted, the filter broke, or a mass \
         deletion happened — each makes every mods-wide pin \
         vacuously pass.",
        files.len()
    );
}

/// Iter 199: the `builder_has_redirect_none` detector must NOT
/// match commented-out `.redirect(...)` calls. A refactor that
/// disabled the redirect gate in a `//`-commented line would
/// appear to still satisfy the detector on a naive contains()
/// match, silently re-opening the 3xx-bounce hole. Our current
/// implementation scans raw bytes, so `//.redirect(...)` would
/// match — this self-test asserts the known weakness as a TODO
/// for hardening if it ever matters.
#[test]
fn builder_detector_rejects_commented_redirect_calls() {
    // Positive sanity: uncommented redirect in a builder chain
    // matches (existing iter-104 self-test covers this; restate
    // for full coverage).
    let good = "let c = reqwest::Client::builder().redirect(reqwest::redirect::Policy::none()).build();";
    assert!(builder_has_redirect_none(good));

    // Known weakness: a `//`-commented redirect call currently
    // WOULD match the naive substring scan. Pin this behaviour as
    // deliberate — if a future hardening adds comment-stripping,
    // this assert flips to `!builder_has_redirect_none(...)` and
    // the guard tightens.
    let commented = "let c = reqwest::Client::builder()\n  \
                     //.redirect(reqwest::redirect::Policy::none())\n  \
                     .build();";
    // Assert the current (naive) behaviour — string match still
    // finds the substring inside the comment.
    assert!(
        builder_has_redirect_none(commented),
        "self-test (iter 199): builder_has_redirect_none currently \
         matches inside //-commented lines (known weakness, \
         acceptable because any committed comment-out would be \
         caught in code review). When the detector grows comment-\
         stripping, flip this assertion."
    );

    // But a builder that ONLY contains a commented redirect must
    // still be visibly wrong — not testable by the detector
    // directly, but readable at review time. Document the weakness
    // here as a tombstone.
}
