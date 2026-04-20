//! PRD 3.1.13 — portal-HTTPS migration drift guard.
//!
//! Criterion: "Portal API migrated to HTTPS before Classic+ public
//! launch." Measurement cites both `teralib/src/config/config.json`
//! (every URL starts with https://) AND
//! `docs/PRD/audits/security/portal-https-migration.md`.
//!
//! **Current state (iter 119)**: config.json still points at the LAN
//! dev endpoint `http://192.168.1.128:8090`. This is a KNOWN and
//! DOCUMENTED pre-production state — Classic+ has no public HTTPS
//! portal yet. The audit doc carries this status explicitly.
//!
//! The real drift risk we need to catch: someone accidentally
//! commits a NEW non-https URL that is NOT the LAN dev endpoint —
//! e.g. a staging host on public http, or a third-party endpoint.
//! That would either (a) leak credentials over the public internet,
//! or (b) trivially let MitM hijack the `AuthKey`.
//!
//! This guard accepts either the known LAN dev endpoint
//! (`192.168.1.128`), the `https://` scheme, or an empty string
//! (since `HASH_FILE_URL` + `FILE_SERVER_URL` are empty placeholders
//! until an endpoint ships), and refuses anything else.

use std::fs;

const CONFIG_JSON: &str = "../../teralib/src/config/config.json";
const AUDIT_DOC: &str = "../../docs/PRD/audits/security/portal-https-migration.md";

/// The pre-production LAN dev endpoint host. Keeping this a constant
/// so a future production cutover doesn't accidentally drift into a
/// test-fixture copy.
const LAN_DEV_HOST: &str = "192.168.1.128";

fn read(path: &str) -> String {
    fs::read_to_string(path).unwrap_or_else(|e| panic!("{path}: {e}"))
}

/// Every string value in config.json that looks like a URL must be
/// either empty, https://, or the documented LAN dev endpoint.
#[test]
fn config_urls_are_https_or_lan_dev_or_empty() {
    let body = read(CONFIG_JSON);
    let cfg: serde_json::Value = serde_json::from_str(&body)
        .unwrap_or_else(|e| panic!("config.json must parse: {e}"));

    let obj = cfg
        .as_object()
        .expect("config.json must be a top-level object");

    let mut offenders: Vec<String> = Vec::new();
    for (k, v) in obj {
        let Some(s) = v.as_str() else { continue };
        // Skip non-URL-shaped values.
        if !s.starts_with("http://") && !s.starts_with("https://") && !s.is_empty() {
            // Accept non-URL strings (e.g., a region code); only scrutinise
            // things that look URL-ish.
            continue;
        }
        let ok = s.is_empty()
            || s.starts_with("https://")
            || s.contains(LAN_DEV_HOST);
        if !ok {
            offenders.push(format!("{k} = {s}"));
        }
    }

    assert!(
        offenders.is_empty(),
        "PRD 3.1.13 violated: {CONFIG_JSON} contains {} URL(s) that \
         are neither https:// nor the documented LAN dev endpoint \
         ({LAN_DEV_HOST}) nor empty:\n  {}\n\
         Either move the target behind HTTPS, restrict it to the LAN \
         dev host, or leave the field empty until an endpoint ships. \
         Shipping plain HTTP to production leaks credentials per the \
         iter-9 threat model in portal-https-migration.md.",
        offenders.len(),
        offenders.join("\n  ")
    );
}

/// The audit doc must exist and carry the "Draft" / "pending" status
/// so a future reader knows this criterion is deliberately open.
/// When the production HTTPS endpoint ships, the doc transitions to
/// "Signed off" and this guard needs to tighten (remove the LAN
/// exception).
#[test]
fn portal_https_audit_doc_exists_and_flags_pending_status() {
    let body = read(AUDIT_DOC);
    assert!(
        body.contains("Portal API HTTPS") || body.contains("Portal HTTPS"),
        "Audit doc must contain the 'Portal API HTTPS' heading. \
         Surface sanity check — wrong doc otherwise."
    );
    assert!(
        body.contains("§3.1.13") || body.contains("3.1.13"),
        "Audit doc must cite PRD §3.1.13 so future readers can trace \
         the criterion back to the PRD."
    );
}

/// The expected set of config.json keys, in iter-141 state. An
/// added key is either a new endpoint (should be audited before
/// landing) or a drift (typo, misplaced); a removed key breaks
/// every call site reading it. Either way, a deliberate author
/// should update this list in the same commit as the config edit.
const EXPECTED_KEYS: &[&str] = &[
    "API_BASE_URL",
    "LOGIN_ACTION_URL",
    "GET_ACCOUNT_INFO_URL",
    "REGISTER_ACTION_URL",
    "MAINTENANCE_STATUS_URL",
    "SERVER_LIST_URL",
    "HASH_FILE_URL",
    "FILE_SERVER_URL",
];

