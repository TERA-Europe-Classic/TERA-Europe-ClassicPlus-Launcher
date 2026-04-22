//! sec.shell-scope-hardening (iter 86) — defence-in-depth against
//! CVE-2025-31477 regression.
//!
//! The Tauri shell-plugin `open` endpoint's default scope accepted
//! arbitrary URI schemes (`file://`, `smb://`, `nfs://` ...) before
//! plugin 2.2.1, letting any XSS-or-IPC-reachable string be shuttled
//! into the OS protocol-handler dispatcher — a reliable RCE primitive.
//! Plugin 2.3.5 (our current pin) ships the fix by default, but that
//! default is a plugin-internal choice; a future default-flip there
//! would silently re-open the hole.
//!
//! Pinning `"plugins": { "shell": { "open": true } }` in
//! `tauri.conf.json` is the Tauri 2.x advisory's recommended
//! defence-in-depth: `true` means "open only mailto:, http:, https:" —
//! an explicit allowlist shape that won't change out from under us.
//!
//! This test guards the config value. Source-inspection style (no
//! Tauri runtime spun up) matches iters 74-79 wiring guards and stays
//! cheap to run on every CI tick.

use std::fs;

const TAURI_CONF: &str = "tauri.conf.json";
const CARGO_TOML: &str = "Cargo.toml";
const GUARD_SOURCE: &str = "tests/shell_scope_pinned.rs";

fn read_conf() -> serde_json::Value {
    let body = fs::read_to_string(TAURI_CONF).expect("tauri.conf.json must exist");
    serde_json::from_str(&body).expect("tauri.conf.json must parse as JSON")
}

fn read(path: &str) -> String {
    fs::read_to_string(path).unwrap_or_else(|e| panic!("{path}: {e}"))
}

/// `plugins.shell.open` must be the JSON boolean `true`.
/// Per the Tauri 2.x shell-plugin docs, `true` restricts the `open`
/// endpoint to the safe-scheme allowlist (mailto, http, https).
/// Values that are NOT acceptable here:
///   - `false` — the endpoint is disabled (breaks our in-app link
///     opener; we use it at app.js:2261 / app.js:5025 / app.js:2259).
///   - A regex string — would admit arbitrary schemes if the regex is
///     over-broad; our current set of call sites is happy with the
///     built-in safe list.
///   - The key is missing — falls back to the plugin default, which is
///     what the advisory warns against depending on.
#[test]
fn plugins_shell_open_is_true() {
    let conf = read_conf();
    let shell_open = conf
        .pointer("/plugins/shell/open")
        .expect("plugins.shell.open must be set in tauri.conf.json");
    assert_eq!(
        shell_open,
        &serde_json::Value::Bool(true),
        "plugins.shell.open must be literally `true` (safe-scheme allowlist). \
         Got: {shell_open}. See sec.shell-scope-hardening / CVE-2025-31477."
    );
}

/// The shell plugin stanza must live under the top-level `plugins`
/// object. If someone refactors it to a Tauri-v1-shaped
/// `tauri.allowlist.shell` block (which v2 ignores), the pin would
/// silently fall back to plugin defaults. Pin the shape.
#[test]
fn shell_scope_lives_under_plugins_not_allowlist() {
    let conf = read_conf();
    assert!(
        conf.pointer("/plugins/shell").is_some(),
        "tauri.conf.json must carry `plugins.shell` (v2 shape), not the v1 \
         `tauri.allowlist.shell` block — the v1 shape is ignored by v2 and \
         the default scope would apply."
    );
    assert!(
        conf.pointer("/tauri/allowlist/shell").is_none(),
        "tauri.allowlist.shell is a v1 leftover and must not exist on v2 \
         configs — it would confuse reviewers about which stanza is \
         authoritative."
    );
}

/// The `plugins.shell` stanza must contain ONLY the `open` key.
/// Adding `execute`, `sidecar`, `scope` overrides, or any other
/// shell-plugin capability widens the attack surface beyond what
/// the PRD audit signed off on — each additional endpoint is a
/// separate OS-command-dispatch sink that needs its own review.
///
/// A strict "exactly 1 key = `open`" pin keeps the stanza
/// whitelist-only — any new key trips CI and forces a design
/// review before landing.
#[test]
fn shell_stanza_contains_only_open_key() {
    let conf = read_conf();
    let shell = conf
        .pointer("/plugins/shell")
        .and_then(|v| v.as_object())
        .expect("plugins.shell must be an object");

    let keys: Vec<&String> = shell.keys().collect();
    assert_eq!(
        keys.len(),
        1,
        "plugins.shell must contain exactly 1 key (`open`). \
         Current keys: {keys:?}. Adding `execute`, `sidecar`, a \
         `scope` override, or any other capability widens the \
         attack surface beyond what the CVE-2025-31477 audit \
         signed off on. Each additional key is a separate sink \
         that needs its own review — file a design doc before \
         re-adding."
    );
    assert_eq!(
        keys[0], "open",
        "plugins.shell's single key must be `open`. Got `{}`. \
         If the shell plugin's API changed, update this guard \
         atomically with the audit doc re-signing.",
        keys[0]
    );
}

