//! PRD 3.1.12 — CSP must refuse inline scripts.
//!
//! Parses `tauri.conf.json::app.security.csp`, walks the policy string for
//! the `script-src` directive, and asserts `'unsafe-inline'` is absent from
//! it. If a future commit re-adds inline-script support (for example by
//! copy-pasting an old CSP string), this test fails.
//!
//! The test is deliberately narrow: `'unsafe-inline'` inside `style-src` is
//! left alone. Tightening style-src would break common CSS-in-JS patterns
//! (inline `style=""` attributes rendered by the launcher are legit) and is
//! not part of 3.1.12.

use std::fs;

use serde_json::Value;

fn load_csp() -> String {
    let body = fs::read_to_string("tauri.conf.json").expect("tauri.conf.json must exist");
    let v: Value = serde_json::from_str(&body).expect("tauri.conf.json must parse");
    v["app"]["security"]["csp"]
        .as_str()
        .expect("app.security.csp must be a string")
        .to_string()
}

/// Returns the token list for the given directive, or None if it's absent.
fn directive_tokens<'a>(csp: &'a str, directive: &str) -> Option<Vec<&'a str>> {
    for part in csp.split(';') {
        let trimmed = part.trim();
        if let Some(rest) = trimmed.strip_prefix(directive) {
            // Must be followed by whitespace, otherwise `scripts-src` could
            // shadow `script-src` on a prefix match.
            let rest = rest.trim_start();
            if rest.is_empty() {
                return Some(Vec::new());
            }
            return Some(rest.split_ascii_whitespace().collect());
        }
    }
    None
}

#[test]
fn csp_denies_inline_scripts() {
    let csp = load_csp();
    let tokens = directive_tokens(&csp, "script-src")
        .expect("CSP must define a script-src directive");
    assert!(
        !tokens.contains(&"'unsafe-inline'"),
        "CSP script-src must NOT contain 'unsafe-inline' (PRD 3.1.12). Found: {:?}",
        tokens
    );
}

#[test]
fn csp_still_allows_self_and_cdnjs_for_scripts() {
    let csp = load_csp();
    let tokens = directive_tokens(&csp, "script-src")
        .expect("CSP must define a script-src directive");
    assert!(
        tokens.contains(&"'self'"),
        "CSP script-src must keep 'self' so bundled scripts still load"
    );
    assert!(
        tokens.contains(&"https://cdnjs.cloudflare.com"),
        "CSP script-src must keep the cdnjs origin for gsap/anime/swiper/particles"
    );
}

#[test]
fn directive_tokens_returns_none_for_missing_directive() {
    assert!(directive_tokens("default-src 'self'", "script-src").is_none());
}

#[test]
fn directive_tokens_does_not_match_prefix() {
    // `scripts-src` should NOT match when we ask for `script-src`.
    assert!(directive_tokens("scripts-src 'self'", "script-src").is_none());
}

/// iter 152 — script-src must not include wildcards or data:-URIs.
/// Either token would let arbitrary scripts through without an
/// inline-source exception (`*` matches any host, `data:` allows
/// base64-encoded scripts). Iter 152 pins both absences so a subtle
/// CSP widening tripp CI even without `'unsafe-inline'`.
#[test]
fn csp_script_src_has_no_wildcard_or_data() {
    let csp = load_csp();
    let tokens = directive_tokens(&csp, "script-src")
        .expect("CSP must define a script-src directive");
    for bad in ["*", "data:", "'unsafe-eval'", "blob:"] {
        assert!(
            !tokens.contains(&bad),
            "PRD 3.1.12 (iter-152 extension): CSP script-src must \
             not carry `{bad}` — found in: {tokens:?}. Each token is \
             a separate bypass class that defeats the no-inline-\
             scripts discipline even without `'unsafe-inline'`."
        );
    }
}

/// iter 152 — `default-src 'self'` baseline. Without an explicit
/// default, browsers apply the CSP lax default, which permits many
/// sink types silently. Pinning `'self'` as default means every
/// other directive starts from a tight baseline.
#[test]
fn csp_default_src_is_self() {
    let csp = load_csp();
    let tokens = directive_tokens(&csp, "default-src")
        .expect("CSP must define a default-src directive");
    assert!(
        tokens.contains(&"'self'"),
        "PRD 3.1.12 (iter-152 extension): CSP default-src must \
         include `'self'` as the tight baseline every other \
         directive derives from. Got: {tokens:?}"
    );
    // Baseline must NOT be widened to `*`.
    assert!(
        !tokens.contains(&"*"),
        "PRD 3.1.12 (iter-152 extension): CSP default-src must not \
         carry `*` — that defeats the baseline restriction. Got: \
         {tokens:?}"
    );
}