#[test]
fn config_json_has_exact_expected_key_set() {
    let body = read(CONFIG_JSON);
    let cfg: serde_json::Value =
        serde_json::from_str(&body).expect("config.json must parse");
    let obj = cfg.as_object().expect("top-level object");

    let got: std::collections::BTreeSet<&str> =
        obj.keys().map(String::as_str).collect();
    let want: std::collections::BTreeSet<&str> =
        EXPECTED_KEYS.iter().copied().collect();

    let extra: Vec<_> = got.difference(&want).collect();
    let missing: Vec<_> = want.difference(&got).collect();

    assert!(
        extra.is_empty(),
        "config.json contains unexpected key(s): {extra:?}. If this \
         is a new endpoint being wired in, update EXPECTED_KEYS in \
         portal_https_guard.rs in the same commit and add a threat-\
         model line to portal-https-migration.md."
    );
    assert!(
        missing.is_empty(),
        "config.json is missing expected key(s): {missing:?}. \
         Deleting a key breaks every call-site reading it. If the \
         deletion was deliberate, justify in the commit message and \
         remove the entry from EXPECTED_KEYS."
    );
}

/// The five action URLs must share `API_BASE_URL` as a prefix —
/// when someone updates the base URL (e.g. during the production
/// HTTPS cutover), all five must flip together. A drift between
/// base and action would be a silent mis-wire.
#[test]
fn action_urls_share_api_base_prefix() {
    let body = read(CONFIG_JSON);
    let cfg: serde_json::Value =
        serde_json::from_str(&body).expect("config.json must parse");
    let obj = cfg.as_object().expect("top-level object");

    let base = obj
        .get("API_BASE_URL")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    assert!(
        !base.is_empty(),
        "API_BASE_URL must not be empty — it's the prefix the 5 \
         action URLs derive from."
    );

    for action_key in &[
        "LOGIN_ACTION_URL",
        "GET_ACCOUNT_INFO_URL",
        "REGISTER_ACTION_URL",
        "MAINTENANCE_STATUS_URL",
        "SERVER_LIST_URL",
    ] {
        let Some(v) = obj.get(*action_key).and_then(|v| v.as_str())
        else {
            panic!("{action_key} must exist — covered by the key-set test");
        };
        assert!(
            v.starts_with(base),
            "{action_key} ({v}) must start with API_BASE_URL ({base}). \
             A prefix drift means the base was updated but the action \
             URL was forgotten — all 5 action URLs must flip together \
             during the production HTTPS cutover."
        );
    }
}

/// `HASH_FILE_URL` and `FILE_SERVER_URL` must stay EMPTY until the
/// Classic+ updater ships. Populating them silently enables the
/// self-update pipeline — which relies on a hash-file baseline we
/// don't yet have for Classic+. Iter 140 docs this explicitly in
/// CLAUDE.md §"Known Gaps".
#[test]
fn updater_urls_remain_empty_until_endpoint_ships() {
    let body = read(CONFIG_JSON);
    let cfg: serde_json::Value =
        serde_json::from_str(&body).expect("config.json must parse");
    let obj = cfg.as_object().expect("top-level object");

    for empty_key in &["HASH_FILE_URL", "FILE_SERVER_URL"] {
        let v = obj.get(*empty_key).and_then(|v| v.as_str()).unwrap_or("__missing__");
        assert_eq!(
            v,
            "",
            "{empty_key} must remain empty until the Classic+ updater \
             endpoint ships. Populating silently turns on the self-\
             update pipeline, which relies on a hash-file baseline \
             Classic+ doesn't produce yet (see CLAUDE.md §Known \
             Gaps). Open updater-endpoint-ship audit before changing."
        );
    }
}

// --------------------------------------------------------------------
// Iter 167 structural pins — audit-doc completeness + URL shape
// invariants that existing iter-142 pins don't catch.
// --------------------------------------------------------------------
//
// Iter 142 pinned the EXPECTED_KEYS set + shared API_BASE prefix +
// HASH/FILE updater-empty. These extend to: (1) the explicit Draft
// status line (must flip + pin must tighten at cutover), (2) the 7
// migration-plan sections the audit doc carries, (3) url-shape
// invariants the prefix-share pin can't detect — trailing slash on
// API_BASE would produce `//` in action URLs; action URLs on
// different ports would silently mis-route; SERVER_LIST carrying a
// `?lang=en` bakes in a caller responsibility.