/// A Tauri v2 `scope` block under `shell.open` would override the
/// `true` shorthand with a regex list — even an apparently-safe
/// regex like `".*"` or `"file://.*"` re-opens the CVE-2025-31477
/// hole. The strict `open: true` shorthand must stay; `scope` must
/// NOT coexist with it.
#[test]
fn shell_open_has_no_scope_override() {
    let conf = read_conf();
    assert!(
        conf.pointer("/plugins/shell/scope").is_none(),
        "plugins.shell.scope must not exist — the `open: true` \
         shorthand provides the safe-scheme allowlist (mailto, \
         http, https). Layering a `scope` regex list on top \
         silently widens the schemes allowed and re-opens \
         CVE-2025-31477."
    );
    // Belt-and-braces: the plugin-v1-shaped `open.scope` (object
    // form) also must not appear.
    let open = conf.pointer("/plugins/shell/open").unwrap();
    assert!(
        open.is_boolean(),
        "plugins.shell.open must be a boolean literal `true` — if \
         it became an object with a `scope` field, the strict \
         safe-scheme allowlist is bypassed. Got type: {}",
        if open.is_string() {
            "string"
        } else if open.is_object() {
            "object"
        } else if open.is_array() {
            "array"
        } else {
            "other"
        }
    );
}