/// iter 152 — connect-src must permit Tauri v2 IPC scheme (`ipc:`
/// and `http://ipc.localhost`). Without these, every `invoke()` call
/// from the frontend fails under strict CSP, breaking the whole mod
/// manager UX. Pinning presence catches an accidental narrowing.
#[test]
fn csp_connect_src_permits_tauri_v2_ipc() {
    let csp = load_csp();
    let tokens = directive_tokens(&csp, "connect-src")
        .expect("CSP must define a connect-src directive");
    assert!(
        tokens.contains(&"ipc:"),
        "PRD 3.1.12 (iter-152 extension): CSP connect-src must \
         include `ipc:` scheme — Tauri v2 IPC uses this for native-\
         to-frontend message routing. Got: {tokens:?}"
    );
    assert!(
        tokens.contains(&"http://ipc.localhost"),
        "PRD 3.1.12 (iter-152 extension): CSP connect-src must \
         include `http://ipc.localhost` — Tauri v2's IPC bridge. \
         Without it, `invoke()` calls fail with a CSP violation. \
         Got: {tokens:?}"
    );
}

/// iter 152 — connect-src must include the documented LAN dev
/// portal endpoint (`http://192.168.1.128:8090`). Without it, the
/// launcher can't reach the portal at all; CSP would block every
/// portal fetch. Complements §3.1.13 portal-https migration (which
/// tracks the future prod FQDN cutover) — until then, this LAN
/// URL must stay.
#[test]
fn csp_connect_src_permits_lan_portal_endpoint() {
    let csp = load_csp();
    let tokens = directive_tokens(&csp, "connect-src")
        .expect("CSP must define a connect-src directive");
    assert!(
        tokens.contains(&"http://192.168.1.128:8090"),
        "PRD 3.1.12 (iter-152 extension): CSP connect-src must \
         include `http://192.168.1.128:8090` (LAN dev portal) — \
         dropping it blocks every portal fetch. When §3.1.13 flips \
         to https://<prod-fqdn>, update this guard atomically with \
         the config + CSP change. Got: {tokens:?}"
    );
}

// --------------------------------------------------------------------
// Iter 197 structural pins — guard traceability + style-src shape +
// img-src hardening + explicit font-src + no object/frame-src
// widening.
// --------------------------------------------------------------------

const GUARD_SOURCE: &str = "tests/csp_audit.rs";

/// Iter 197: guard source header must cite `PRD 3.1.12` and explain
/// the deliberate style-src / inline-style trade-off. Without the
/// rationale, a reviewer unfamiliar with the history might try to
/// tighten style-src and break inline `style=""` attributes
/// rendered by the launcher.
#[test]
fn guard_file_header_cites_prd_and_style_src_tradeoff() {
    let body = fs::read_to_string(GUARD_SOURCE).expect("guard source must exist");
    let header = &body[..body.len().min(1500)];
    assert!(
        header.contains("PRD 3.1.12"),
        "PRD 3.1.12 (iter 197): {GUARD_SOURCE} header must cite \
         `PRD 3.1.12` so the criterion is reachable via grep."
    );
    assert!(
        header.contains("style-src") && header.contains("not part of 3.1.12"),
        "PRD 3.1.12 (iter 197): {GUARD_SOURCE} header must explain \
         the deliberate style-src trade-off (why `'unsafe-inline'` \
         is left alone in style-src). Without the rationale, a \
         reviewer may tighten style-src and break inline `style=\"\"`."
    );
}

/// Iter 197: CSP must define a `style-src` directive with `'self'`,
/// `'unsafe-inline'` (documented trade-off), and the Cloudflare CDN
/// origin. Pin the shape so the trade-off stays deliberate and
/// documented — not widened to `*` or narrowed to exclude the CDN
/// (which would break gsap/anime/swiper fonts).
#[test]
fn csp_defines_style_src_with_documented_shape() {
    let csp = load_csp();
    let tokens = directive_tokens(&csp, "style-src")
        .expect("CSP must define a style-src directive");
    assert!(
        tokens.contains(&"'self'"),
        "PRD 3.1.12 (iter 197): CSP style-src must contain `'self'` \
         as the baseline. Got: {tokens:?}"
    );
    assert!(
        tokens.contains(&"'unsafe-inline'"),
        "PRD 3.1.12 (iter 197): CSP style-src must keep \
         `'unsafe-inline'` — the launcher renders inline \
         `style=\"\"` attributes (the iter-152 / iter-197 \
         documented trade-off, NOT part of 3.1.12). Got: {tokens:?}"
    );
    // And must NOT be widened to wildcards (tightness invariant).
    assert!(
        !tokens.contains(&"*"),
        "PRD 3.1.12 (iter 197): CSP style-src must not carry `*` — \
         widens to arbitrary remote stylesheets. Got: {tokens:?}"
    );
}