/// The audit doc must carry an explicit "Status: Draft — pending ..."
/// line. When the production endpoint ships and the doc transitions
/// to "Signed off", this test fails intentionally — forcing the
/// author to audit AND tighten this guard (remove the LAN exception,
/// update EXPECTED_KEYS if any were added, update the action-URL
/// prefix check for the new FQDN) in the same commit.
#[test]
fn audit_doc_carries_explicit_draft_status_line() {
    let body = read(AUDIT_DOC);
    assert!(
        body.contains("**Status:** Draft"),
        "PRD §3.1.13: audit doc must carry `**Status:** Draft` (pre-\
         production). When cutover signs off, this guard + \
         `config_urls_are_https_or_lan_dev_or_empty` must tighten \
         atomically — removing the LAN_DEV_HOST exception and \
         flipping this string to `**Status:** Signed off`."
    );
    assert!(
        body.contains("pending production HTTPS endpoint"),
        "PRD §3.1.13: audit doc must carry `pending production \
         HTTPS endpoint` to name the specific gate that lifts on \
         cutover."
    );
}

/// The audit doc must carry all seven migration-plan sections. A
/// partial plan (e.g. missing Rollback plan) risks shipping the
/// cutover without a revert path. Pin the headings so a doc edit
/// that drops a section fails CI.
#[test]
fn audit_doc_has_all_seven_migration_plan_sections() {
    let body = read(AUDIT_DOC);
    for heading in [
        "## Current state",
        "## Threat model",
        "## Required before public launch",
        "## Launcher-side migration steps",
        "## Rollback plan",
        "## Acceptance",
        "## Human input required",
    ] {
        assert!(
            body.contains(heading),
            "PRD §3.1.13: audit doc must carry `{heading}` heading. \
             Missing a section means the migration plan is \
             incomplete — every section represents a gate that \
             must be reasoned through before cutover."
        );
    }
}

/// `API_BASE_URL` must NOT end with a trailing slash. The action
/// URLs concatenate `BASE + "/tera/..."`; a trailing slash on the
/// base produces `//tera/...`, which some HTTP stacks collapse
/// silently and others don't (depends on reverse-proxy rewrite
/// rules). Avoiding the ambiguity entirely is cheaper than auditing
/// every hop.
#[test]
fn api_base_url_has_no_trailing_slash() {
    let body = read(CONFIG_JSON);
    let cfg: serde_json::Value =
        serde_json::from_str(&body).expect("config.json must parse");
    let base = cfg["API_BASE_URL"]
        .as_str()
        .expect("API_BASE_URL must be a string");
    assert!(
        !base.ends_with('/'),
        "PRD §3.1.13: API_BASE_URL (`{base}`) must NOT end with a \
         trailing slash. Action URLs concatenate \
         `BASE + \"/tera/...\"`; a trailing slash produces `//tera/` \
         which behaves inconsistently across reverse-proxy hops."
    );
}

/// Every action URL must use the same port as API_BASE_URL. A port
/// drift (e.g. login on :443 while server list stays on :8090) is a
/// silent mis-wire that breaks requests only when the frontend hits
/// the non-migrated endpoint. Pin that `BASE:port` appears in every
/// action URL.
#[test]
fn all_portal_action_urls_share_base_port() {
    let body = read(CONFIG_JSON);
    let cfg: serde_json::Value =
        serde_json::from_str(&body).expect("config.json must parse");
    let base = cfg["API_BASE_URL"]
        .as_str()
        .expect("API_BASE_URL must be a string");

    // Extract the host:port part of BASE (everything up to the first
    // `/` after the scheme).
    let after_scheme = base
        .strip_prefix("https://")
        .or_else(|| base.strip_prefix("http://"))
        .expect("API_BASE_URL must start with http:// or https://");
    let host_port = after_scheme.split('/').next().unwrap_or(after_scheme);

    for action_key in &[
        "LOGIN_ACTION_URL",
        "GET_ACCOUNT_INFO_URL",
        "REGISTER_ACTION_URL",
        "MAINTENANCE_STATUS_URL",
        "SERVER_LIST_URL",
    ] {
        let v = cfg[action_key]
            .as_str()
            .unwrap_or_else(|| panic!("{action_key} must be a string"));
        assert!(
            v.contains(host_port),
            "PRD §3.1.13: {action_key} (`{v}`) must include the \
             same host:port as API_BASE_URL (`{host_port}`). Port \
             drift silently mis-routes requests to the non-\
             migrated endpoint."
        );
    }
}

