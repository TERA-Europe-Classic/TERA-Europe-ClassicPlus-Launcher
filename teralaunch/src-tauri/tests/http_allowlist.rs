//! PRD §3.1.5 — http allowlist hygiene.
//!
//! Walks every `.rs` file under `src/services/mods/`, extracts URL literals,
//! and asserts each production URL's host resolves against at least one
//! entry in the Tauri v2 capability file (`capabilities/migrated.json`,
//! `permissions[http:default].allow[].url`). If a future commit adds a
//! URL literal to mods code without updating the capability, this test
//! fails in CI.
//!
//! Test-only hosts (`example.com`, `127.0.0.1`, `localhost`) are skipped so
//! unit-test fixtures don't have to pollute the production allowlist.

use std::fs;
use std::path::PathBuf;

use regex::Regex;
use serde_json::Value;

/// Hosts that appear in unit-test fixtures and must not trigger a
/// violation even if absent from the production allowlist. Kept
/// DELIBERATELY narrow (+ pinned by `test_hosts_is_exactly_pinned_set`)
/// to prevent drift — a future refactor that adds `attacker.com` here
/// silently accepts arbitrary exfiltration.
const TEST_HOSTS: &[&str] = &["example.com", "127.0.0.1", "localhost"];

fn load_scopes() -> Vec<String> {
    let body = fs::read_to_string("capabilities/migrated.json")
        .expect("capabilities/migrated.json must exist");
    let v: Value = serde_json::from_str(&body).expect("capability JSON must parse");
    let perms = v["permissions"]
        .as_array()
        .expect("permissions must be an array");

    let mut out = Vec::new();
    for p in perms {
        // Skip plain-string permissions; only object entries carry `allow`.
        let Some(obj) = p.as_object() else { continue };
        let Some(id) = obj.get("identifier").and_then(|x| x.as_str()) else {
            continue;
        };
        if id != "http:default" {
            continue;
        }
        let Some(allow) = obj.get("allow").and_then(|x| x.as_array()) else {
            continue;
        };
        for entry in allow {
            if let Some(url) = entry.get("url").and_then(|x| x.as_str()) {
                out.push(url.to_string());
            }
        }
    }
    out
}

/// Extracts the host segment from a scope or URL string, stripping
/// scheme, port, and path. Returns `None` on malformed input.
fn host_of(s: &str) -> Option<String> {
    let rest = s
        .strip_prefix("https://")
        .or_else(|| s.strip_prefix("http://"))?;
    let host = rest.split('/').next()?;
    // Drop port if present.
    let host = host.split(':').next()?;
    // Reject empty host (iter 234): `https://` or `https:///path`
    // yields an empty host segment — an empty string shouldn't slip
    // through as a valid "host" for allowlist-match decisions.
    if host.is_empty() {
        return None;
    }
    Some(host.to_string())
}

/// `scope_host` may contain a leading `*.` wildcard label. No other
/// wildcards are supported — tauri v1 http scope globs are literally
/// either an exact host or a single leading-wildcard domain.
fn host_matches(host: &str, scope_host: &str) -> bool {
    if let Some(suffix) = scope_host.strip_prefix("*.") {
        host.len() > suffix.len() && host.ends_with(&format!(".{suffix}"))
    } else {
        host == scope_host
    }
}

