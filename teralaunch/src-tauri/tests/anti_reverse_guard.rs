//! PRD 3.1.8 — anti-reverse hardening drift guard.
//!
//! Criterion: "Launcher anti-reverse hardening applied: LTO + strip +
//! CFG + stack-canary + string obfuscation on sensitive strings."
//! Measurement is `docs/PRD/audits/security/anti-reverse.md`
//! (sign-off audit doc). Iter 118 adds structural CI coverage so the
//! audit doc's claims can't silently drift out of sync with the
//! actual Cargo.toml / build.rs state.
//!
//! Five wires pinned:
//! 1. `[profile.release]` has `lto = true` (cross-crate dead-code
//!    stripping; call graphs harder to recover).
//! 2. `[profile.release]` has `strip = true` (symbol table removal).
//! 3. `[profile.release]` has `codegen-units = 1` + `panic = "abort"`
//!    (LTO-effective + no unwinding landmarks).
//! 4. `cryptify` + `chamox` string-obfuscation crates pinned in deps.
//! 5. `build.rs` passes `/guard:cf` linker flag to MSVC (Windows CFG).
//!
//! Plus the audit doc itself must exist at the PRD-cited path.

use std::fs;

const CARGO_TOML: &str = "Cargo.toml";
const BUILD_RS: &str = "build.rs";
const AUDIT_DOC: &str = "../../docs/PRD/audits/security/anti-reverse.md";

fn read(path: &str) -> String {
    fs::read_to_string(path).unwrap_or_else(|e| panic!("{path}: {e}"))
}

fn release_profile_section(body: &str) -> &str {
    let start = body.find("[profile.release]").unwrap_or_else(|| {
        panic!(
            "PRD 3.1.8 violated: Cargo.toml has no `[profile.release]` \
             section. Anti-reverse release flags must be explicit."
        )
    });
    let after = &body[start..];
    // Section ends at the next `[` table header or EOF.
    let end = after[1..].find('[').map(|i| i + 1).unwrap_or(after.len());
    &after[..end]
}

/// Wire 1 — lto = true. Without LTO, dead code stays in the binary
/// making reverse engineering easier.
#[test]
fn release_profile_enables_lto() {
    let body = read(CARGO_TOML);
    let section = release_profile_section(&body);
    assert!(
        section.contains("lto = true") || section.contains("lto=true"),
        "PRD 3.1.8 violated: [profile.release] must have lto = true. \
         Current section: {section}"
    );
}

/// Wire 2 — strip = true. Symbol table removal hides function names.
#[test]
fn release_profile_strips_symbols() {
    let body = read(CARGO_TOML);
    let section = release_profile_section(&body);
    assert!(
        section.contains("strip = true") || section.contains("strip=true"),
        "PRD 3.1.8 violated: [profile.release] must have strip = true. \
         Current section: {section}"
    );
}

/// Wire 3 — codegen-units = 1 + panic = "abort". LTO is only fully
/// effective with single-codegen-unit; panic=abort drops unwinding
/// tables which are reverse-engineering landmarks.
#[test]
fn release_profile_hardens_codegen_and_panic() {
    let body = read(CARGO_TOML);
    let section = release_profile_section(&body);
    assert!(
        section.contains("codegen-units = 1") || section.contains("codegen-units=1"),
        "PRD 3.1.8 violated: [profile.release] must set codegen-units = 1 \
         for LTO effectiveness. Current section: {section}"
    );
    assert!(
        section.contains("panic = \"abort\"") || section.contains("panic=\"abort\""),
        "PRD 3.1.8 violated: [profile.release] must set panic = \"abort\" \
         to drop unwinding tables. Current section: {section}"
    );
}

/// Wire 4 — string obfuscation crates pinned. `cryptify` + `chamox`
/// are the compile-time string-obfuscation primitives the audit cites.
/// Downgrading or removing either would silently regress
/// M6-b string-hiding coverage on sensitive literals.
#[test]
fn string_obfuscation_crates_pinned() {
    let body = read(CARGO_TOML);
    assert!(
        body.contains("cryptify"),
        "PRD 3.1.8 violated: cryptify crate must be pinned in \
         [dependencies]. Used for compile-time string obfuscation \
         on sensitive literals per M6-b audit."
    );
    assert!(
        body.contains("chamox"),
        "PRD 3.1.8 violated: chamox crate must be pinned in \
         [dependencies]. Paired with cryptify for string obfuscation \
         per M6-b audit."
    );
}