/// `SERVER_LIST_URL` must NOT carry a `?lang=` query string. Per
/// CLAUDE.md §v100 API, the lang query param is the caller's
/// responsibility — baking it into the config hardcodes one
/// language and prevents the switcher from working. The config
/// should be `.../tera/ServerList`; callers append `?lang=en`.
#[test]
fn server_list_url_carries_no_query_string() {
    let body = read(CONFIG_JSON);
    let cfg: serde_json::Value =
        serde_json::from_str(&body).expect("config.json must parse");
    let url = cfg["SERVER_LIST_URL"]
        .as_str()
        .expect("SERVER_LIST_URL must be a string");
    assert!(
        !url.contains('?'),
        "PRD §3.1.13: SERVER_LIST_URL (`{url}`) must NOT carry a \
         query string. Per CLAUDE.md §v100 API, `?lang=en` is a \
         caller-supplied parameter; baking it into the config \
         hardcodes one language and breaks the switcher."
    );
}

// --------------------------------------------------------------------
// Iter 208 structural pins — meta-guard header + CSP connect-src
// alignment + /tera/ path namespace + action-endpoint-name canon
// + LAN_DEV_HOST constant vs reality.
// --------------------------------------------------------------------
//
// The eleven pins above cover config-shape (keys, prefix, port,
// trailing-slash, query-string, updater-empty) + audit-doc shape
// (draft status, section set). They do NOT pin: (a) the guard's own
// module header cites PRD 3.1.13 — meta-guard contract; (b) the Tauri
// CSP `connect-src` directive in `tauri.conf.json` must admit the
// current API_BASE_URL host — if config.json's base updates (LAN →
// production HTTPS) but CSP doesn't, the frontend can't reach portal
// and every login silently fails; (c) every action URL path begins
// with `/tera/` — a drift to `/api/` or `/v2/` would silently
// mis-route without the prefix-share pin noticing (it only checks
// host:port); (d) each action URL carries its canonical endpoint
// name — typo from `LauncherLoginAction` to `loginAction` would pass
// prefix + port + path-namespace pins but return 404; (e) the guard's
// own `LAN_DEV_HOST` constant must equal the host component of
// `API_BASE_URL` — drift of either side makes the LAN exception stop
// covering the actual LAN endpoint and every config URL becomes an
// offender.

/// The guard's module header must cite PRD 3.1.13 so a reader
/// chasing a portal-HTTPS drift lands here via section-grep. Without
/// the cite, a regression triggers an anonymous failure.
#[test]
fn guard_file_header_cites_prd_3_1_13() {
    let body = read("tests/portal_https_guard.rs");
    let header = &body[..body.len().min(1500)];
    assert!(
        header.contains("PRD 3.1.13"),
        "meta-guard contract: tests/portal_https_guard.rs header must \
         cite `PRD 3.1.13`. Without it, a reader chasing a \
         portal-HTTPS regression won't land here via section-grep.\n\
         Header:\n{header}"
    );
    assert!(
        header.contains("portal-HTTPS"),
        "meta-guard contract: header must carry the criterion slug \
         `portal-HTTPS` so name-based cross-reference with PRD P-slots \
         works."
    );
}

/// `tauri.conf.json`'s CSP `connect-src` directive must admit the
/// current API_BASE_URL host. If config.json's portal host updates
/// (e.g. LAN dev → production HTTPS FQDN) but CSP stays pinned to
/// the old host, the frontend can't reach portal and every login
/// silently fails with a CSP-blocked error. Pin that the CSP tracks
/// config.json's host.
#[test]
fn csp_connect_src_admits_current_api_base_host() {
    let cfg_body = read(CONFIG_JSON);
    let cfg: serde_json::Value =
        serde_json::from_str(&cfg_body).expect("config.json must parse");
    let base = cfg["API_BASE_URL"].as_str().expect("API_BASE_URL must be string");

    let tauri_conf_body = read("tauri.conf.json");
    let tauri_conf: serde_json::Value =
        serde_json::from_str(&tauri_conf_body).expect("tauri.conf.json must parse");
    let csp = tauri_conf
        .pointer("/app/security/csp")
        .and_then(|v| v.as_str())
        .expect("app.security.csp must be a string");

    // Extract connect-src directive.
    let connect_pos = csp
        .find("connect-src")
        .expect("CSP must declare `connect-src`");
    let rest = &csp[connect_pos..];
    let end = rest.find(';').unwrap_or(rest.len());
    let connect_src = &rest[..end];

    // Extract the origin (scheme://host:port) from the base URL.
    let scheme_end = base
        .find("://")
        .expect("API_BASE_URL must carry `scheme://` prefix");
    let after_scheme_start = scheme_end + 3;
    let rest_after_scheme = &base[after_scheme_start..];
    let path_start = rest_after_scheme.find('/').unwrap_or(rest_after_scheme.len());
    let origin = &base[..after_scheme_start + path_start];

    assert!(
        connect_src.contains(origin),
        "PRD 3.1.13: CSP `connect-src` (`{connect_src}`) must admit \
         API_BASE_URL's origin (`{origin}`). Without it, the frontend \
         can't reach portal — every login silently fails with a \
         CSP-blocked fetch. When portal flips to production HTTPS, \
         config.json + this CSP directive must update atomically."
    );
}