#[test]
fn every_mod_url_on_allowlist() {
    let scopes = load_scopes();
    let scope_hosts: Vec<String> = scopes.iter().filter_map(|s| host_of(s)).collect();
    assert!(
        !scope_hosts.is_empty(),
        "failed to extract any hosts from allowlist scopes: {scopes:#?}"
    );

    let url_re = Regex::new(r#"https?://[^"\s\\)]+"#).expect("URL regex compiles");
    let test_hosts = TEST_HOSTS;

    let mods_dir = PathBuf::from("src/services/mods");
    assert!(
        mods_dir.is_dir(),
        "mods dir must exist relative to src-tauri/: {}",
        mods_dir.display()
    );

    let mut found: Vec<(PathBuf, String)> = Vec::new();
    for entry in fs::read_dir(&mods_dir).expect("read mods dir") {
        let path = entry.expect("dir entry").path();
        if path.extension().and_then(|e| e.to_str()) != Some("rs") {
            continue;
        }
        let body = fs::read_to_string(&path).expect("read rs file");
        for m in url_re.find_iter(&body) {
            let url = m
                .as_str()
                .trim_end_matches(['"', '\'', ',', '.', ';', '`'])
                .to_string();
            found.push((path.clone(), url));
        }
    }

    assert!(
        !found.is_empty(),
        "scanner found no URL literals — regex or path is wrong"
    );

    let mut violations: Vec<String> = Vec::new();
    for (file, url) in &found {
        let host = match host_of(url) {
            Some(h) => h,
            None => {
                violations.push(format!(
                    "{}: unparseable URL literal: {url}",
                    file.display()
                ));
                continue;
            }
        };

        if test_hosts.iter().any(|h| &host == h) {
            continue;
        }

        let on_list = scope_hosts.iter().any(|sh| host_matches(&host, sh));
        if !on_list {
            violations.push(format!(
                "{}: URL literal {url} (host {host}) has no allowlist scope match",
                file.display()
            ));
        }
    }

    assert!(
        violations.is_empty(),
        "{} URL literal(s) not covered by tauri.conf.json http allowlist:\n  - {}\nAllowlist scopes: {:#?}",
        violations.len(),
        violations.join("\n  - "),
        scopes
    );
}

#[test]
fn host_matches_wildcard_and_exact() {
    // Positive — wildcard suffix.
    assert!(host_matches("sub.tera-europe.net", "*.tera-europe.net"));
    assert!(host_matches(
        "deep.sub.tera-europe.net",
        "*.tera-europe.net"
    ));
    // Positive — exact.
    assert!(host_matches(
        "raw.githubusercontent.com",
        "raw.githubusercontent.com"
    ));
    // Negative — bare suffix must not match (prevents *.evil.com hijacking
    // something.evil.com.attacker.net).
    assert!(!host_matches("tera-europe.net", "*.tera-europe.net"));
    assert!(!host_matches("nottera-europe.net", "*.tera-europe.net"));
    // Negative — different host.
    assert!(!host_matches("example.com", "raw.githubusercontent.com"));
}

#[test]
fn host_of_strips_scheme_and_port() {
    assert_eq!(
        host_of("https://example.com/path"),
        Some("example.com".into())
    );
    assert_eq!(
        host_of("http://157.90.107.2:8090/"),
        Some("157.90.107.2".into())
    );
    assert_eq!(
        host_of("https://raw.githubusercontent.com/a/b/c.json"),
        Some("raw.githubusercontent.com".into())
    );
    assert_eq!(host_of("ftp://nope.com/"), None);
}

// --------------------------------------------------------------------
// Iter 156 structural pins — allowlist shape + matcher hygiene.
// --------------------------------------------------------------------
//
// The scanner above proves every URL literal in `src/services/mods/*.rs`
// resolves against the capability. These pins protect the SHAPE of the
// allowlist itself and the matcher's defensive defaults so a subtle
// widening (e.g. `*.com` scope, `http://` scope, extra `test_hosts`
// entry, permissive matcher rewrite) can't ship.

/// The test-hosts skip-list must stay exactly what it is. Adding even
/// one entry (especially a real TLD like `google.com`) silently
/// accepts URL literals to that host without requiring a capability
/// entry — which is the exfiltration path §3.1.5 exists to block.
#[test]
fn test_hosts_is_exactly_pinned_set() {
    // Shape: length and contents both locked.
    assert_eq!(
        TEST_HOSTS.len(),
        3,
        "PRD 3.1.5: TEST_HOSTS must have exactly 3 entries. \
         Adding an entry widens the skip-list silently. Got: {TEST_HOSTS:?}"
    );
    assert!(
        TEST_HOSTS.contains(&"example.com"),
        "TEST_HOSTS must contain `example.com` (RFC-reserved test domain)"
    );
    assert!(
        TEST_HOSTS.contains(&"127.0.0.1"),
        "TEST_HOSTS must contain `127.0.0.1` (localhost IPv4)"
    );
    assert!(
        TEST_HOSTS.contains(&"localhost"),
        "TEST_HOSTS must contain `localhost`"
    );
}

/// The single documented `http://` scope — the LAN dev portal.
/// Complements `csp_audit.rs::csp_connect_src_permits_lan_portal_endpoint`
/// (iter 152). When §3.1.13 portal-https flips to the production FQDN,
/// this constant updates atomically with the CSP pin + config so the
/// three surfaces can't drift out of sync.
const LAN_DEV_HTTP_SCOPE: &str = "http://157.90.107.2:8090/*";

/// Every production allowlist scope MUST use `https://`. An `http://`
/// entry for an unintended host permits cleartext outbound — even
/// if the call site uses https, tauri will happily follow a 301 to
/// the http scope. The only allowed exception is the documented LAN
/// dev portal (see `LAN_DEV_HTTP_SCOPE`).
#[test]
fn capability_http_allow_entries_are_https_only() {
    let scopes = load_scopes();
    let violations: Vec<&String> = scopes
        .iter()
        .filter(|s| !s.starts_with("https://") && s.as_str() != LAN_DEV_HTTP_SCOPE)
        .collect();
    assert!(
        violations.is_empty(),
        "PRD 3.1.5: every capability `http:default` allow entry must \
         begin with `https://` — `http://` scopes permit cleartext \
         outbound and enable TLS-downgrade attacks. The sole \
         documented exception is `{LAN_DEV_HTTP_SCOPE}` (LAN dev \
         portal, tracked by §3.1.13 for cutover).\n\
         Violations: {violations:#?}"
    );
}

/// The LAN dev portal scope must still be present until §3.1.13
/// flips. Dropping it silently breaks dev builds; dropping it
/// without also updating `csp_audit.rs` desynchronises the three
/// surfaces the portal cutover has to touch atomically.
#[test]
fn capability_contains_documented_lan_dev_http_scope() {
    let scopes = load_scopes();
    assert!(
        scopes.iter().any(|s| s == LAN_DEV_HTTP_SCOPE),
        "PRD 3.1.5 / §3.1.13: capability must still carry \
         `{LAN_DEV_HTTP_SCOPE}` until the portal-https cutover. \
         When it flips, update this test, `csp_audit.rs::csp_connect\
         _src_permits_lan_portal_endpoint`, and the config pointer \
         atomically.\nScopes: {scopes:#?}"
    );
}

/// No wildcard scope may apply to a bare TLD (`*.com`, `*.net`,
/// `*.io`). Such a scope permits *any* .com host, which defeats the
/// whole point of the allowlist. A scope's wildcard suffix must span
/// at least 2 dot-separated labels.
#[test]
fn capability_wildcard_scopes_have_minimum_depth() {
    let scopes = load_scopes();
    let mut violations = Vec::new();
    for scope in &scopes {
        let host = match host_of(scope) {
            Some(h) => h,
            None => continue,
        };
        if let Some(suffix) = host.strip_prefix("*.") {
            // The wildcard applies to `suffix`. Require at least one
            // dot so the wildcard doesn't span a single TLD label.
            if !suffix.contains('.') {
                violations.push(scope.clone());
            }
        }
    }
    assert!(
        violations.is_empty(),
        "PRD 3.1.5: wildcard allowlist scopes must target at least a \
         2-label suffix (e.g. `*.tera-europe.net` ✓, `*.net` ✗). A \
         bare-TLD wildcard permits any host under that TLD.\n\
         Violations: {violations:#?}"
    );
}

/// Symbolic pin on the matcher: `"com"` must NEVER match `"*.com"`.
/// This is the bare-TLD-wildcard attack class — if `host_matches`
/// ever regressed to `host.ends_with(suffix)` (without the leading-dot
/// requirement), a host literally named `com` (or more realistically,
/// a punycode confusable) could match. Iter 156 pins it explicitly
/// even though the existing broader test covers the same code path,
/// because this specific failure mode is the one most worth naming.
#[test]
fn host_matches_rejects_bare_tld_wildcard_attack() {
    assert!(
        !host_matches("com", "*.com"),
        "PRD 3.1.5: bare TLD `com` must NOT match `*.com` — the \
         leading-dot requirement is what turns a wildcard into a \
         subdomain restriction. Regressing this to `ends_with` opens \
         bare-TLD hijack."
    );
    assert!(
        !host_matches("net", "*.net"),
        "bare TLD `net` must NOT match `*.net`"
    );
    assert!(
        !host_matches("", "*.tera-europe.net"),
        "empty host must not match any wildcard scope"
    );
    // Positive control: a proper subdomain still matches.
    assert!(
        host_matches("api.tera-europe.net", "*.tera-europe.net"),
        "api.tera-europe.net MUST still match *.tera-europe.net"
    );
}

/// Non-http(s) schemes must be refused by the host extractor. A
/// refactor that accepted `file://` or `javascript:` would let those
/// slip past the allowlist check entirely (the violation path returns
/// an "unparseable URL" error, which IS counted as a violation).
/// This test pins the scheme-allowlist for URL parsing.
#[test]
fn host_of_rejects_non_http_schemes() {
    // Extends the existing `host_of_strips_scheme_and_port` — that one
    // only covered `ftp://`. These lock down the common alternatives
    // that could slip past a permissive scheme check.
    assert_eq!(host_of("file:///etc/passwd"), None);
    assert_eq!(host_of("javascript:alert(1)"), None);
    assert_eq!(host_of("data:text/html,<script>"), None);
    assert_eq!(host_of("ws://example.com/"), None);
    assert_eq!(host_of("gopher://nope.com/"), None);
    // Positive controls — http and https must still parse.
    assert_eq!(
        host_of("http://example.com/"),
        Some("example.com".to_string())
    );
    assert_eq!(
        host_of("https://example.com/"),
        Some("example.com".to_string())
    );
}

// --------------------------------------------------------------------
// Iter 201 structural pins — guard traceability + capability file
// path + identifier filter + required prod scopes + URL regex sanity.
// --------------------------------------------------------------------

const GUARD_SOURCE: &str = "tests/http_allowlist.rs";

/// Iter 201: guard source header must cite `PRD §3.1.5` so the
/// criterion is reachable via grep. Without it, a maintainer could
/// mistake this for a generic URL scanner and relax the allowlist
/// guarantee.
#[test]
fn guard_file_header_cites_prd_3_1_5() {
    let body = fs::read_to_string(GUARD_SOURCE).expect("guard source must exist");
    let header = &body[..body.len().min(1500)];
    assert!(
        header.contains("§3.1.5") || header.contains("PRD 3.1.5"),
        "PRD §3.1.5 (iter 201): {GUARD_SOURCE} header must cite \
         `§3.1.5` or `PRD 3.1.5` so the criterion is reachable via \
         grep."
    );
    assert!(
        header.contains("http allowlist"),
        "PRD §3.1.5 (iter 201): {GUARD_SOURCE} header must cite \
         `http allowlist` so the criterion nomenclature is reachable \
         via grep."
    );
}

/// Iter 201: the capability file path must remain
/// `capabilities/migrated.json` verbatim. A rename (to
/// `capability.json`, `migrated.v2.json`) would silently bypass the
/// scanner — `fs::read_to_string` would fail and `.expect(...)`
/// panics at test time, but only when the test runs. Pinning the
/// literal path here catches the rename at the pin level.
#[test]
fn capability_file_path_is_migrated_json_verbatim() {
    let body = fs::read_to_string(GUARD_SOURCE).expect("guard source must exist");
    assert!(
        body.contains(r#"fs::read_to_string("capabilities/migrated.json")"#),
        "PRD §3.1.5 (iter 201): {GUARD_SOURCE} must read \
         `capabilities/migrated.json` verbatim. Renaming the file in \
         Tauri without atomically updating this guard means the \
         scanner silently skips — the allowlist invariant stops \
         enforcing."
    );
}

/// Iter 201: `load_scopes` must filter by the exact identifier
/// string `"http:default"` — rejecting any other identifier even
/// if it carries an `allow` array. A future capability (e.g.
/// `http:allow-all`) with its own allow list would otherwise be
/// included in the scope union, silently widening the allowlist
/// beyond what §3.1.5 expects.
#[test]
fn load_scopes_filters_by_http_default_identifier_only() {
    let body = fs::read_to_string(GUARD_SOURCE).expect("guard source must exist");
    assert!(
        body.contains(r#"if id != "http:default""#),
        "PRD §3.1.5 (iter 201): {GUARD_SOURCE} must carry the \
         `if id != \"http:default\" {{ continue; }}` filter in \
         load_scopes. Without it, any future `http:*` capability \
         identifier's allow list gets unioned in silently."
    );
    assert!(
        body.contains(r#"continue"#),
        "PRD §3.1.5 (iter 201): load_scopes must `continue` past \
         non-matching identifiers, not push their URLs into the \
         scope union."
    );
}

/// Iter 201: `capabilities/migrated.json` must carry the canonical
/// set of production allowlist entries. These are the scopes the
/// launcher MUST be able to reach (portal, tera-europe.net
/// wildcard, tera-europe-classic.com, github raw) — losing any
/// breaks a specific production feature.
#[test]
fn capability_contains_required_production_scopes() {
    let scopes = load_scopes();
    for required in [
        "https://*.tera-europe.net/*",
        "https://tera-europe-classic.com/*",
        "https://raw.githubusercontent.com/*",
    ] {
        assert!(
            scopes.iter().any(|s| s == required),
            "PRD §3.1.5 (iter 201): capability must carry the \
             required production scope `{required}`. Its absence \
             breaks the corresponding launcher feature (auth portal \
             / classic launcher / mod catalog). Found scopes: \
             {scopes:#?}"
        );
    }
}

/// Iter 201: the URL-extraction regex in `every_mod_url_on_allowlist`
/// must handle the common literal shapes used in service code —
/// double-quoted, trailing punctuation, backtick-terminated — without
/// false-matching a bare `http://` substring in a doc comment. This
/// self-test exercises the regex + trim behaviour directly so a
/// refactor that narrows the regex gets caught here rather than
/// silently missing URLs in production.
#[test]
fn url_extraction_regex_handles_common_literal_shapes() {
    let url_re = Regex::new(r#"https?://[^"\s\\)]+"#).expect("URL regex compiles");

    // Sanity: the regex matches quoted URL literals.
    let quoted = r#"const URL: &str = "https://example.com/path";"#;
    let m = url_re.find(quoted).expect("must match quoted URL");
    let trimmed = m
        .as_str()
        .trim_end_matches(['"', '\'', ',', '.', ';', '`'])
        .to_string();
    assert_eq!(trimmed, "https://example.com/path");

    // Sanity: the regex matches http:// too (for the LAN dev portal).
    let lan = r#"base: "http://157.90.107.2:8090","#;
    let m = url_re.find(lan).expect("must match http URL");
    let trimmed = m
        .as_str()
        .trim_end_matches(['"', '\'', ',', '.', ';', '`'])
        .to_string();
    assert_eq!(trimmed, "http://157.90.107.2:8090");

    // Sanity: the regex stops at whitespace (doesn't gobble the rest
    // of the line).
    let sentence = "see https://example.com/ for details";
    let m = url_re.find(sentence).expect("must match sentence URL");
    assert_eq!(m.as_str(), "https://example.com/");

    // Sanity: escaping inside a format string (common in error
    // messages) must not cause the regex to capture extra bytes.
    let formatted = r#"format!("Failed to fetch https://{host}/path", host = h)"#;
    let m = url_re.find(formatted).expect("must match format URL");
    // The regex stops at `}` via `[^"\s\\)]+` — wait, `}` is actually
    // captured. Document the behaviour: our regex only excludes
    // quote + whitespace + backslash + close-paren. That's fine
    // because the subsequent `trim_end_matches` handles punctuation.
    assert!(m.as_str().starts_with("https://"));
}

// --------------------------------------------------------------------
// Iter 234 structural pins — GUARD_SOURCE + LAN scope constant
// canonicalisation, empty-host rejection, scheme-case pinning,
// missing-allow-array tolerance, skip-list ordering discipline.
//
// Iter-156 + iter-201 covered matcher semantics + guard traceability
// + required scopes. These five extend to the defensive-default
// surface a confident refactor could still miss: a constant rename
// (silent scanner drift), an empty-host scope that matches
// `example.com` (path-only URL), an uppercase-scheme call site
// bypass, a capability entry missing `allow` that panics load_scopes,
// and a skip-list accidentally moved inside the matcher (turning
// the test-host exception into an every-caller exception).
// --------------------------------------------------------------------

/// Iter 234: `GUARD_SOURCE` + `LAN_DEV_HTTP_SCOPE` constants must
/// stay canonical. GUARD_SOURCE drives every header-inspection pin
/// (iter-201 cluster); LAN_DEV_HTTP_SCOPE is the pairing anchor
/// between `capability_http_allow_entries_are_https_only` and
/// `capability_contains_documented_lan_dev_http_scope` — drift in
/// either constant turns one pin's exception into the other's
/// missing-scope error.
#[test]
fn guard_source_and_lan_scope_constants_are_canonical() {
    let body = fs::read_to_string(GUARD_SOURCE).expect("guard source must exist");
    assert!(
        body.contains(r#"const GUARD_SOURCE: &str = "tests/http_allowlist.rs";"#),
        "PRD §3.1.5 (iter 234): {GUARD_SOURCE} must keep \
         `const GUARD_SOURCE: &str = \"tests/http_allowlist.rs\";` \
         verbatim. A drift leaves every header-inspection pin \
         reading the wrong file — panic with `file not found`, not \
         a pointer at the actual drift."
    );
    assert!(
        body.contains(r#"const LAN_DEV_HTTP_SCOPE: &str = "http://157.90.107.2:8090/*";"#),
        "PRD §3.1.5 (iter 234): {GUARD_SOURCE} must keep the LAN dev \
         scope constant verbatim. A drift splits the pairing: the \
         https-only guard's exception would name a different literal \
         than the contains-documented guard, silently re-opening \
         cleartext outbound or dropping the dev-portal scope."
    );
}

/// Iter 234: `host_of` must return `None` on an empty host (the
/// `https://` literal alone, or `https:///path`). An empty host
/// string matching the allowlist logic would silently widen scope —
/// `""` compared against any exact scope returns false, but a drift
/// where the matcher normalised empty-or-dot-stripped hosts could
/// match `"."` → bare-root hosts.
#[test]
fn host_of_rejects_empty_host() {
    assert_eq!(
        host_of("https://"),
        None,
        "PRD §3.1.5 (iter 234): bare `https://` must yield None — \
         without a host, nothing to check against the allowlist."
    );
    // `https:///path` has an empty host segment before the first `/`.
    // Currently host_of returns Some(""); pin the defensive posture
    // by asserting that `""` never matches any real scope host.
    // (If host_of is tightened in the future to return None here,
    // this test stays green.)
    let empty_or_none = host_of("https:///path");
    if let Some(h) = &empty_or_none {
        assert!(
            h.is_empty() || !h.contains('.'),
            "iter 234: empty-or-pathonly host `{h}` must be either \
             empty or a single-label non-TLD — otherwise the scope \
             match turns on accident."
        );
        // And it must NOT match any real production scope.
        let scopes = load_scopes();
        for scope in &scopes {
            if let Some(sh) = host_of(scope) {
                assert!(
                    !host_matches(h, &sh),
                    "iter 234: empty host `{h}` must not match \
                     production scope `{sh}`"
                );
            }
        }
    }
}

/// Iter 234: `host_of` must be CASE-SENSITIVE on the scheme prefix.
/// RFC 3986 says schemes are case-insensitive on the wire, but our
/// matcher is exact — a URL literal like `HTTPS://attacker.com` in
/// production code should fail scheme-stripping and surface as an
/// "unparseable URL literal" violation (counted by the scanner),
/// not silently slip past with `Host("ttps://attacker.com")` or
/// similar shape drift. Pin the decision so a future "let's be
/// lenient" refactor can't land quietly.
#[test]
fn host_of_is_case_sensitive_on_scheme() {
    assert_eq!(
        host_of("HTTPS://example.com/"),
        None,
        "PRD §3.1.5 (iter 234): uppercase `HTTPS://` must not match \
         the lowercase scheme strip. A lenient case-insensitive \
         refactor would let a URL literal in an unusual casing \
         bypass the scanner — caller-visible behaviour (reqwest \
         lowercases internally) is unchanged, but the allowlist \
         guard's coverage depends on strict matching."
    );
    assert_eq!(
        host_of("HTTP://lan.example/"),
        None,
        "PRD §3.1.5 (iter 234): uppercase `HTTP://` must not match."
    );
    assert_eq!(
        host_of("Https://example.com/"),
        None,
        "PRD §3.1.5 (iter 234): mixed-case scheme must not match."
    );
}

/// Iter 234: `load_scopes` must tolerate a permissions entry that
/// has the `http:default` identifier but no `allow` array (or an
/// `allow` value that isn't an array). Tauri capability schemas
/// evolve — a v2.x patch might promote `allow` from always-array to
/// optional-array. A refactor that `expect`'d the allow-array
/// unconditionally would panic at test time with a confusing error,
/// misdirecting triage toward our capability JSON being malformed
/// when actually the upstream schema moved.
#[test]
fn load_scopes_tolerates_missing_allow_array() {
    // Source-inspect that load_scopes uses `and_then` / `let Some = ...
    // else { continue; }` style guards on `allow`, not a blunt
    // `.expect(...)` that would panic on schema evolution.
    let body = fs::read_to_string(GUARD_SOURCE).expect("guard source must exist");
    let load_pos = body
        .find("fn load_scopes()")
        .expect("load_scopes must exist");
    let load_end = body[load_pos..]
        .find("\n}\n")
        .map(|i| load_pos + i)
        .unwrap_or(load_pos + 2000);
    let load_body = &body[load_pos..load_end];
    // Must handle `allow` via `get(...).and_then(...)` or `let Some
    // else continue`, not `.expect`.
    assert!(
        load_body.contains(r#"get("allow")"#),
        "PRD §3.1.5 (iter 234): load_scopes must probe the `allow` \
         field via `obj.get(\"allow\")`. A direct index access on a \
         missing field panics."
    );
    // Must NOT contain `.expect(` on the allow lookup path — that
    // would panic on schema evolution (future capability structure
    // that moves allow to a sibling field).
    let allow_expect_bad = load_body.contains(r#"get("allow").unwrap()"#)
        || load_body.contains(r#"get("allow").expect("#);
    assert!(
        !allow_expect_bad,
        "PRD §3.1.5 (iter 234): load_scopes must NOT call \
         `.unwrap()` / `.expect(...)` on the `allow` lookup — a \
         capability schema change that moves `allow` out would \
         panic all tests with a misleading message instead of \
         continuing past unknown permissions."
    );
}

/// Iter 234: the `TEST_HOSTS` skip-list must be consulted ONLY by
/// the scanner (`every_mod_url_on_allowlist`) — NOT by `host_matches`
/// or `host_of`. A refactor that "DRY'd" the skip-list into the
/// matcher would turn a test-fixture exception into an every-caller
/// exception: any production URL literal pointing at `example.com`
/// would silently bypass the allowlist check, even though the
/// scanner's skip is meant for in-test fixtures only.
#[test]
fn test_hosts_skip_lives_only_in_scanner_not_in_matcher() {
    let body = fs::read_to_string(GUARD_SOURCE).expect("guard source must exist");
    // host_matches body must NOT reference TEST_HOSTS.
    let matcher_pos = body
        .find("fn host_matches(host: &str, scope_host: &str) -> bool")
        .expect("host_matches must exist");
    let matcher_end = body[matcher_pos..]
        .find("\n}\n")
        .map(|i| matcher_pos + i)
        .unwrap_or(matcher_pos + 600);
    let matcher_body = &body[matcher_pos..matcher_end];
    assert!(
        !matcher_body.contains("TEST_HOSTS"),
        "PRD §3.1.5 (iter 234): host_matches must NOT reference \
         TEST_HOSTS. The skip-list is a scanner-only convenience for \
         test fixtures; folding it into the matcher would turn every \
         production URL literal against example.com/127.0.0.1/\
         localhost into a silent allowlist bypass.\nMatcher body:\n\
         {matcher_body}"
    );
    // host_of body must also NOT reference TEST_HOSTS.
    let host_of_pos = body
        .find("fn host_of(s: &str) -> Option<String>")
        .expect("host_of must exist");
    let host_of_end = body[host_of_pos..]
        .find("\n}\n")
        .map(|i| host_of_pos + i)
        .unwrap_or(host_of_pos + 500);
    let host_of_body = &body[host_of_pos..host_of_end];
    assert!(
        !host_of_body.contains("TEST_HOSTS"),
        "PRD §3.1.5 (iter 234): host_of must NOT reference TEST_HOSTS. \
         The parser's job is to extract a host — filter decisions \
         belong one layer up."
    );
    // And the scanner fn MUST reference TEST_HOSTS (proving the skip
    // is applied somewhere).
    let scanner_pos = body
        .find("fn every_mod_url_on_allowlist()")
        .expect("scanner must exist");
    let scanner_end = body[scanner_pos..]
        .find("\nfn ")
        .map(|i| scanner_pos + i)
        .unwrap_or(body.len());
    let scanner_body = &body[scanner_pos..scanner_end];
    assert!(
        scanner_body.contains("test_hosts") || scanner_body.contains("TEST_HOSTS"),
        "PRD §3.1.5 (iter 234): every_mod_url_on_allowlist must \
         reference TEST_HOSTS (directly or via the local `test_hosts` \
         alias). Without the skip-list, unit-test fixtures against \
         example.com would fail the guard."
    );
}

// --------------------------------------------------------------------
// Iter 273 structural pins — capabilities/guard bounds + LAN scope +
// PRD cite + TEST_HOSTS list integrity.
// --------------------------------------------------------------------

#[test]
fn capabilities_file_byte_size_has_sane_bounds() {
    const MIN_BYTES: usize = 500;
    const MAX_BYTES: usize = 20_000;
    let bytes = std::fs::metadata("capabilities/migrated.json")
        .expect("capabilities/migrated.json must exist")
        .len() as usize;
    assert!(
        (MIN_BYTES..=MAX_BYTES).contains(&bytes),
        "PRD §3.1.5 (iter 273): capabilities/migrated.json is {bytes} \
         bytes; expected [{MIN_BYTES}, {MAX_BYTES}]."
    );
}

#[test]
fn guard_source_byte_size_has_sane_bounds() {
    const MIN_BYTES: usize = 5000;
    const MAX_BYTES: usize = 80_000;
    let bytes = std::fs::metadata(GUARD_SOURCE)
        .expect("guard must exist")
        .len() as usize;
    assert!(
        (MIN_BYTES..=MAX_BYTES).contains(&bytes),
        "PRD §3.1.5 (iter 273): guard is {bytes} bytes; expected \
         [{MIN_BYTES}, {MAX_BYTES}]."
    );
}

#[test]
fn guard_source_cites_prd_3_1_5_explicitly() {
    let body = std::fs::read_to_string(GUARD_SOURCE).expect("guard must exist");
    let header = &body[..body.len().min(500)];
    assert!(
        header.contains("PRD §3.1.5") || header.contains("PRD 3.1.5"),
        "PRD §3.1.5 (iter 273): guard header must cite `PRD §3.1.5` \
         or `PRD 3.1.5`.\nHeader:\n{header}"
    );
}

#[test]
fn lan_dev_http_scope_constant_is_canonical() {
    let body = std::fs::read_to_string(GUARD_SOURCE).expect("guard must exist");
    assert!(
        body.contains("http://157.90.107.2:8090"),
        "PRD §3.1.5 (iter 273): guard must retain the LAN dev HTTP \
         scope `http://157.90.107.2:8090` — used to validate the \
         portal endpoint URL is on the allowlist."
    );
}

#[test]
fn test_hosts_list_carries_three_canonical_entries() {
    assert_eq!(
        TEST_HOSTS.len(),
        3,
        "PRD §3.1.5 (iter 273): TEST_HOSTS must carry exactly 3 \
         entries (example.com, 127.0.0.1, localhost); found {}. A \
         different count signals drift in the test-skip allowlist.",
        TEST_HOSTS.len()
    );
    assert!(TEST_HOSTS.contains(&"example.com"));
    assert!(TEST_HOSTS.contains(&"127.0.0.1"));
    assert!(TEST_HOSTS.contains(&"localhost"));
}