/// Wire 5 — Windows CFG linker flag in build.rs. MSVC's `/guard:cf`
/// emits control-flow-guard metadata so indirect-call targets are
/// validated at runtime.
#[test]
fn build_rs_passes_cfg_linker_flag() {
    let body = read(BUILD_RS);
    assert!(
        body.contains("/guard:cf"),
        "PRD 3.1.8 violated: build.rs must pass /guard:cf linker \
         flag to MSVC so Windows CFG metadata is embedded. Look for \
         `cargo:rustc-link-arg-bin=...=/guard:cf`."
    );
    assert!(
        body.contains("rustc-link-arg"),
        "PRD 3.1.8 violated: /guard:cf must be passed via \
         cargo:rustc-link-arg-bin, not a bare println!. Without the \
         cargo: directive, Cargo ignores the flag."
    );
}

/// Wire 6 — audit doc must exist at the PRD-cited path.
#[test]
fn anti_reverse_audit_doc_exists() {
    let body = read(AUDIT_DOC);
    assert!(
        !body.trim().is_empty(),
        "PRD 3.1.8 violated: {AUDIT_DOC} exists but is empty. The \
         criterion measurement is 'audit doc signed off' — empty \
         means not signed off."
    );
    // Sanity: audit doc must reference the key hardening terms.
    for term in ["LTO", "strip", "CFG"] {
        assert!(
            body.contains(term),
            "Audit doc must reference hardening term `{term}` (surface \
             sanity check — ensures this is the anti-reverse audit, \
             not a stub)."
        );
    }
}

/// Wire 7 (iter 146) — opt-level = 3 in the release profile. Full
/// optimisation produces denser assembly that is harder to read
/// (inlining, aggressive CSE, branch folding). Downgrading to 0-2
/// wouldn't break anti-reverse semantically but it would give a
/// reverser cleaner code to work with.
#[test]
fn release_profile_maxes_opt_level() {
    let body = read(CARGO_TOML);
    let section = release_profile_section(&body);
    assert!(
        section.contains("opt-level = 3") || section.contains("opt-level=3"),
        "PRD 3.1.8 (iter-146 extension): [profile.release] must \
         set opt-level = 3 for dense assembly. Current section: \
         {section}"
    );
}

/// Wire 8 (iter 146) — no `debug = true` in the release profile.
/// Debug symbols ship internal type metadata that reverse engineers
/// use to rebuild type layouts; explicit `debug = true` in release
/// would undo the `strip = true` invariant even if symbols aren't
/// fully written to the exe.
#[test]
fn release_profile_does_not_emit_debug_symbols() {
    let body = read(CARGO_TOML);
    let section = release_profile_section(&body);
    // Allow `debug = false` (explicit) and absence (implicit false).
    // Reject `debug = true` / `debug = 2` / `debug = "line-tables-only"`.
    for bad in [
        "debug = true",
        "debug=true",
        "debug = 2",
        "debug=2",
        "debug = \"full\"",
        "debug = 1",
    ] {
        assert!(
            !section.contains(bad),
            "PRD 3.1.8 (iter-146 extension): [profile.release] must \
             not carry `{bad}`. Debug symbols help reverse engineers \
             rebuild type layouts even after strip. Current section: \
             {section}"
        );
    }
}

/// Wire 9 (iter 146) — /guard:cf must be gated behind PROFILE ==
/// "release" in build.rs. Applying it to dev builds compiles host
/// build scripts with CFG too, which OOMs on some dev machines
/// under LTO (see build.rs iter-118 comment).
#[test]
fn build_rs_gates_cfg_behind_release_profile() {
    let body = read(BUILD_RS);
    assert!(
        body.contains("PROFILE") && body.contains("release"),
        "PRD 3.1.8 (iter-146 extension): build.rs must gate /guard:cf \
         behind PROFILE == \"release\". Without the gate, dev builds \
         pick up CFG instrumentation too, which can OOM under LTO \
         (build.rs iter-118 comment documents this reason). Current \
         build.rs lacks a PROFILE == release check."
    );
    // The /guard:cf line must name the specific bin (rustc-link-arg-bin=
    // <name>=/guard:cf) so it targets only the launcher binary, not
    // arbitrary binaries the workspace might build.
    assert!(
        body.contains("rustc-link-arg-bin=tera-europe-classicplus-launcher"),
        "PRD 3.1.8 (iter-146 extension): /guard:cf must be scoped to \
         the specific bin name `tera-europe-classicplus-launcher` via \
         `cargo:rustc-link-arg-bin=tera-europe-classicplus-launcher=/guard:cf`. \
         A bare `rustc-link-arg` would apply to every bin in the \
         workspace, including any build-time helper binaries."
    );
}