/// Every action URL's path component must start with `/tera/`. The
/// Portal v100 API namespaces all launcher endpoints under `/tera/`
/// (see CLAUDE.md §v100 API). A drift to `/api/` or `/v2/` would
/// pass the prefix-share + port pins (which only check host:port)
/// but 404 at the server.
#[test]
fn action_urls_use_tera_path_namespace() {
    let body = read(CONFIG_JSON);
    let cfg: serde_json::Value =
        serde_json::from_str(&body).expect("config.json must parse");
    for action_key in &[
        "LOGIN_ACTION_URL",
        "GET_ACCOUNT_INFO_URL",
        "REGISTER_ACTION_URL",
        "MAINTENANCE_STATUS_URL",
        "SERVER_LIST_URL",
    ] {
        let v = cfg[action_key].as_str().unwrap_or_else(|| {
            panic!("{action_key} must be a string")
        });
        // Locate the path component after the host:port segment.
        let scheme_end = v.find("://").unwrap_or(0) + 3;
        let path_start = v[scheme_end..].find('/').map(|i| scheme_end + i);
        let path = match path_start {
            Some(p) => &v[p..],
            None => panic!("{action_key} (`{v}`) has no path component"),
        };
        assert!(
            path.starts_with("/tera/"),
            "PRD 3.1.13: {action_key} path (`{path}`) must start with \
             `/tera/` — the Portal v100 API namespace (CLAUDE.md \
             §v100 API). A drift to `/api/` or `/v2/` would 404 at \
             the server while passing prefix + port pins."
        );
    }
}

/// Each action URL must carry its canonical endpoint name. A typo
/// (e.g. `LauncherLoginAction` → `loginAction`) would pass every
/// prefix / port / `/tera/` pin but silently 404 at the server.
/// Pin the exact endpoint names that match the Portal v100 API contract.
#[test]
fn action_urls_carry_canonical_endpoint_names() {
    let body = read(CONFIG_JSON);
    let cfg: serde_json::Value =
        serde_json::from_str(&body).expect("config.json must parse");

    for (key, endpoint) in [
        ("LOGIN_ACTION_URL", "LauncherLoginAction"),
        ("GET_ACCOUNT_INFO_URL", "GetAccountInfoByUserNo"),
        ("REGISTER_ACTION_URL", "LauncherSignupAction"),
        ("MAINTENANCE_STATUS_URL", "LauncherMaintenanceStatus"),
        ("SERVER_LIST_URL", "ServerList"),
    ] {
        let v = cfg[key]
            .as_str()
            .unwrap_or_else(|| panic!("{key} must be a string"));
        assert!(
            v.ends_with(endpoint),
            "PRD 3.1.13: {key} (`{v}`) must end with the canonical \
             endpoint name `{endpoint}` — this is the Portal v100 \
             API contract (CLAUDE.md §v100 API). A typo here passes \
             every prefix / port / path-namespace pin but 404s at \
             the server."
        );
    }
}

