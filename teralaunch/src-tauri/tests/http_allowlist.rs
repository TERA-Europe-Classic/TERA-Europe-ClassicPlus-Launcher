//! PRD §3.1.5 — http allowlist hygiene.
//!
//! Walks every `.rs` file under `src/services/mods/`, extracts URL literals,
//! and asserts each production URL's host resolves against at least one
//! entry in `tauri.conf.json::tauri.allowlist.http.scope`. If a future
//! commit adds a URL literal to mods code without updating the allowlist,
//! this test fails in CI.
//!
//! Test-only hosts (`example.com`, `127.0.0.1`, `localhost`) are skipped so
//! unit-test fixtures don't have to pollute the production allowlist.

use std::fs;
use std::path::PathBuf;

use regex::Regex;
use serde_json::Value;

fn load_scopes() -> Vec<String> {
    let body = fs::read_to_string("tauri.conf.json").expect("tauri.conf.json must exist");
    let v: Value = serde_json::from_str(&body).expect("tauri.conf.json must be valid JSON");
    v["tauri"]["allowlist"]["http"]["scope"]
        .as_array()
        .expect("http.scope must be an array")
        .iter()
        .map(|s| s.as_str().expect("scope entries must be strings").to_string())
        .collect()
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
    let scope_hosts: Vec<String> = scopes
        .iter()
        .filter_map(|s| host_of(s))
        .collect();
    assert!(
        !scope_hosts.is_empty(),
        "failed to extract any hosts from allowlist scopes: {scopes:#?}"
    );

    let url_re = Regex::new(r#"https?://[^"\s\\)]+"#).expect("URL regex compiles");
    let test_hosts = ["example.com", "127.0.0.1", "localhost"];

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
                violations.push(format!("{}: unparseable URL literal: {url}", file.display()));
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
    assert!(host_matches("deep.sub.tera-europe.net", "*.tera-europe.net"));
    // Positive — exact.
    assert!(host_matches("raw.githubusercontent.com", "raw.githubusercontent.com"));
    // Negative — bare suffix must not match (prevents *.evil.com hijacking
    // something.evil.com.attacker.net).
    assert!(!host_matches("tera-europe.net", "*.tera-europe.net"));
    assert!(!host_matches("nottera-europe.net", "*.tera-europe.net"));
    // Negative — different host.
    assert!(!host_matches("example.com", "raw.githubusercontent.com"));
}

#[test]
fn host_of_strips_scheme_and_port() {
    assert_eq!(host_of("https://example.com/path"), Some("example.com".into()));
    assert_eq!(host_of("http://192.168.1.128:8090/"), Some("192.168.1.128".into()));
    assert_eq!(
        host_of("https://raw.githubusercontent.com/a/b/c.json"),
        Some("raw.githubusercontent.com".into())
    );
    assert_eq!(host_of("ftp://nope.com/"), None);
}