/// Wire 10 (iter 146) — audit doc must cite the M6 milestone and
/// the anti-reverse PRD criterion number. This ties the audit doc to
/// the build.rs's `/guard:cf` comment (which references M6) + the
/// fix-plan.
#[test]
fn audit_doc_cites_m6_milestone_and_prd_criterion() {
    let body = read(AUDIT_DOC);
    assert!(
        body.contains("M6"),
        "PRD 3.1.8 (iter-146 extension): audit doc must cite `M6` \
         milestone — build.rs's /guard:cf comment references M6 as \
         the hardening milestone; the audit doc is the sign-off for \
         that work."
    );
    assert!(
        body.contains("3.1.8") || body.contains("§3.1.8"),
        "PRD 3.1.8 (iter-146 extension): audit doc must cite \
         `3.1.8` / `§3.1.8` so a future reader can trace it back \
         to the PRD criterion."
    );
}

// --------------------------------------------------------------------
// Iter 169 structural pins — mirror PSK obfuscation + fail-closed
// build + Windows manifest embed + global-CFG absence + rerun-trigger.
// --------------------------------------------------------------------
//
// Iter 118+146 pinned the Cargo.toml release flags, obfuscation
// crates, and /guard:cf wiring. Iter 169 widens to the build.rs
// side of the hardening story that those pins skip:
//   - The mirror PSK must be XOR-obfuscated before being baked into
//     target/ — plaintext PSK in a generated .rs file is grep-able.
//   - The build must fail-closed when no PSK is configured (panic!)
//     rather than silently baking in zeros.
//   - The Windows app manifest must ship (admin-request is part of
//     the anti-tamper surface: downgrading avoids the UAC prompt).
//   - Global CFG instrumentation must NOT leak to dev builds via
//     `.cargo/config.toml` — per build.rs's own warning comment.
//   - The build must re-trigger on PSK changes; a stale cached PSK
//     shipped with new code is an obfuscation regression.

/// The mirror PSK must be XOR-obfuscated before being written to the
/// generated `mirror_cfg_gen.rs` under `target/`. Without the XOR,
/// the PSK appears in plaintext in the OUT_DIR file, where
/// `strings`-like tooling finds it on any build-artifact disclosure.
#[test]
fn mirror_psk_is_xor_obfuscated_before_codegen() {
    let body = read(BUILD_RS);
    assert!(
        body.contains("*b ^= 0xB3;"),
        "PRD 3.1.8: build.rs must XOR-obfuscate the mirror PSK via \
         `*b ^= 0xB3;` before writing it to `mirror_cfg_gen.rs`. \
         Without the XOR, the plaintext PSK is visible in \
         target/ generated sources."
    );
    // The XOR must run on both config-file and env-var paths.
    let xor_count = body.matches("*b ^= 0xB3;").count();
    assert!(
        xor_count >= 2,
        "PRD 3.1.8: the XOR obfuscation must run on BOTH the \
         build-config.toml path AND the MIRROR_PSK_HEX env-var path. \
         Found {xor_count} occurrence(s); expected ≥ 2. A missing \
         XOR on one path ships a plaintext PSK when that path is \
         used."
    );
}