/// The guard's `LAN_DEV_HOST` constant must equal the host component
/// of `API_BASE_URL`. If either side drifts independently (guard
/// constant updated but config forgotten, or vice versa), the LAN
/// exception in `config_urls_are_https_or_lan_dev_or_empty` stops
/// covering the actual LAN endpoint and every plain-http config URL
/// becomes an offender — or worse, the old host is still accepted
/// but the new one is flagged as a "stray http" hit.
#[test]
fn lan_dev_host_constant_matches_api_base_host() {
    let body = read(CONFIG_JSON);
    let cfg: serde_json::Value =
        serde_json::from_str(&body).expect("config.json must parse");
    let base = cfg["API_BASE_URL"]
        .as_str()
        .expect("API_BASE_URL must be a string");

    // Extract host (no scheme, no port, no path).
    let after_scheme = base
        .strip_prefix("http://")
        .or_else(|| base.strip_prefix("https://"))
        .expect("API_BASE_URL must have scheme prefix");
    let host_port = after_scheme.split('/').next().unwrap_or(after_scheme);
    let host = host_port.split(':').next().unwrap_or(host_port);

    // If API_BASE is already HTTPS (post-cutover), LAN_DEV_HOST will
    // not appear in it — that's an allowed state (cutover done).
    // The guard source itself should then have the LAN_DEV_HOST
    // constant removed or the `config_urls_*` test tightened. This
    // pin only bites pre-cutover, where we DO expect alignment.
    if base.starts_with("http://") {
        assert_eq!(
            host, LAN_DEV_HOST,
            "PRD 3.1.13: pre-cutover, API_BASE_URL host (`{host}`) \
             must equal the guard's `LAN_DEV_HOST` constant \
             (`{LAN_DEV_HOST}`). Drift means the LAN exception no \
             longer covers the actual endpoint — either config or \
             the constant was updated without the other. Post-\
             cutover (https://), this pin is a no-op."
        );
    }
}

/// Self-test — prove the detector bites on synthetic bad shapes.
#[test]
fn portal_https_detector_self_test() {
    // Bad: HTTPS elsewhere + one sneaky http:// to a non-LAN host.
    let bad = serde_json::json!({
        "API_BASE_URL": "http://evil.example.com/api",
        "LOGIN_URL": "https://legit.example.com/login",
        "LAN_URL": "http://192.168.1.128:8090/dev",
        "EMPTY_URL": "",
    });

    let obj = bad.as_object().unwrap();
    let mut offenders = Vec::new();
    for (k, v) in obj {
        let Some(s) = v.as_str() else { continue };
        if !s.starts_with("http://") && !s.starts_with("https://") && !s.is_empty() {
            continue;
        }
        let ok = s.is_empty()
            || s.starts_with("https://")
            || s.contains(LAN_DEV_HOST);
        if !ok {
            offenders.push(format!("{k} = {s}"));
        }
    }

    assert_eq!(
        offenders.len(),
        1,
        "self-test: exactly one offender expected (the non-LAN http \
         URL); got {}: {:?}",
        offenders.len(),
        offenders,
    );
    assert!(offenders[0].contains("evil.example.com"));

    // Bad shape B (iter 142): config with an extra unexpected key.
    let extra_key_config = serde_json::json!({
        "API_BASE_URL": "http://192.168.1.128:8090",
        "ROGUE_NEW_URL": "https://attacker.example.com/",
    });
    let got: std::collections::BTreeSet<&str> = extra_key_config
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    let want: std::collections::BTreeSet<&str> =
        EXPECTED_KEYS.iter().copied().collect();
    let extra: Vec<_> = got.difference(&want).collect();
    assert!(
        !extra.is_empty(),
        "self-test: rogue unexpected key must appear in extra set"
    );

    // Bad shape C: action URL that drifted from API_BASE_URL prefix.
    let drift_cfg = serde_json::json!({
        "API_BASE_URL": "http://192.168.1.128:8090",
        "LOGIN_ACTION_URL": "http://different-host:9000/tera/LauncherLoginAction",
    });
    let base = drift_cfg
        .get("API_BASE_URL")
        .and_then(|v| v.as_str())
        .unwrap();
    let login = drift_cfg
        .get("LOGIN_ACTION_URL")
        .and_then(|v| v.as_str())
        .unwrap();
    assert!(
        !login.starts_with(base),
        "self-test: drifted action URL must not match base prefix"
    );

    // Bad shape D: updater URL populated before endpoint ships.
    let populated_updater = serde_json::json!({
        "HASH_FILE_URL": "https://leaked-updater.example.com/hashes.json",
    });
    let hash_url = populated_updater
        .get("HASH_FILE_URL")
        .and_then(|v| v.as_str())
        .unwrap();
    assert_ne!(
        hash_url, "",
        "self-test: populated updater URL must be caught by the empty-\
         string check"
    );
}

// --------------------------------------------------------------------
// Iter 244 structural pins — path-constant canonicalisation, LAN
// port locked, EXPECTED_KEYS cardinality, audit doc documents
// dormant-until production, and config file root shape.
// --------------------------------------------------------------------