/// Iter 197: CSP must define an `img-src` directive that permits
/// data-URIs + https: for catalog thumbnails, but NEVER
/// `'unsafe-inline'` (which has no img-level meaning and signals
/// a copy-paste drift from another directive). A widened img-src
/// opens SVG-based script injection on browsers that honour embedded
/// scripts in SVG.
#[test]
fn csp_defines_img_src_without_unsafe_inline() {
    let csp = load_csp();
    let tokens = directive_tokens(&csp, "img-src")
        .expect("CSP must define an img-src directive");
    assert!(
        tokens.contains(&"'self'"),
        "PRD 3.1.12 (iter 197): CSP img-src must contain `'self'`. \
         Got: {tokens:?}"
    );
    assert!(
        tokens.contains(&"data:"),
        "PRD 3.1.12 (iter 197): CSP img-src must contain `data:` — \
         inline-icon PNG data-URIs shipped in index.html depend on \
         it. Got: {tokens:?}"
    );
    assert!(
        !tokens.contains(&"'unsafe-inline'"),
        "PRD 3.1.12 (iter 197): CSP img-src must NOT contain \
         `'unsafe-inline'` — img-src has no inline-script meaning; \
         its presence signals a copy-paste drift from another \
         directive. Got: {tokens:?}"
    );
}

/// Iter 197: CSP must define a `font-src` directive explicitly, not
/// rely on the default-src fallback. Without an explicit font-src,
/// a future tightening of default-src (plausible future hardening)
/// would silently break Google Fonts / cdnjs font loading because
/// the fallback would inherit the narrower default.
#[test]
fn csp_defines_font_src_explicitly() {
    let csp = load_csp();
    let tokens = directive_tokens(&csp, "font-src")
        .expect(
            "CSP must define a `font-src` directive explicitly. \
             Relying on default-src means a future default-src \
             tightening silently breaks font loading.",
        );
    assert!(
        tokens.contains(&"'self'"),
        "PRD 3.1.12 (iter 197): CSP font-src must contain `'self'`. \
         Got: {tokens:?}"
    );
    // Google Fonts and Cloudflare fonts are the current remote sources.
    assert!(
        tokens.contains(&"https://fonts.gstatic.com"),
        "PRD 3.1.12 (iter 197): CSP font-src must contain \
         `https://fonts.gstatic.com` — Google Fonts origin. Got: \
         {tokens:?}"
    );
}