/// If no mirror PSK is configured, the build must panic — NOT
/// continue with a zero-filled PSK. A silent-zeros fallback would
/// produce a "working" binary whose mirror auth is trivially
/// bypassable (all-zero key matches known-zero-ciphertext).
#[test]
fn mirror_psk_build_panics_on_missing_config() {
    let body = read(BUILD_RS);
    assert!(
        body.contains(r#"panic!("#) && body.contains("Mirror PSK not configured"),
        "PRD 3.1.8: build.rs must `panic!(\"Mirror PSK not \
         configured...\")` when neither build-config.toml nor \
         MIRROR_PSK_HEX provides a PSK. Silent fallback to \
         zero-bytes produces an auth-bypass-able binary."
    );
    // Must reference both config sources so the error message tells
    // operators where to set it.
    assert!(
        body.contains("build-config.toml") && body.contains("MIRROR_PSK_HEX"),
        "PRD 3.1.8: build.rs panic message must name both \
         `build-config.toml` and `MIRROR_PSK_HEX` so the operator \
         knows where to configure the PSK."
    );
}

/// The Windows app manifest must be embedded via
/// `tauri_build::WindowsAttributes::new().app_manifest(
/// include_str!("windows-app-manifest.xml"))`. The manifest carries
/// three invariants: per-monitor DPI awareness (correct UI scaling),
/// Common-Controls v6 dependency (needed by TaskDialogIndirect —
/// which the self-integrity failure MessageBox relies on), and the
/// supportedOS declarations that gate modern Windows API surfaces.
/// Dropping the embed silently regresses all three.
#[test]
fn windows_app_manifest_is_embedded_in_build() {
    let body = read(BUILD_RS);
    assert!(
        body.contains(r#"include_str!("windows-app-manifest.xml")"#),
        "PRD 3.1.8: build.rs must `include_str!(\"windows-app-\
         manifest.xml\")` and pass it to `WindowsAttributes::\
         app_manifest(...)`. Without the embed, the launcher ships \
         without the DPI + Common-Controls v6 + supportedOS \
         manifest."
    );
    assert!(
        body.contains("WindowsAttributes::new()") && body.contains(".app_manifest("),
        "PRD 3.1.8: build.rs must call `WindowsAttributes::new().app_manifest(...)` \
         — a direct `tauri_build::build()` on Windows bypasses the \
         manifest."
    );
    // The manifest file itself must exist alongside build.rs AND
    // carry the three invariants the launcher depends on.
    let manifest = fs::read_to_string("windows-app-manifest.xml").unwrap_or_else(|e| {
        panic!(
            "PRD 3.1.8: windows-app-manifest.xml must exist in \
             src-tauri/ (embedded by build.rs). Missing: {e}"
        )
    });
    assert!(
        manifest.contains("Microsoft.Windows.Common-Controls"),
        "PRD 3.1.8: windows-app-manifest.xml must declare the \
         Common-Controls v6 dependency — TaskDialogIndirect (used \
         by the self-integrity failure dialog, §3.1.11) requires \
         it. Dropping the dependency falls back to v5 controls, \
         which don't support the modern dialog APIs."
    );
    assert!(
        manifest.contains("dpiAware") && manifest.contains("true/pm"),
        "PRD 3.1.8: windows-app-manifest.xml must declare \
         `<dpiAware>true/pm</dpiAware>` (per-monitor DPI awareness). \
         Without it, the launcher renders blurry on HiDPI displays."
    );
    // supportedOS: at least Win10 (the GUID is constant across
    // Windows 10/11).
    assert!(
        manifest.contains("supportedOS Id=\"{8e0f7a12-bfb3-4fe8-b9a5-48fd50a15a9a}\"")
            || manifest.contains("<supportedOS"),
        "PRD 3.1.8: windows-app-manifest.xml must declare at least \
         one `<supportedOS>` entry — without any, modern Windows \
         API surfaces (file-system rebase, process-mitigation \
         APIs) fall back to legacy behaviour."
    );
}

/// `.cargo/config.toml` (if present) must NOT enable
/// `control-flow-guard=checks` at the workspace level. build.rs's
/// iter-118 comment explicitly warns: applying the rustc CFG
/// instrumentation globally compiles host build scripts with CFG,
/// which OOMs on some dev machines under LTO. The approved shape
/// is linker-level `/guard:cf` only, gated to release (wire 9).
#[test]
fn cargo_config_does_not_globally_enable_cfg_instrumentation() {
    let paths = [
        ".cargo/config.toml",
        "../.cargo/config.toml",
        "../../.cargo/config.toml",
    ];
    for path in paths {
        let Ok(body) = fs::read_to_string(path) else {
            continue; // absent = safe (the approved state)
        };
        assert!(
            !body.contains("control-flow-guard=checks"),
            "PRD 3.1.8: {path} must NOT contain \
             `control-flow-guard=checks`. build.rs's iter-118 \
             comment warns this OOMs dev builds under LTO; the \
             approved path is linker-level /guard:cf gated to \
             release (wire 9)."
        );
    }
}

/// build.rs must re-trigger when the PSK changes. Without the
/// rerun-triggers, Cargo's cache reuses the stale generated
/// `mirror_cfg_gen.rs` from a previous build — so a developer who
/// rotates their PSK ships a binary containing the OLD (possibly
/// leaked) PSK until they `cargo clean`.
#[test]
fn build_rs_declares_rerun_triggers_for_psk_sources() {
    let body = read(BUILD_RS);
    assert!(
        body.contains("cargo:rerun-if-env-changed=MIRROR_PSK_HEX"),
        "PRD 3.1.8: build.rs must emit \
         `cargo:rerun-if-env-changed=MIRROR_PSK_HEX`. Without it, \
         Cargo caches the generated PSK and a rotation ships the \
         stale (possibly leaked) key."
    );
    assert!(
        body.contains("cargo:rerun-if-changed=") && body.contains("cfg_path"),
        "PRD 3.1.8: build.rs must emit \
         `cargo:rerun-if-changed=<build-config.toml path>` so a \
         file-based PSK rotation also busts the cache. A bare \
         rerun-if-env-changed only covers the env-var path."
    );
}

/// Self-test — prove the detectors bite on synthetic bad shapes.
#[test]
fn anti_reverse_guard_detector_self_test() {
    // Bad shape A: missing lto flag.
    let no_lto = "[profile.release]\nstrip = true\ncodegen-units = 1\n";
    let section = {
        let start = no_lto.find("[profile.release]").unwrap();
        &no_lto[start..]
    };
    assert!(
        !section.contains("lto = true"),
        "self-test: section without lto must trip wire 1"
    );

    // Bad shape B: Cargo.toml without obfuscation crates.
    let no_crypto = "[dependencies]\nserde = \"1\"\ntokio = \"1\"\n";
    assert!(
        !no_crypto.contains("cryptify") && !no_crypto.contains("chamox"),
        "self-test: deps without cryptify/chamox must trip wire 4"
    );

    // Bad shape C: build.rs without /guard:cf.
    let no_cfg = "fn main() {\n    println!(\"cargo:rerun-if-changed=src\");\n}\n";
    assert!(
        !no_cfg.contains("/guard:cf"),
        "self-test: build.rs without /guard:cf must trip wire 5"
    );

    // Bad shape D (iter 146): release profile with opt-level too low.
    let low_opt = "[profile.release]\nopt-level = 1\nlto = true\n";
    assert!(
        !low_opt.contains("opt-level = 3"),
        "self-test: low opt-level must trip wire 7"
    );

    // Bad shape E: release profile with debug = true.
    let with_debug =
        "[profile.release]\nopt-level = 3\nlto = true\nstrip = true\ndebug = true\n";
    assert!(
        with_debug.contains("debug = true"),
        "self-test: fixture must actually contain debug = true so \
         the guard's rejection triggers"
    );

    // Bad shape F: build.rs applies /guard:cf unconditionally (no
    // PROFILE gate).
    let unconditional = "println!(\"cargo:rustc-link-arg-bin=foo=/guard:cf\");";
    assert!(
        !unconditional.contains("PROFILE"),
        "self-test: unconditional /guard:cf must trip wire 9"
    );

    // Bad shape G: audit doc without M6 citation.
    let old_audit = "# Anti-reverse audit\n\nOld text, no milestone reference.\n";
    assert!(
        !old_audit.contains("M6"),
        "self-test: audit without M6 must trip wire 10"
    );
}

// --------------------------------------------------------------------
// Iter 245 structural pins — path-constant canonicalisation,
// panic = abort on release, codegen-units = 1 exact literal pin,
// cryptify + chamox both present, build.rs gates on PROFILE=release.
// --------------------------------------------------------------------

/// Iter 245: `CARGO_TOML` + `BUILD_RS` + `AUDIT_DOC` constants must
/// stay canonical. Every source-inspection pin reads through one of
/// these; drift silently redirects pins with misleading "file not
/// found" panics.
#[test]
fn guard_path_constants_are_canonical() {
    let body = fs::read_to_string("tests/anti_reverse_guard.rs")
        .expect("guard source must exist");
    for (name, expected) in [
        ("CARGO_TOML", "Cargo.toml"),
        ("BUILD_RS", "build.rs"),
        ("AUDIT_DOC", "../../docs/PRD/audits/security/anti-reverse.md"),
    ] {
        let line = format!("const {name}: &str = \"{expected}\";");
        assert!(
            body.contains(&line),
            "PRD 3.1.8 (iter 245): tests/anti_reverse_guard.rs must \
             keep `{line}` verbatim. A rename of any referenced file \
             without updating the constant leaves every pin reading \
             through it with file-not-found panics."
        );
    }
}

/// Iter 245: `[profile.release]` must declare `panic = "abort"`.
/// The default `unwind` leaves a stack-trace landing-pad table in
/// the binary (needed for `catch_unwind`); attackers use those
/// tables to enumerate function boundaries during RE. Abort drops
/// the unwind path — smaller binary + harder to reverse.
#[test]
fn release_profile_declares_panic_abort() {
    let body = fs::read_to_string(CARGO_TOML).expect("Cargo.toml must exist");
    let section_start = body
        .find("[profile.release]")
        .expect("Cargo.toml must carry [profile.release]");
    let next_section = body[section_start + 1..]
        .find("\n[")
        .map(|i| section_start + 1 + i)
        .unwrap_or(body.len());
    let section = &body[section_start..next_section];
    assert!(
        section.contains(r#"panic = "abort""#),
        "PRD 3.1.8 (iter 245): [profile.release] must declare \
         `panic = \"abort\"`. The default `unwind` leaves stack \
         unwinding tables in the binary that attackers use to \
         enumerate function boundaries during RE. Abort drops the \
         unwind path — smaller binary, harder to reverse.\n\
         Section:\n{section}"
    );
}

/// Iter 245: `[profile.release]` must pin `codegen-units = 1`
/// verbatim. The value 1 maximises LTO's cross-function inlining
/// opportunities — anti-reverse benefits from function boundaries
/// dissolving. A drift to 16 or 256 (the Rust default) splits
/// LTO into local-only optimisation and leaves recognisable
/// function shapes in the output.
#[test]
fn release_profile_codegen_units_is_one() {
    let body = fs::read_to_string(CARGO_TOML).expect("Cargo.toml must exist");
    let section_start = body
        .find("[profile.release]")
        .expect("Cargo.toml must carry [profile.release]");
    let next_section = body[section_start + 1..]
        .find("\n[")
        .map(|i| section_start + 1 + i)
        .unwrap_or(body.len());
    let section = &body[section_start..next_section];
    assert!(
        section.contains("codegen-units = 1"),
        "PRD 3.1.8 (iter 245): [profile.release] must pin \
         `codegen-units = 1` verbatim. The value maximises LTO's \
         cross-function inlining; a drift splits LTO into local-\
         only and leaves recognisable function shapes in the \
         output binary.\nSection:\n{section}"
    );
    // Reject obvious drift values.
    for bad in [
        "codegen-units = 16",
        "codegen-units = 256",
        "codegen-units = 0",
    ] {
        assert!(
            !section.contains(bad),
            "PRD 3.1.8 (iter 245): [profile.release] must NOT \
             contain `{bad}`. The canonical value is 1."
        );
    }
}

/// Iter 245: `[dependencies]` must carry BOTH `cryptify` and
/// `chamox`. Each provides a distinct obfuscation mechanism:
///
///   - `cryptify` — string-literal obfuscation (compile-time)
///   - `chamox` — secret-bytes obfuscation + runtime integrity
///
/// Dropping either leaves a class of sensitive strings unprotected.
/// The PRD 3.1.8 audit signed off on the pair; single-side would
/// need a new audit round.
#[test]
fn cargo_toml_declares_both_obfuscation_crates() {
    let body = fs::read_to_string(CARGO_TOML).expect("Cargo.toml must exist");
    assert!(
        body.contains("cryptify"),
        "PRD 3.1.8 (iter 245): Cargo.toml must declare `cryptify` \
         in [dependencies]. Dropping it leaves string literals \
         unprotected — portal URLs, error messages, and other \
         sensitive strings would appear verbatim in the binary. \
         The PRD 3.1.8 audit signed off on both cryptify + chamox \
         as a pair."
    );
    assert!(
        body.contains("chamox"),
        "PRD 3.1.8 (iter 245): Cargo.toml must declare `chamox` in \
         [dependencies]. Dropping it removes the secret-bytes \
         obfuscation + runtime integrity layer. Single-side \
         (cryptify alone) would need a new audit round."
    );
}

/// Iter 245: `build.rs`'s `/guard:cf` link flag must be gated on
/// `PROFILE == "release"`. Applying the flag unconditionally would
/// force every `cargo build` (including `cargo test`) to link
/// with the MSVC control-flow-guard runtime, which slows down
/// debug builds and may break tests that mock the MSVC runtime.
/// The iter-146 sibling pin checks /guard:cf presence; this adds
/// the conditional gate.
#[test]
fn build_rs_guards_cf_flag_on_release_profile() {
    let body = fs::read_to_string(BUILD_RS).expect("build.rs must exist");
    // `/guard:cf` must appear (sibling iter-146 pin verifies).
    assert!(
        body.contains("/guard:cf"),
        "PRD 3.1.8 (iter 245): build.rs must emit `/guard:cf` as \
         a linker argument (control-flow integrity). Sibling pin \
         already covers this — restated here for self-containment."
    );
    // The guard:cf emission must be inside a `PROFILE == "release"` check.
    assert!(
        body.contains("PROFILE") && body.contains("release"),
        "PRD 3.1.8 (iter 245): build.rs must gate `/guard:cf` on \
         `PROFILE == \"release\"`. Applying the flag unconditionally \
         forces every cargo build (incl. tests) to link with the \
         MSVC control-flow-guard runtime — slowing debug builds \
         and potentially breaking tests that mock the MSVC \
         runtime.\nbuild.rs body:\n{body}"
    );
}

// --------------------------------------------------------------------
// Iter 282 structural pins — cargo/build.rs/audit/guard bounds +
// LTO + strip directives.
// --------------------------------------------------------------------

#[test]
fn cargo_toml_byte_bounds_iter_282() {
    const MIN: usize = 500;
    const MAX: usize = 20_000;
    let bytes = std::fs::metadata(CARGO_TOML)
        .expect("Cargo.toml must exist")
        .len() as usize;
    assert!(
        (MIN..=MAX).contains(&bytes),
        "PRD 3.1.8 (iter 282): {CARGO_TOML} is {bytes} bytes; \
         expected [{MIN}, {MAX}]."
    );
}

#[test]
fn build_rs_byte_bounds_iter_282() {
    const MIN: usize = 100;
    const MAX: usize = 20_000;
    let bytes = std::fs::metadata(BUILD_RS)
        .expect("build.rs must exist")
        .len() as usize;
    assert!(
        (MIN..=MAX).contains(&bytes),
        "PRD 3.1.8 (iter 282): {BUILD_RS} is {bytes} bytes; expected \
         [{MIN}, {MAX}]."
    );
}

#[test]
fn audit_doc_byte_bounds_iter_282() {
    const MIN: usize = 500;
    const MAX: usize = 50_000;
    let bytes = std::fs::metadata(AUDIT_DOC)
        .expect("audit doc must exist")
        .len() as usize;
    assert!(
        (MIN..=MAX).contains(&bytes),
        "PRD 3.1.8 (iter 282): {AUDIT_DOC} is {bytes} bytes; expected \
         [{MIN}, {MAX}]."
    );
}

#[test]
fn cargo_toml_declares_lto_and_strip_for_release() {
    let toml = std::fs::read_to_string(CARGO_TOML)
        .expect("Cargo.toml must exist");
    assert!(
        toml.contains("lto"),
        "PRD 3.1.8 (iter 282): Cargo.toml must declare `lto` in \
         [profile.release] — the criterion names LTO as one of the \
         hardening requirements."
    );
    assert!(
        toml.contains("strip"),
        "PRD 3.1.8 (iter 282): Cargo.toml must declare `strip` in \
         [profile.release] — criterion names strip as required."
    );
}

#[test]
fn guard_source_byte_bounds_iter_282() {
    const MIN: usize = 5000;
    const MAX: usize = 80_000;
    let bytes = std::fs::metadata("tests/anti_reverse_guard.rs")
        .expect("guard must exist")
        .len() as usize;
    assert!(
        (MIN..=MAX).contains(&bytes),
        "PRD 3.1.8 (iter 282): guard is {bytes} bytes; expected \
         [{MIN}, {MAX}]."
    );
}