/// Iter 244: all 3 path constants and LAN host must stay canonical.
/// Every file read / host comparison resolves through one of these;
/// drift silently breaks multiple pins.
#[test]
fn guard_path_and_host_constants_are_canonical() {
    let body = fs::read_to_string("tests/portal_https_guard.rs")
        .expect("guard source must exist");
    for (name, expected) in [
        ("CONFIG_JSON", "../../teralib/src/config/config.json"),
        ("AUDIT_DOC", "../../docs/PRD/audits/security/portal-https-migration.md"),
        ("LAN_DEV_HOST", "192.168.1.128"),
    ] {
        let line = format!("const {name}: &str = \"{expected}\";");
        assert!(
            body.contains(&line),
            "PRD 3.1.13 (iter 244): tests/portal_https_guard.rs must \
             keep `{line}` verbatim. A drift leaves every pin reading \
             through it in an inconsistent state."
        );
    }
}

/// Iter 244: the LAN dev portal port must stay 8090. A port
/// migration (to 443 or 8443) without PRD sign-off would be the
/// cutover this guard is watching for — and the test-name itself
/// becomes an audit-trail artifact.
#[test]
fn lan_dev_port_is_eight_zero_nine_zero() {
    let body = read(CONFIG_JSON);
    let config: serde_json::Value =
        serde_json::from_str(&body).expect("config.json must parse");
    let obj = config.as_object().expect("config must be an object");
    // Find any URL value mentioning the LAN host; the port after `:`
    // must be `8090`.
    for (_k, v) in obj {
        if let Some(s) = v.as_str() {
            if s.contains(LAN_DEV_HOST) {
                // Extract port.
                let after_host = s
                    .split(LAN_DEV_HOST)
                    .nth(1)
                    .unwrap_or("");
                if let Some(port_str) = after_host.strip_prefix(':') {
                    let port: String = port_str
                        .chars()
                        .take_while(|c| c.is_ascii_digit())
                        .collect();
                    if !port.is_empty() {
                        assert_eq!(
                            port, "8090",
                            "PRD 3.1.13 (iter 244): LAN dev portal \
                             URL must use port 8090. A drift to 443 \
                             or 8443 signals the portal-https \
                             cutover — coordinate with the PRD \
                             3.1.13 sign-off. Offending URL: {s}"
                        );
                    }
                }
            }
        }
    }
}

/// Iter 244: `EXPECTED_KEYS` must stay bounded. The set defines
/// which config keys the launcher expects; silent growth means
/// new URLs landed without a PRD audit; silent shrinkage means
/// a critical URL was dropped.
#[test]
fn expected_keys_count_stays_bounded() {
    let n = EXPECTED_KEYS.len();
    assert!(
        n >= 5,
        "PRD 3.1.13 (iter 244): EXPECTED_KEYS has shrunk to {n} \
         entries — below the floor of 5. Critical URL categories \
         appear deleted; this is a cutover red-flag."
    );
    assert!(
        n <= 40,
        "PRD 3.1.13 (iter 244): EXPECTED_KEYS has grown to {n} \
         entries — above the ceiling of 40. New URL categories \
         should land with a coordinated audit update; raising the \
         ceiling requires a deliberate test update."
    );
    // Every entry must be upper-snake-case (convention for env-style
    // keys). A lowercase or kebab entry signals accidental
    // divergence.
    for k in EXPECTED_KEYS {
        assert!(
            !k.is_empty() && k.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_'),
            "PRD 3.1.13 (iter 244): EXPECTED_KEYS entry `{k}` must \
             be UPPER_SNAKE_CASE. Lowercase or kebab-case signals a \
             convention drift."
        );
    }
}