/// Detector self-test — proves the JSON-pointer classifier rejects
/// obviously-bad shapes. If the classifier regressed to always passing,
/// the real tests above would silently accept a removed pin.
#[test]
fn detector_self_test_rejects_bad_shapes() {
    // Missing plugins.shell.open
    let bad_missing: serde_json::Value =
        serde_json::from_str(r#"{ "plugins": { "updater": {} } }"#).unwrap();
    assert!(
        bad_missing.pointer("/plugins/shell/open").is_none(),
        "self-test: detector must see the missing-pin case"
    );

    // plugins.shell.open = false (endpoint disabled — not what we want)
    let bad_false: serde_json::Value =
        serde_json::from_str(r#"{ "plugins": { "shell": { "open": false } } }"#).unwrap();
    assert_eq!(
        bad_false.pointer("/plugins/shell/open").unwrap(),
        &serde_json::Value::Bool(false),
        "self-test: detector must distinguish true from false"
    );

    // plugins.shell.open = "^https?:" (regex — admits custom schemes)
    let bad_regex: serde_json::Value =
        serde_json::from_str(r#"{ "plugins": { "shell": { "open": "^https?:" } } }"#).unwrap();
    assert!(
        bad_regex
            .pointer("/plugins/shell/open")
            .unwrap()
            .is_string(),
        "self-test: detector must see the regex-string case as non-boolean"
    );

    // Bad shape D (iter 148): shell stanza with extra keys.
    let bad_extra: serde_json::Value = serde_json::from_str(
        r#"{ "plugins": { "shell": { "open": true, "execute": { "scope": [{ "name": "test" }] } } } }"#,
    )
    .unwrap();
    let shell_extra = bad_extra
        .pointer("/plugins/shell")
        .and_then(|v| v.as_object())
        .unwrap();
    assert!(
        shell_extra.len() > 1,
        "self-test: shell stanza with extra keys must have >1 key"
    );

    // Bad shape E: open becomes an object with a scope regex list.
    let bad_scope: serde_json::Value =
        serde_json::from_str(r#"{ "plugins": { "shell": { "open": { "scope": [".*"] } } } }"#)
            .unwrap();
    let open_obj = bad_scope.pointer("/plugins/shell/open").unwrap();
    assert!(
        open_obj.is_object(),
        "self-test: object-form open must not be boolean"
    );

    // Bad shape F: scope subkey appears sibling to open.
    let bad_sibling_scope: serde_json::Value = serde_json::from_str(
        r#"{ "plugins": { "shell": { "open": true, "scope": [{ "name": "x", "args": true }] } } }"#,
    )
    .unwrap();
    assert!(
        bad_sibling_scope.pointer("/plugins/shell/scope").is_some(),
        "self-test: sibling scope block must be detected"
    );

    // Iter 188 — additional bad shapes.

    // Bad shape G: CSP connect-src admitting file: scheme.
    let dangerous_csp = "connect-src 'self' file: https://example.com";
    assert!(
        dangerous_csp.contains("file:"),
        "self-test: dangerous CSP scheme must be flagged"
    );

    // Bad shape H: plugins object grows an unexpected key.
    let bad_plugins: serde_json::Value = serde_json::from_str(
        r#"{ "plugins": { "shell": {"open": true}, "updater": {}, "process": { "exec": ["*"] } } }"#,
    )
    .unwrap();
    let plugins_obj = bad_plugins
        .pointer("/plugins")
        .and_then(|v| v.as_object())
        .unwrap();
    assert!(
        plugins_obj.len() > 2,
        "self-test: plugins object with unexpected key must be flagged by count"
    );

    // Bad shape I: window URL set to a remote endpoint.
    let remote_url: serde_json::Value =
        serde_json::from_str(r#"{ "app": { "windows": [{ "url": "https://example.com" }] } }"#)
            .unwrap();
    let url = remote_url
        .pointer("/app/windows/0/url")
        .unwrap()
        .as_str()
        .unwrap();
    assert!(
        url.starts_with("http"),
        "self-test: remote window URL must be flagged"
    );
}

/// Iter 188: guard file header must cite `CVE-2025-31477` so a
/// future reader understands what this pin protects against. Without
/// the citation, a maintainer unfamiliar with the history might
/// relax a pin to match a "simpler" config and silently reintroduce
/// the regression.
#[test]
fn guard_file_header_cites_cve() {
    let body = read(GUARD_SOURCE);
    // Take first 1500 chars (the `//!` module comment block).
    let header = &body[..body.len().min(1500)];
    assert!(
        header.contains("CVE-2025-31477"),
        "sec.shell-scope-hardening (iter 188): {GUARD_SOURCE} header \
         must cite `CVE-2025-31477`. Without it, a maintainer might \
         relax a pin to match a simpler default config and silently \
         reintroduce the regression."
    );
    assert!(
        header.contains("sec.shell-scope-hardening"),
        "sec.shell-scope-hardening (iter 188): {GUARD_SOURCE} header \
         must cite `sec.shell-scope-hardening` so the fix-plan P-slot \
         is reachable via grep."
    );
}

/// Iter 188: `Cargo.toml` must keep the `tauri-plugin-shell`
/// dependency. If someone drops the dep while keeping the
/// `plugins.shell.open: true` config block, the config becomes a
/// silent no-op (the plugin isn't loaded). The config-level pin
/// would still pass; this real-file pin catches the drift.
#[test]
fn cargo_toml_keeps_tauri_plugin_shell_dep() {
    let cargo = read(CARGO_TOML);
    assert!(
        cargo.contains("tauri-plugin-shell"),
        "sec.shell-scope-hardening (iter 188): {CARGO_TOML} must \
         carry the `tauri-plugin-shell` dependency. Dropping it \
         while keeping the `plugins.shell.open: true` config in \
         tauri.conf.json silently disables shell.open — the feature \
         is broken in production but the config-level pin still \
         passes."
    );
    // The major version line must still be `= "2"` (Tauri v2 line).
    // A v3 bump is a real event that should break the build explicitly,
    // not silently migrate.
    assert!(
        cargo.contains("tauri-plugin-shell = \"2\"")
            || cargo.contains("tauri-plugin-shell =\"2\"")
            || cargo.contains("tauri-plugin-shell = { version = \"2"),
        "sec.shell-scope-hardening (iter 188): {CARGO_TOML} must \
         pin `tauri-plugin-shell` to the `\"2\"` major. A v3 bump \
         should be deliberate, not implicit — the shell-plugin API \
         and default scope could change."
    );
}

/// Iter 188: the `app.security.csp` must not admit `file:`, `smb:`,
/// `nfs:`, or `javascript:` schemes in any directive. Even if
/// shell.open stays locked, a CSP that allows those schemes in
/// connect-src or script-src would let injected content reach them
/// through other APIs (fetch, a tag hrefs).
#[test]
fn app_security_csp_excludes_dangerous_schemes() {
    let conf = read_conf();
    let csp = conf
        .pointer("/app/security/csp")
        .and_then(|v| v.as_str())
        .expect("app.security.csp must be a string");
    for dangerous in ["file:", "smb:", "nfs:", "javascript:"] {
        assert!(
            !csp.contains(dangerous),
            "sec.shell-scope-hardening (iter 188): app.security.csp \
             must not admit scheme `{dangerous}`. A CSP that allows \
             it lets injected content reach dangerous handlers even \
             if shell.open stays locked down.\nCSP: {csp}"
        );
    }
}

/// Iter 188: the `plugins` object must only contain the expected
/// pair `updater` + `shell`. Adding new plugin stanzas here (e.g.
/// `fs`, `os`, `process` with a widened allowlist) would expand the
/// attack surface — each new plugin is a fresh capability-review
/// opportunity that should fail CI until the reviewer looks.
#[test]
fn plugins_object_contains_only_expected_keys() {
    let conf = read_conf();
    let plugins = conf
        .pointer("/plugins")
        .and_then(|v| v.as_object())
        .expect("tauri.conf.json must carry a `plugins` object");
    let mut keys: Vec<&String> = plugins.keys().collect();
    keys.sort();
    let actual: Vec<&str> = keys.iter().map(|s| s.as_str()).collect();
    assert_eq!(
        actual,
        vec!["shell", "updater"],
        "sec.shell-scope-hardening (iter 188): tauri.conf.json \
         `plugins` must contain exactly the pair \
         `{{shell, updater}}`. Found {actual:?}. Adding a new plugin \
         stanza expands the capability surface; each addition must \
         be reviewed and this pin updated atomically."
    );
}

/// Iter 188: the main window's `url` must be a relative local file,
/// not a remote http/https URL. A remote URL would load JS from the
/// network into the Tauri webview — that JS can call `shell.open`,
/// and even though scheme-filtering stays in place, attacker-
/// controlled JS inside the trusted origin bypasses the whole
/// defence model.
#[test]
fn main_window_url_is_local_not_remote() {
    let conf = read_conf();
    let windows = conf
        .pointer("/app/windows")
        .and_then(|v| v.as_array())
        .expect("app.windows must be an array");
    assert!(
        !windows.is_empty(),
        "sec.shell-scope-hardening (iter 188): tauri.conf.json must \
         declare at least one window under app.windows."
    );
    let url = windows[0]
        .pointer("/url")
        .and_then(|v| v.as_str())
        .expect("app.windows[0].url must be a string");
    assert!(
        !url.starts_with("http://") && !url.starts_with("https://"),
        "sec.shell-scope-hardening (iter 188): app.windows[0].url \
         must be a relative local file (e.g. `index.html`), not a \
         remote URL. Got `{url}`. A remote URL loads JS from the \
         network into the Tauri webview — that JS can call \
         shell.open within our allowlisted scheme, bypassing the \
         whole CVE-2025-31477 defence model."
    );
}

// --------------------------------------------------------------------
// Iter 206 structural pins — capabilities/migrated.json permission
// inventory + CSP positive-directive hardening.
// --------------------------------------------------------------------
//
// The ten pins above lock down `tauri.conf.json` (shell stanza shape,
// plugin allowlist, CSP negative scheme list, window URL). They do
// NOT pin the adjacent capability file nor the CSP's POSITIVE
// strictness: (a) `capabilities/migrated.json` carries the runtime
// permission grants — if `shell:allow-open` gets widened to
// `shell:default` (everything), the tauri.conf.json-level shape pins
// stay green but runtime execute/sidecar/spawn become reachable;
// (b) the `shell:allow-execute` grant MUST carry a validator — a
// future refactor that drops `"validator": "\\S+"` would admit
// arbitrary whitespace-containing arguments; (c) CSP's fallback
// `default-src 'self'` must be present (positive strictness — the
// negative-only iter-188 pin lets a CSP like `default-src *` pass);
// (d) no CSP directive may contain `'unsafe-eval'` (amplifies XSS
// into RCE); (e) script-src must not carry blanket wildcard origins
// (`*` or bare `https:`) which would nullify the allowlist.

const CAPABILITIES_JSON: &str = "capabilities/migrated.json";

/// `capabilities/migrated.json` must grant shell access via the
/// narrow `shell:allow-open` permission only — not the wide
/// `shell:default` or any `shell:allow-spawn` / `shell:allow-kill`
/// grants. Widening the runtime permission set would re-open
/// attack surfaces even if tauri.conf.json shape pins stay green.
#[test]
fn capabilities_shell_permissions_are_narrow() {
    let body = read(CAPABILITIES_JSON);
    let conf: serde_json::Value =
        serde_json::from_str(&body).expect("capabilities/migrated.json must parse as JSON");
    let perms = conf
        .pointer("/permissions")
        .and_then(|v| v.as_array())
        .expect("capabilities/migrated.json must carry a `permissions` array");

    // Collect permission identifier strings (either bare strings or
    // objects with an `identifier` field).
    let identifiers: Vec<String> = perms
        .iter()
        .filter_map(|p| {
            p.as_str().map(String::from).or_else(|| {
                p.pointer("/identifier")
                    .and_then(|v| v.as_str())
                    .map(String::from)
            })
        })
        .collect();

    let has_allow_open = identifiers.iter().any(|s| s == "shell:allow-open");
    assert!(
        has_allow_open,
        "sec.shell-scope-hardening (iter 206): \
         capabilities/migrated.json must include `shell:allow-open` — \
         the launcher uses `shell.open` at app.js call sites and \
         requires the runtime permission grant. \
         Identifiers: {identifiers:?}"
    );

    // Negative: these broader grants must NOT appear.
    for forbidden in ["shell:default", "shell:allow-spawn", "shell:allow-kill"] {
        assert!(
            !identifiers.iter().any(|s| s == forbidden),
            "sec.shell-scope-hardening (iter 206): \
             capabilities/migrated.json must NOT grant `{forbidden}` \
             — widens the runtime shell capability surface beyond \
             what CVE-2025-31477 audit signed off on. Identifiers: \
             {identifiers:?}"
        );
    }
}

/// If `shell:allow-execute` is present (currently used for the
/// `cmd /C start <url>` open-url shim on Windows), its allow entry
/// must carry a `validator` field. Without it the `{"validator": ...}`
/// placeholder accepts arbitrary argument strings and the single
/// cmd invocation becomes a generic launcher.
#[test]
fn capabilities_shell_execute_carries_validator() {
    let body = read(CAPABILITIES_JSON);
    let conf: serde_json::Value =
        serde_json::from_str(&body).expect("capabilities/migrated.json must parse as JSON");
    let perms = conf
        .pointer("/permissions")
        .and_then(|v| v.as_array())
        .expect("capabilities/migrated.json must carry a `permissions` array");

    // Find the shell:allow-execute entry (if any).
    let execute_entry = perms.iter().find(|p| {
        p.pointer("/identifier")
            .and_then(|v| v.as_str())
            .map(|s| s == "shell:allow-execute")
            .unwrap_or(false)
    });

    if let Some(entry) = execute_entry {
        let allow = entry
            .pointer("/allow")
            .and_then(|v| v.as_array())
            .expect("shell:allow-execute must carry an `allow` array");
        assert!(
            !allow.is_empty(),
            "sec.shell-scope-hardening (iter 206): \
             shell:allow-execute grant must carry at least one \
             allow entry (otherwise the grant is redundant and \
             should be removed)."
        );
        // Each allow entry must include at least one argument-position
        // validator, i.e. an args slot shaped `{"validator": "..."}`.
        let serialised = serde_json::to_string(allow).unwrap_or_default();
        assert!(
            serialised.contains("\"validator\""),
            "sec.shell-scope-hardening (iter 206): \
             shell:allow-execute `allow` entries must carry a \
             `validator` field on variable argument slots. Without \
             it, `\"cmd /C start <arbitrary>`\" becomes a generic \
             process launcher.\nEntries: {serialised}"
        );
    }
    // If execute isn't granted at all, the negative `shell_permissions_are_narrow`
    // pin covers the "no widening" invariant — this test becomes a no-op.
}

/// The CSP's strict fallback `default-src 'self'` must be present.
/// The iter-188 pin checks NEGATIVE (no `file:` / `smb:` / `nfs:` /
/// `javascript:`), but a CSP like `default-src *` would pass that
/// check and still disable the whole sandbox. Pin the positive
/// strict fallback directly.
#[test]
fn csp_has_strict_default_src_self() {
    let conf = read_conf();
    let csp = conf
        .pointer("/app/security/csp")
        .and_then(|v| v.as_str())
        .expect("app.security.csp must be a string");
    assert!(
        csp.contains("default-src 'self'"),
        "sec.shell-scope-hardening (iter 206): app.security.csp must \
         contain `default-src 'self'` as the strict fallback — without \
         it, directives not explicitly listed fall back to the UA's \
         default, which on some engines is `*`.\nCSP: {csp}"
    );
}

/// No CSP directive may contain `'unsafe-eval'`. `unsafe-eval` in
/// script-src (or `default-src` acting as its fallback) lets
/// dynamic code execution — it turns a read-only XSS (e.g. a
/// reflected string rendered via innerHTML) into arbitrary JS
/// execution. The CVE-2025-31477 defence model assumes untrusted
/// JS can't execute arbitrary new code inside the webview.
#[test]
fn csp_rejects_unsafe_eval_everywhere() {
    let conf = read_conf();
    let csp = conf
        .pointer("/app/security/csp")
        .and_then(|v| v.as_str())
        .expect("app.security.csp must be a string");
    assert!(
        !csp.contains("'unsafe-eval'"),
        "sec.shell-scope-hardening (iter 206): app.security.csp must \
         not contain `'unsafe-eval'` in any directive. It turns a \
         read-only XSS into arbitrary code execution inside the \
         webview, defeating the CVE-2025-31477 defence model.\n\
         CSP: {csp}"
    );
}

/// `script-src` must not carry blanket wildcard origins — neither
/// bare `*` nor a scheme-only `https:` token (which matches every
/// HTTPS origin). Either would defeat the allowlist: the current
/// `script-src 'self' https://cdnjs.cloudflare.com` locks script
/// loading to two specific origins, and that strictness is the
/// load-bearing invariant.
#[test]
fn csp_script_src_does_not_wildcard_origins() {
    let conf = read_conf();
    let csp = conf
        .pointer("/app/security/csp")
        .and_then(|v| v.as_str())
        .expect("app.security.csp must be a string");

    // Extract the script-src directive (up to the next `;` or end).
    let script_src_pos = csp
        .find("script-src")
        .expect("CSP must declare a `script-src` directive");
    let rest = &csp[script_src_pos..];
    let end = rest.find(';').unwrap_or(rest.len());
    let directive = &rest[..end];

    // Reject bare `*` (surrounded by whitespace or at end — must not
    // match `*.foo.com` wildcards or `wasm-unsafe-eval`).
    for token in directive.split_whitespace() {
        assert_ne!(
            token, "*",
            "sec.shell-scope-hardening (iter 206): script-src must \
             not contain bare `*` — it allows loading JS from any \
             origin, defeating the allowlist.\nDirective: {directive}"
        );
        // Scheme-only `https:` (with trailing colon, no host) is a
        // blanket-every-https-origin wildcard.
        assert_ne!(
            token, "https:",
            "sec.shell-scope-hardening (iter 206): script-src must \
             not contain scheme-only `https:` — matches every \
             HTTPS origin (any CDN, attacker-controlled or not). \
             Use explicit `https://host.tld` entries.\n\
             Directive: {directive}"
        );
    }
}

// --------------------------------------------------------------------
// Iter 240 structural pins — path-constants canonicalisation,
// unsafe-hashes CSP3 rejection, default-src scheme hardening,
// capabilities list cardinality, guard-source header traceability.
//
// Prior iters pinned shell.open literal + stanza cardinality + scope
// absence + 8 CSP directives + unsafe-eval absence + script-src
// wildcard rejection. These five extend to the meta-guard + CSP3-
// evolution surface a confident refactor could still bypass: a
// path-constant drift, a future CSP3 `'unsafe-hashes'` token, a
// default-src `data:/blob:/filesystem:` scheme opt-in that widens
// fetch targets, a capability-array growth that slips unreviewed
// permissions in, and a guard file without a PRD citation in the
// header.
// --------------------------------------------------------------------

/// Iter 240: all 4 path constants must stay canonical. Every
/// read_conf()/read_cargo()/read_guard/read_capabilities call in
/// this guard resolves through one of these; a rename without
/// updating the constant panics tests with "file not found",
/// misdirecting triage.
#[test]
fn guard_path_constants_are_canonical() {
    let body = fs::read_to_string(GUARD_SOURCE).expect("guard source must exist");
    for (name, expected) in [
        ("TAURI_CONF", "tauri.conf.json"),
        ("CARGO_TOML", "Cargo.toml"),
        ("GUARD_SOURCE", "tests/shell_scope_pinned.rs"),
        ("CAPABILITIES_JSON", "capabilities/migrated.json"),
    ] {
        let line = format!("const {name}: &str = \"{expected}\";");
        assert!(
            body.contains(&line),
            "sec.shell-scope-hardening (iter 240): \
             tests/shell_scope_pinned.rs must keep `{line}` \
             verbatim. A rename without updating the constant \
             silently disables every pin reading through it."
        );
    }
}

/// Iter 240: CSP must not contain `'unsafe-hashes'` anywhere.
/// CSP3 introduces `'unsafe-hashes'` as a narrower form of
/// `'unsafe-inline'` for event handlers (onclick=...) and
/// javascript: URLs. It's strictly less bad than `'unsafe-inline'`,
/// but it still re-opens the inline-handler sink — a reflected-XSS
/// into an attribute context becomes executable. Iter-206 already
/// pins `'unsafe-eval'` absence; this adds the CSP3 sibling.
#[test]
fn csp_rejects_unsafe_hashes_everywhere() {
    let conf = read_conf();
    let csp = conf
        .pointer("/app/security/csp")
        .and_then(|v| v.as_str())
        .expect("app.security.csp must be a string");
    assert!(
        !csp.contains("'unsafe-hashes'"),
        "sec.shell-scope-hardening (iter 240): app.security.csp must \
         not contain `'unsafe-hashes'` in any directive. CSP3 \
         `'unsafe-hashes'` narrows `'unsafe-inline'` but still \
         re-opens the inline-event-handler sink (onclick=..., \
         javascript: URLs). Our hard-line is no inline execution.\n\
         CSP: {csp}"
    );
}

/// Iter 240: `default-src` must NOT include opaque or data-bearing
/// schemes (`data:`, `blob:`, `filesystem:`). These schemes allow
/// execution or fetch of inline/dynamic content — `data:` for
/// arbitrary Base64-encoded HTML/JS, `blob:` for runtime-constructed
/// URL objects, `filesystem:` for HTML5 FileSystem API. Each
/// defeats the allowlist for the default-src fallback directives
/// (script-src inherits if unset; fetch-equivalent inherits).
#[test]
fn csp_default_src_rejects_opaque_schemes() {
    let conf = read_conf();
    let csp = conf
        .pointer("/app/security/csp")
        .and_then(|v| v.as_str())
        .expect("app.security.csp must be a string");
    let default_pos = csp
        .find("default-src")
        .expect("CSP must declare default-src");
    let rest = &csp[default_pos..];
    let end = rest.find(';').unwrap_or(rest.len());
    let directive = &rest[..end];
    for bad in ["data:", "blob:", "filesystem:"] {
        assert!(
            !directive.split_whitespace().any(|t| t == bad),
            "sec.shell-scope-hardening (iter 240): default-src must \
             not include `{bad}` — the scheme allows opaque / \
             runtime-constructed content that defeats the allowlist \
             for any directive inheriting default-src.\n\
             Directive: {directive}"
        );
    }
    // img-src is allowed to have data: (legitimate inline icons)
    // but ONLY explicitly in img-src, never via default-src
    // inheritance. This positive pin documents the allowed
    // exception.
    let img_pos = csp.find("img-src").expect("CSP must declare img-src");
    let img_rest = &csp[img_pos..];
    let img_end = img_rest.find(';').unwrap_or(img_rest.len());
    let img_directive = &img_rest[..img_end];
    assert!(
        img_directive.contains("data:"),
        "sec.shell-scope-hardening (iter 240): img-src must \
         explicitly carry `data:` (iter 206 invariant). If it's \
         missing here, either CSP has been narrowed (good) or \
         img-src is falling back to default-src (bad — no inline \
         icons would render, and the CSP narrows the allowlist).\n\
         img-src directive: {img_directive}"
    );
}

/// Iter 240: the `permissions` array in `capabilities/migrated.json`
/// must stay bounded. Every entry is a reviewed capability grant;
/// silent growth past a reasonable floor means a refactor added
/// permissions without a PRD audit trail. Pin a soft upper bound
/// that catches a 2×+ growth — raising the ceiling requires a
/// deliberate test update.
#[test]
fn capabilities_permissions_count_stays_bounded() {
    let body =
        fs::read_to_string(CAPABILITIES_JSON).expect("capabilities/migrated.json must exist");
    let cap: serde_json::Value = serde_json::from_str(&body).expect("capability JSON must parse");
    let perms = cap
        .pointer("/permissions")
        .and_then(|v| v.as_array())
        .expect("capability must carry a permissions array");
    // Current set at iter 240 is small (< 25 entries). Pin at 30 so
    // adding 2-3 new capabilities fits without churn, but a doubling
    // (e.g. from 20 to 45) trips CI and forces an audit.
    assert!(
        perms.len() < 30,
        "sec.shell-scope-hardening (iter 240): \
         capabilities/migrated.json's `permissions` array has grown \
         to {} entries — above the soft ceiling of 30. Each entry \
         is a reviewed capability grant; this growth indicates \
         permissions may have landed without an audit. Re-run the \
         capability audit, then raise this ceiling if the new set \
         is legitimate.",
        perms.len()
    );
    // Lower floor: any drop below 5 means critical permissions
    // (http:default, shell:default-deny, process:allow-restart) were
    // deleted — something's broken.
    assert!(
        perms.len() >= 5,
        "sec.shell-scope-hardening (iter 240): \
         capabilities/migrated.json's `permissions` array has \
         shrunk to {} entries — below the floor of 5. Critical \
         capabilities (http:default, shell:default-deny, process: \
         allow-restart, updater:default, ...) appear to have been \
         deleted. The launcher will fail at runtime with permission \
         errors.",
        perms.len()
    );
}

/// Iter 240: the guard source header must cite `sec.shell-scope-
/// hardening` + `CVE-2025-31477` so a reader tracing a CI failure
/// lands on both the fix-plan slot and the specific advisory the
/// pins defend against. Header-inspection pin parallels iter-199's
/// http_redirect_offlist traceability.
#[test]
fn guard_file_header_cites_fix_slot_and_cve() {
    let body = fs::read_to_string(GUARD_SOURCE).expect("guard source must exist");
    let header = &body[..body.len().min(2000)];
    assert!(
        header.contains("sec.shell-scope-hardening") || header.contains("shell-scope-hardening"),
        "sec.shell-scope-hardening (iter 240): \
         tests/shell_scope_pinned.rs header must cite \
         `sec.shell-scope-hardening` (the fix-plan slot) so the PRD \
         trail is reachable via grep. Without it, a CI failure \
         triggers an anonymous red with no pointer to the criterion."
    );
    assert!(
        header.contains("CVE-2025-31477"),
        "sec.shell-scope-hardening (iter 240): \
         tests/shell_scope_pinned.rs header must cite \
         `CVE-2025-31477` so the underlying advisory is reachable \
         via grep. Without the CVE reference, a maintainer might \
         relax a pin thinking the defensive posture is theoretical."
    );
}

// --------------------------------------------------------------------
// Iter 277 structural pins — tauri.conf/cargo/capabilities/guard bounds
// + iter-86 provenance.
// --------------------------------------------------------------------

#[test]
fn tauri_conf_byte_bounds() {
    const MIN: usize = 500;
    const MAX: usize = 30_000;
    let bytes = std::fs::metadata(TAURI_CONF)
        .expect("tauri.conf.json must exist")
        .len() as usize;
    assert!(
        (MIN..=MAX).contains(&bytes),
        "sec.shell-scope-hardening (iter 277): {TAURI_CONF} is {bytes} \
         bytes; expected [{MIN}, {MAX}]."
    );
}

#[test]
fn cargo_toml_byte_bounds() {
    const MIN: usize = 500;
    const MAX: usize = 20_000;
    let bytes = std::fs::metadata(CARGO_TOML)
        .expect("Cargo.toml must exist")
        .len() as usize;
    assert!(
        (MIN..=MAX).contains(&bytes),
        "sec.shell-scope-hardening (iter 277): {CARGO_TOML} is {bytes} \
         bytes; expected [{MIN}, {MAX}]."
    );
}

#[test]
fn capabilities_json_byte_bounds() {
    const MIN: usize = 200;
    const MAX: usize = 20_000;
    let bytes = std::fs::metadata(CAPABILITIES_JSON)
        .expect("capabilities/migrated.json must exist")
        .len() as usize;
    assert!(
        (MIN..=MAX).contains(&bytes),
        "sec.shell-scope-hardening (iter 277): {CAPABILITIES_JSON} is \
         {bytes} bytes; expected [{MIN}, {MAX}]."
    );
}

#[test]
fn guard_source_byte_bounds() {
    const MIN: usize = 5000;
    const MAX: usize = 80_000;
    let bytes = std::fs::metadata(GUARD_SOURCE)
        .expect("guard must exist")
        .len() as usize;
    assert!(
        (MIN..=MAX).contains(&bytes),
        "sec.shell-scope-hardening (iter 277): guard is {bytes} bytes; \
         expected [{MIN}, {MAX}]."
    );
}

#[test]
fn guard_header_cites_iter_86_provenance() {
    let body = std::fs::read_to_string(GUARD_SOURCE).expect("guard must exist");
    let header = &body[..body.len().min(500)];
    assert!(
        header.contains("iter 86"),
        "sec.shell-scope-hardening (iter 277): guard header must cite \
         `iter 86` as the iteration that shipped the hardening.\n\
         Header:\n{header}"
    );
}