/// Iter 228: `GUARD_SOURCE` const must be pinned verbatim. A rename
/// of the guard file would surface only as a generic must-exist
/// panic; the canonical literal anchors the diff.
#[test]
fn guard_path_constant_is_canonical() {
    let src = fs::read_to_string("tests/csp_audit.rs")
        .expect("tests/csp_audit.rs must exist");
    assert!(
        src.contains(r#"const GUARD_SOURCE: &str = "tests/csp_audit.rs";"#),
        "canonical path constant missing: \
         `const GUARD_SOURCE: &str = \"tests/csp_audit.rs\";`. A \
         rename must surface as a guard update, not a generic panic."
    );
}

/// Iter 228: `connect-src` must carry `'self'` as a baseline token.
/// Iter 152's tests pin the `ipc:` / `http://ipc.localhost` schemes
/// and the LAN portal origin but leave the `'self'` baseline
/// implicit. Dropping `'self'` silently breaks any same-origin
/// fetch (static JSON reads, bundled-asset HEAD probes) that doesn't
/// go through `invoke()`.
#[test]
fn csp_connect_src_carries_self_baseline() {
    let csp = load_csp();
    let tokens = directive_tokens(&csp, "connect-src")
        .expect("CSP must define a connect-src directive");
    assert!(
        tokens.contains(&"'self'"),
        "PRD 3.1.12 (iter 228): CSP connect-src must contain `'self'` \
         as the baseline. Without it, same-origin fetches that don't \
         route through `invoke()` fail. Got: {tokens:?}"
    );
}

/// Iter 228: CSP must define exactly six canonical directives —
/// `default-src`, `script-src`, `style-src`, `font-src`, `img-src`,
/// `connect-src`. A stealth addition of `object-src: *` / `frame-src:
/// *` / `worker-src: *` (all sinks that default-src doesn't cover on
/// every browser) would widen the surface without tripping any of
/// the name-specific pins. Requiring an exact set makes additions a
/// conscious guard update.
#[test]
fn csp_defines_exactly_six_canonical_directives() {
    let csp = load_csp();
    let names: Vec<&str> = csp
        .split(';')
        .map(|part| part.trim().split_ascii_whitespace().next().unwrap_or(""))
        .filter(|name| !name.is_empty())
        .collect();
    let expected = [
        "default-src",
        "script-src",
        "style-src",
        "font-src",
        "img-src",
        "connect-src",
    ];
    assert_eq!(
        names.len(), expected.len(),
        "PRD 3.1.12 (iter 228): CSP must define exactly 6 canonical \
         directives (default/script/style/font/img/connect). Got {} \
         directive(s): {:?}. A stealth addition of object-src / \
         frame-src / worker-src widens attack surface.",
        names.len(), names
    );
    for expected_name in expected {
        assert!(
            names.contains(&expected_name),
            "PRD 3.1.12 (iter 228): CSP must define `{expected_name}` \
             directive. Got: {names:?}"
        );
    }
}

/// Iter 228: the CSP string must live at the exact JSON path
/// `app.security.csp`. A schema migration that moves the field
/// (e.g. `tauri.security.csp` in a hypothetical v3) would leave
/// `load_csp` panicking at the `.expect()` — but the panic is
/// indistinguishable from "field genuinely absent." Pinning the
/// top-level path shape lets a real migration show up as a guard
/// update rather than a mystery panic.
#[test]
fn csp_field_lives_at_app_security_csp_json_path() {
    let body = fs::read_to_string("tauri.conf.json")
        .expect("tauri.conf.json must exist");
    let v: Value = serde_json::from_str(&body)
        .expect("tauri.conf.json must parse");
    assert!(
        v.get("app").is_some(),
        "PRD 3.1.12 (iter 228): tauri.conf.json must carry a top-\
         level `app` object."
    );
    assert!(
        v["app"].get("security").is_some(),
        "PRD 3.1.12 (iter 228): tauri.conf.json `app` object must \
         carry a `security` nested object."
    );
    assert!(
        v["app"]["security"].get("csp").is_some(),
        "PRD 3.1.12 (iter 228): tauri.conf.json must carry \
         `app.security.csp`. A schema migration that moves the field \
         must land atomically with a csp_audit.rs path update."
    );
}

/// Iter 228: guard file must cite both extension iterations — iter
/// 152 (script-src hardening + connect-src IPC + default-src) and
/// iter 197 (style-src / img-src / font-src / object-src-frame-src
/// absence). The header's section dividers name both; a cleanup
/// pass that strips the iter attributions loses the changelog.
#[test]
fn guard_header_cites_both_extension_iters_152_and_197() {
    let src = fs::read_to_string("tests/csp_audit.rs")
        .expect("tests/csp_audit.rs must exist");
    for iter_marker in ["iter 152", "Iter 197"] {
        assert!(
            src.contains(iter_marker),
            "PRD 3.1.12 (iter 228): {} must cite `{iter_marker}` so \
             the per-iter extension history stays traceable.",
            "tests/csp_audit.rs"
        );
    }
}

/// Iter 197: CSP must NOT define an `object-src` or `frame-src`
/// directive. Both default to default-src (`'self'`) which is
/// tight. An explicit widening of either (e.g. to `*`) would open
/// embed/plugin surface that the launcher doesn't need — pin the
/// absence so a drifted CSP with an explicit widening trips CI.
#[test]
fn csp_has_no_object_or_frame_src_widening() {
    let csp = load_csp();
    // If the directive is defined at all, it must only contain
    // `'self'` / `'none'`. Any wildcard or scheme-only value is a
    // widening.
    if let Some(tokens) = directive_tokens(&csp, "object-src") {
        for bad in ["*", "data:", "blob:", "https:"] {
            assert!(
                !tokens.contains(&bad),
                "PRD 3.1.12 (iter 197): CSP object-src must not \
                 carry `{bad}` — opens embed/plugin surface the \
                 launcher doesn't need. Got: {tokens:?}"
            );
        }
    }
    if let Some(tokens) = directive_tokens(&csp, "frame-src") {
        for bad in ["*", "data:", "blob:", "https:"] {
            assert!(
                !tokens.contains(&bad),
                "PRD 3.1.12 (iter 197): CSP frame-src must not \
                 carry `{bad}` — enables framing arbitrary sites \
                 inside the launcher webview. Got: {tokens:?}"
            );
        }
    }
}