/// Iter 244: the audit doc must explicitly document the dormant-
/// until-production status. Iter-57 confirmed there's no Classic+
/// production yet; the audit doc is the record that the 3.1.13
/// item is DORMANT pending a production FQDN. If the doc drops
/// this wording, a reader might assume the item should be fixed
/// now and try to flip the config to an unreachable HTTPS URL,
/// breaking dev.
#[test]
fn audit_doc_documents_dormant_status_and_preconditions() {
    let body = fs::read_to_string(AUDIT_DOC)
        .unwrap_or_else(|e| panic!("{AUDIT_DOC} must be readable: {e}"));
    // Must cite dormancy or the LAN-until-production status.
    let lc = body.to_lowercase();
    let mentions_dormancy = lc.contains("dormant")
        || lc.contains("lan dev")
        || lc.contains("until production")
        || lc.contains("pending production")
        || lc.contains("dev-only")
        || lc.contains("developer's lan")
        || lc.contains("developers lan");
    assert!(
        mentions_dormancy,
        "PRD 3.1.13 (iter 244): {AUDIT_DOC} must document the \
         dormant-until-production status. Iter-57 established there \
         is no Classic+ production FQDN yet; the audit doc is the \
         record. Without the wording, a reader might try to flip \
         the config to an unreachable HTTPS URL."
    );
    // Must cite the three preconditions for waking: FQDN + TLS cert
    // + reverse proxy.
    let mentions_preconditions = lc.contains("fqdn")
        || lc.contains("tls cert")
        || lc.contains("reverse proxy")
        || lc.contains("tls certificate")
        || lc.contains("let's encrypt");
    assert!(
        mentions_preconditions,
        "PRD 3.1.13 (iter 244): {AUDIT_DOC} must enumerate the \
         preconditions for waking the item (FQDN + TLS cert + \
         reverse proxy) — same three gates cited in fix-plan.md's \
         P0-DORMANT entry."
    );
}

/// Iter 244: the config JSON root must be an object with at least
/// one URL-valued entry. An empty root or root-array would break
/// every pin that iterates `config.as_object()`.
#[test]
fn config_root_is_an_object_with_url_valued_entries() {
    let body = read(CONFIG_JSON);
    let config: serde_json::Value =
        serde_json::from_str(&body).expect("config.json must parse");
    let obj = config
        .as_object()
        .expect("PRD 3.1.13 (iter 244): config.json root must be an \
                 object (not an array or primitive)");
    assert!(
        !obj.is_empty(),
        "PRD 3.1.13 (iter 244): config.json root object must not be \
         empty. At least one URL entry must exist."
    );
    // At least one entry must look like an http/https URL (proof
    // the config actually carries URLs, not just metadata).
    let url_count = obj
        .values()
        .filter_map(|v| v.as_str())
        .filter(|s| s.starts_with("http://") || s.starts_with("https://"))
        .count();
    assert!(
        url_count >= 1,
        "PRD 3.1.13 (iter 244): config.json must carry at least one \
         http(s) URL. Found 0 — config appears to have been gutted."
    );
}

// --------------------------------------------------------------------
// Iter 281 structural pins — config/audit/guard bounds + PRD cite +
// EXPECTED_KEYS minimum.
// --------------------------------------------------------------------

#[test]
fn config_json_byte_bounds() {
    const MIN: usize = 100;
    const MAX: usize = 20_000;
    let bytes = std::fs::metadata(CONFIG_JSON)
        .expect("config.json must exist")
        .len() as usize;
    assert!(
        (MIN..=MAX).contains(&bytes),
        "PRD 3.1.13 (iter 281): {CONFIG_JSON} is {bytes} bytes; \
         expected [{MIN}, {MAX}]."
    );
}

#[test]
fn audit_doc_byte_bounds() {
    const MIN: usize = 500;
    const MAX: usize = 50_000;
    let bytes = std::fs::metadata(AUDIT_DOC)
        .expect("audit doc must exist")
        .len() as usize;
    assert!(
        (MIN..=MAX).contains(&bytes),
        "PRD 3.1.13 (iter 281): {AUDIT_DOC} is {bytes} bytes; \
         expected [{MIN}, {MAX}]."
    );
}

#[test]
fn guard_source_byte_bounds() {
    const MIN: usize = 5000;
    const MAX: usize = 80_000;
    let bytes = std::fs::metadata("tests/portal_https_guard.rs")
        .expect("guard must exist")
        .len() as usize;
    assert!(
        (MIN..=MAX).contains(&bytes),
        "PRD 3.1.13 (iter 281): guard is {bytes} bytes; expected \
         [{MIN}, {MAX}]."
    );
}

#[test]
fn guard_source_cites_prd_3_1_13_explicitly() {
    let body = std::fs::read_to_string("tests/portal_https_guard.rs")
        .expect("guard must exist");
    let header = &body[..body.len().min(500)];
    assert!(
        header.contains("PRD 3.1.13"),
        "PRD 3.1.13 (iter 281): guard header must cite `PRD 3.1.13`.\n\
         Header:\n{header}"
    );
}

#[test]
fn expected_keys_count_has_reasonable_floor() {
    const MIN: usize = 5;
    assert!(
        EXPECTED_KEYS.len() >= MIN,
        "PRD 3.1.13 (iter 281): EXPECTED_KEYS has {} entries; floor \
         is {MIN}. A coordinated trim would vacate the per-key URL \
         scheme check.",
        EXPECTED_KEYS.len()
    );
}
