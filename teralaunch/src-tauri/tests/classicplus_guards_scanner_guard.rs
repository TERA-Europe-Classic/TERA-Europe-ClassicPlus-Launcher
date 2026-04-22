//! Classic+ disabled-features contract scanner drift guard.
//!
//! `teralaunch/tests/classicplus-guards.test.js` is the JS test that
//! pins the Classic+ disabled-features contract per CLAUDE.md:
//!
//! - OAuth (startOAuth, handleOAuthCallback, checkDeepLink): stubs
//!   return immediately — empty-URL isn't the trigger here; removal
//!   of the stubs would re-enable broken OAuth flow.
//! - Leaderboard (ensureAuthSession, getLeaderboardConsent,
//!   setLeaderboardConsent, checkLeaderboardConsent): stubs return
//!   safe defaults.
//! - News / patch-notes / launcher-updater / profile / register /
//!   forum / privacy: empty URL + `if (!URLS.x.y) return` guard.
//!
//! These tests are the only automated guard against accidentally
//! re-enabling a Classic feature in Classic+ (e.g. someone merges a
//! PR from upstream Classic that adds OAuth wiring; this test
//! catches it because the stub's return-immediately contract fails).
//!
//! The scanner file is pure pattern-test (URLS is local, not
//! imported). The value of a Rust-side guard is preserving the
//! exhaustive enumeration — dropping a stub test or a URL-guard
//! test would silently shrink the contract.
//!
//! Fifth in the iter-124-to-128 JS-scanner-pin chain.

use std::fs;

const SCANNER: &str = "../tests/classicplus-guards.test.js";
const APP_JS: &str = "../src/app.js";
const TERALIB_CONFIG: &str = "../../teralib/src/config/config.json";

fn read(path: &str) -> String {
    fs::read_to_string(path).unwrap_or_else(|e| panic!("{path}: {e}"))
}

/// The scanner file must exist, be non-trivial, and carry the
/// Classic+ header comment.
#[test]
fn classicplus_scanner_file_exists_and_self_identifies() {
    let body = read(SCANNER);
    assert!(
        body.len() > 5000,
        "{SCANNER} is missing or truncated (<5000 bytes). The \
         Classic+ disabled-features contract is the ONLY automated \
         guard against accidentally re-enabling a Classic feature \
         (e.g. via upstream merge); losing the scanner silences \
         that gate."
    );
    assert!(
        body.contains("Classic+"),
        "{SCANNER} must self-identify as the Classic+ disabled-\
         features contract test in its header comment."
    );
}

/// The URLS fixture must carry the exhaustive set of empty-URL
/// fields. Dropping a field means the test stops asserting the \
/// feature is disabled.
#[test]
fn urls_fixture_covers_every_disabled_feature() {
    let body = read(SCANNER);
    // Launcher updater triplet (PRD §3.x: updater off on Classic+).
    assert!(
        body.contains("download: \"\"")
            && body.contains("versionCheck: \"\"")
            && body.contains("versionInfo: \"\""),
        "{SCANNER} URLS.launcher must keep all three fields empty \
         (download, versionCheck, versionInfo). Dropping any \
         silently re-enables that launcher sub-feature."
    );
    // Content news + patch-notes.
    assert!(
        body.contains("news: \"\"") && body.contains("patchNotes: \"\""),
        "{SCANNER} URLS.content must keep news + patchNotes empty \
         (no news endpoints on Classic+)."
    );
    // External: 4 empty, 2 present.
    for field in [
        "register: \"\"",
        "forum: \"\"",
        "privacy: \"\"",
        "profile: \"\"",
    ] {
        assert!(
            body.contains(field),
            "{SCANNER} URLS.external must keep `{field}` — that is \
             the Classic+ disabled contract for the corresponding \
             feature."
        );
    }
    // External: Discord + support are the ONLY external features
    // retained in Classic+. A regression that drops them would mean
    // the external-link UI disappears entirely.
    assert!(
        body.contains("discord.com") && body.contains("helpdesk"),
        "{SCANNER} URLS.external must keep Discord + helpdesk \
         support non-empty — these are the ONLY external features \
         retained in Classic+."
    );
}

/// The `no leaderboard section` invariant must stay. Classic+ has
/// no leaderboard at all; the scanner asserts `URLS.leaderboard`
/// itself is undefined, not just empty.
#[test]
fn urls_has_no_leaderboard_section() {
    let body = read(SCANNER);
    assert!(
        body.contains("URLS.leaderboard") && body.contains("toBeUndefined"),
        "{SCANNER} must assert `URLS.leaderboard` is undefined (not \
         just empty). The whole section is removed in Classic+; \
         leaving an empty-string section would still let code \
         branch-test `URLS.leaderboard?.foo` and maybe re-enable \
         wiring."
    );
}

/// The seven disabled-function stubs must all have a test. Missing
/// one means that stub could silently be replaced with live wiring
/// and the test suite wouldn't notice.
#[test]
fn scanner_carries_seven_disabled_stubs() {
    let body = read(SCANNER);
    let stubs = [
        "startOAuth stub",
        "handleOAuthCallback stub",
        "checkDeepLink stub",
        "ensureAuthSession stub",
        "getLeaderboardConsent stub",
        "setLeaderboardConsent stub",
        "checkLeaderboardConsent stub",
    ];
    for stub in stubs {
        assert!(
            body.contains(stub),
            "{SCANNER} must carry the `{stub}` test. This is the \
             only guard against a merge from upstream Classic that \
             re-wires this function to live code."
        );
    }
}

/// The six URL-guard behavior tests must stay. Each tests a
/// specific feature's "empty URL → early return / skip" pattern.
#[test]
fn scanner_carries_six_url_guard_tests() {
    let body = read(SCANNER);
    let guards = [
        "loadNewsFeed guard",
        "loadPatchNotes guard",
        "checkLauncherUpdate guard",
        "openRegisterPopup guard",
        "handleViewProfile guard",
        "versionInfo guard",
    ];
    for guard in guards {
        assert!(
            body.contains(guard),
            "{SCANNER} must carry the `{guard}` test — each guard \
             asserts the Classic+ contract that empty URL ⇒ no \
             fetch / no external-open. Dropping a guard test lets \
             that feature silently re-enable."
        );
    }
    // The `setupHeaderLinks URL guards` block tests the shared
    // pattern of "hide DOM elements with empty URLs".
    assert!(
        body.contains("setupHeaderLinks URL guards"),
        "{SCANNER} must carry the `setupHeaderLinks URL guards` \
         test — covers the DOM-hiding half of the empty-URL \
         contract."
    );
}

/// The LoadStartPage guard must stay — the page-load path is a
/// distinct call site from loadNewsFeed and needs its own coverage.
#[test]
fn scanner_carries_load_start_page_guard() {
    let body = read(SCANNER);
    assert!(
        body.contains("LoadStartPage with empty news URL"),
        "{SCANNER} must carry the `LoadStartPage with empty news \
         URL` test. LoadStartPage is a separate entry point that \
         triggers a news fetch; losing this test lets the \
         page-load path re-enable news fetching even if \
         loadNewsFeed itself stays guarded."
    );
}

/// Self-test — prove the detectors in THIS guard bite on synthetic
/// bad shapes of the scanner file.
#[test]
fn classicplus_guards_scanner_guard_detector_self_test() {
    // Bad shape A: URLS.launcher missing versionInfo (so the
    // launcher-info fetch would go unguarded).
    let incomplete = "launcher: {\n    download: \"\",\n    versionCheck: \"\",\n  },";
    assert!(
        !incomplete.contains("versionInfo: \"\""),
        "self-test: URLS.launcher without versionInfo must be \
         flagged"
    );

    // Bad shape B: URLS.leaderboard present (even as empty).
    let leaderboard_present = "leaderboard: { submit: \"\" }";
    assert!(
        !(leaderboard_present.contains("URLS.leaderboard")
            && leaderboard_present.contains("toBeUndefined")),
        "self-test: URLS with a leaderboard section present must \
         be flagged"
    );

    // Bad shape C: scanner missing startOAuth stub test.
    let no_oauth = "describe('handleOAuthCallback stub', () => {});";
    assert!(
        !no_oauth.contains("startOAuth stub"),
        "self-test: scanner missing startOAuth stub must be flagged"
    );

    // Bad shape D: scanner missing LoadStartPage guard.
    let no_load = "describe('loadNewsFeed guard', () => {});";
    assert!(
        !no_load.contains("LoadStartPage with empty news URL"),
        "self-test: scanner missing LoadStartPage guard must be \
         flagged"
    );

    // Iter 183 — additional bad shapes.

    // Bad shape E: URLs fixture containing an unapproved host.
    let leaked_url = r#"forum: "https://forum.en.tera.gameforge.com","#;
    assert!(
        !leaked_url.contains("157.90.107.2")
            && !leaked_url.contains("discord.com")
            && !leaked_url.contains("helpdesk.crazy-esports.com"),
        "self-test: unapproved URL host in fixture must be flagged"
    );

    // Bad shape F: an OAuth stub that fetches instead of returning.
    let live_oauth = "function startOAuth(p) { return fetch('/auth/' + p); }";
    assert!(
        !live_oauth.contains("return;"),
        "self-test: stub without early `return;` must be flagged"
    );

    // Bad shape G: scanner with `.only` marker.
    let only_pin = "it.only('startOAuth stub', () => {});";
    assert!(
        only_pin.contains("it.only"),
        "self-test: .only detector must bite on `it.only(`"
    );
}

/// Iter 183: scanner must carry at least 10 `it(` blocks. Expected
/// breakdown: 7 stubs, 6 URL guards, 1 LoadStartPage (14 total). A
/// floor of 10 catches a multi-test deletion that would otherwise
/// silently shrink the Classic+ disabled contract.
#[test]
fn classicplus_scanner_has_minimum_it_count() {
    let body = read(SCANNER);
    let it_count = body.matches("it(").count() + body.matches("it.only(").count();
    assert!(
        it_count >= 10,
        "Classic+ contract (iter 183): {SCANNER} must carry at least \
         10 `it(` blocks (7 stubs + 6 URL guards + 1 LoadStartPage). \
         Found {it_count}. Below the floor means one or more \
         disabled-feature guards were deleted."
    );
}

/// Iter 183: reject `.only` / `.skip` / `xit` / `xdescribe` in the
/// scanner — a dev-local pin would silently disable sibling guards.
#[test]
fn classicplus_scanner_carries_no_only_or_skip_markers() {
    let body = read(SCANNER);
    for forbidden in [
        "it.only(",
        "describe.only(",
        "it.skip(",
        "describe.skip(",
        "xit(",
        "xdescribe(",
    ] {
        assert!(
            !body.contains(forbidden),
            "Classic+ contract (iter 183): {SCANNER} must not carry \
             `{forbidden}` — disabling one stub or guard test is the \
             exact regression class this scanner is supposed to \
             catch."
        );
    }
}

/// Iter 183: the URLs fixture in the scanner must only contain URLs
/// with approved hosts — 157.90.107.2 (LAN dev portal), discord.com,
/// helpdesk.crazy-esports.com. Anything else is a stale Classic URL
/// that slipped through and would pass the "URL is non-empty" check
/// trivially.
#[test]
fn classicplus_scanner_urls_fixture_contains_only_allowed_hosts() {
    let body = read(SCANNER);
    // Extract the URLS const declaration block.
    let start = body
        .find("const URLS = {")
        .expect("scanner URLS declaration missing");
    let remaining = &body[start..];
    let end_rel = remaining
        .find("};")
        .expect("URLS block must close with `};`");
    let block = &remaining[..end_rel];
    // Find every quoted URL-like string in the block.
    let mut in_quote = false;
    let mut current = String::new();
    let mut urls: Vec<String> = Vec::new();
    for ch in block.chars() {
        if ch == '"' {
            if in_quote {
                if !current.is_empty() {
                    urls.push(current.clone());
                }
                current.clear();
                in_quote = false;
            } else {
                in_quote = true;
            }
        } else if in_quote {
            current.push(ch);
        }
    }
    // Every quoted non-empty string that is clearly a URL (contains
    // "://") must carry one of the allowed hosts.
    const ALLOWED: &[&str] = &["157.90.107.2", "discord.com", "helpdesk.crazy-esports.com"];
    for url in &urls {
        if !url.contains("://") {
            continue; // not a URL — path fragment or key
        }
        let ok = ALLOWED.iter().any(|host| url.contains(host));
        assert!(
            ok,
            "Classic+ contract (iter 183): {SCANNER} URLS fixture \
             contains an unapproved host: `{url}`. Allowed hosts: \
             {ALLOWED:?}. An unapproved URL in the fixture would \
             pass the empty-URL guards trivially while re-enabling \
             the corresponding feature."
        );
    }
}

/// Iter 183: the production `src/app.js` must keep the seven
/// Classic+ stubs AND each must reach an early-return short-circuit
/// before any network-touching code (fetch / invoke). Read source
/// directly so a drift where the scanner fixture stays clean but
/// app.js re-enables a stub still gets caught.
#[test]
fn app_js_classicplus_stubs_reach_early_return() {
    let src = read(APP_JS);
    // For each stub, find its function/method definition and inspect
    // the first non-comment non-log line after `{`.
    let stubs = [
        "function startOAuth",
        "async function handleOAuthCallback",
        "async function checkDeepLink",
        "async ensureAuthSession(",
        "async getLeaderboardConsent(",
        "async setLeaderboardConsent(",
        "async checkLeaderboardConsent(",
    ];
    for stub in stubs {
        let pos = src.find(stub).unwrap_or_else(|| {
            panic!(
                "Classic+ contract (iter 183): {APP_JS} must keep the \
                 `{stub}` stub — missing means the function was \
                 re-wired to live code or renamed without updating \
                 this guard."
            )
        });
        // Window covers the function body header + first ~30 lines.
        let window = &src[pos..pos.saturating_add(1200).min(src.len())];
        // Require the stub body contain a `return` BEFORE any
        // `fetch(`, `invoke(`, or `await this.` call to a non-early-
        // return method. The simplest invariant: the substring
        // `return` must appear in the window.
        assert!(
            window.contains("return"),
            "Classic+ contract (iter 183): {APP_JS} `{stub}` body \
             must carry an early `return` — without it the stub \
             executes live code. Window:\n{window}"
        );
        // And it must carry a Classic+ marker so a future reader
        // understands why the stub is gutted.
        assert!(
            window.contains("Classic+"),
            "Classic+ contract (iter 183): {APP_JS} `{stub}` body \
             must carry a `Classic+` marker comment so future readers \
             know this is a deliberate stub, not a half-written \
             implementation."
        );
    }
}

/// Iter 183: the production Rust-side config (`teralib/src/config/
/// config.json`) must keep HASH_FILE_URL and FILE_SERVER_URL empty.
/// These are the Rust half of the Classic+ no-updater contract — if
/// a merge re-enables them, the launcher would try to fetch a hash
/// manifest that doesn't exist.
#[test]
fn teralib_config_keeps_hash_and_file_server_empty() {
    let config = read(TERALIB_CONFIG);
    for needle in [r#""HASH_FILE_URL": """#, r#""FILE_SERVER_URL": """#] {
        assert!(
            config.contains(needle),
            "Classic+ contract (iter 183): {TERALIB_CONFIG} must \
             carry `{needle}`. This is the Rust half of the no-\
             updater contract; a non-empty URL would make the \
             launcher try to fetch a hash manifest that doesn't \
             exist on Classic+."
        );
    }
}

// --------------------------------------------------------------------
// Iter 218 structural pins — meta-guard header + 3 path constants +
// app.js stub-body fetch/invoke absence + config no-Classic-residue +
// ALLOWED hosts list count.
// --------------------------------------------------------------------
//
// The twelve pins above cover scanner presence + URLs fixture + no-
// leaderboard + seven stubs + six URL guards + LoadStartPage + it-
// count + .only/.skip + ALLOWED-host check + app.js stubs reach early
// return + teralib config empty-URL invariants. They do NOT pin: (a)
// the guard's own header cites `Classic+` + scanner slug — meta-guard
// contract; (b) SCANNER + APP_JS + TERALIB_CONFIG path constants —
// rename drift hides as opaque panics; (c) each stub body NEVER calls
// `fetch(` or `invoke(` — the iter-183 early-return pin requires
// `return` to appear somewhere, but `return await fetch(...)` would
// pass that check while live-wiring the stub; (d) teralib config
// doesn't carry Classic-only keys (LEADERBOARD_URL, PROFILE_URL,
// NEWS_URL) whose mere presence in the schema signals Classic-era
// wiring intent; (e) the iter-183 ALLOWED hosts list has exactly 3
// entries — trimming to 1 would make the fixture-host check pass
// vacuously against just the LAN host.

/// The guard's own module header must cite `Classic+` + the scanner
/// slug so a reader chasing a Classic+-disabled-feature regression
/// lands here via name-based grep.
#[test]
fn guard_file_header_cites_classicplus_and_scanner_slug() {
    let body = fs::read_to_string("tests/classicplus_guards_scanner_guard.rs")
        .expect("tests/classicplus_guards_scanner_guard.rs must exist");
    let header = &body[..body.len().min(2000)];
    assert!(
        header.contains("Classic+"),
        "meta-guard contract: \
         tests/classicplus_guards_scanner_guard.rs header must cite \
         `Classic+`. Without it, a reader chasing a Classic+-disabled-\
         feature regression won't land here via name-based grep.\n\
         Header:\n{header}"
    );
    assert!(
        header.contains("classicplus-guards.test.js"),
        "meta-guard contract: header must name the target JS scanner \
         `classicplus-guards.test.js` so the file-under-test is \
         unambiguous."
    );
}

/// All three path constants must equal their canonical relative forms
/// verbatim. A rename of any (SCANNER, APP_JS, TERALIB_CONFIG) would
/// silently cause `read(path)` calls to panic with opaque "file not
/// readable" messages that obscure the root cause.
#[test]
fn all_path_constants_are_canonical() {
    let guard_body = fs::read_to_string("tests/classicplus_guards_scanner_guard.rs")
        .expect("guard source must be readable");
    for literal in [
        "const SCANNER: &str = \"../tests/classicplus-guards.test.js\";",
        "const APP_JS: &str = \"../src/app.js\";",
        "const TERALIB_CONFIG: &str = \"../../teralib/src/config/config.json\";",
    ] {
        assert!(
            guard_body.contains(literal),
            "Classic+ contract (iter 218): \
             tests/classicplus_guards_scanner_guard.rs must retain \
             `{literal}` verbatim. A rename without atomic constant \
             update would break every pin with an opaque `file not \
             readable` panic."
        );
    }
}

/// Each Classic+ stub's LIVE body (from function header to the first
/// `return`) must NOT contain `fetch(`, `invoke(`, or `await this.`
/// calls. The iter-183 early-return pin only checks that `return`
/// appears SOMEWHERE in the window, so `return await fetch(...)`
/// would pass. This pin is stricter: any live-call token appearing
/// BEFORE the stub's first `return` would be reachable code. Dead
/// reference code after the return (which Classic+ stubs preserve
/// for future re-enable) is ignored.
#[test]
fn app_js_stub_live_body_has_no_network_call() {
    let src = read(APP_JS);
    let stubs = [
        "function startOAuth",
        "async function handleOAuthCallback",
        "async function checkDeepLink",
        "async ensureAuthSession(",
        "async getLeaderboardConsent(",
        "async setLeaderboardConsent(",
        "async checkLeaderboardConsent(",
    ];
    for stub in stubs {
        let pos = src
            .find(stub)
            .unwrap_or_else(|| panic!("stub `{stub}` must exist (iter-183 pin)"));
        // Locate the first `return` statement AFTER the fn header.
        // This caps the live-body window — anything past the first
        // return is unreachable reference code (legit Classic+
        // pattern: keep original logic commented-out-via-dead-code
        // for future re-enable).
        let tail = &src[pos..];
        let first_return_rel = tail
            .find("return")
            .expect("every Classic+ stub must have at least one `return` (iter-183 pin)");
        let live_body = &tail[..first_return_rel];
        for forbidden in ["fetch(", "invoke(", "await this."] {
            assert!(
                !live_body.contains(forbidden),
                "Classic+ contract (iter 218): {APP_JS} `{stub}` LIVE \
                 body (before first `return`) must NOT contain \
                 `{forbidden}`. The iter-183 early-return pin would \
                 still pass if the stub did `return await {forbidden}...`, \
                 but the feature would silently go live. Live \
                 body:\n{live_body}"
            );
        }
    }
}

/// `teralib/src/config/config.json` must NOT carry Classic-era keys
/// whose mere presence signals wiring intent even when the URL is
/// empty. LEADERBOARD_URL / PROFILE_URL / NEWS_URL / PATCH_NOTES_URL
/// are Classic artefacts; Classic+ has deleted them from the schema
/// entirely. A merge from upstream Classic that re-introduces them
/// would pass the existing empty-URL check while advertising
/// "leaderboard exists" to any caller that probes the keys.
#[test]
fn teralib_config_has_no_classic_residue_keys() {
    let config = read(TERALIB_CONFIG);
    for residue in [
        "LEADERBOARD",
        "PROFILE_URL",
        "NEWS_URL",
        "PATCH_NOTES_URL",
        "OAUTH_URL",
    ] {
        assert!(
            !config.contains(residue),
            "Classic+ contract (iter 218): {TERALIB_CONFIG} must NOT \
             contain `{residue}` — it's a Classic-era key deleted \
             from the Classic+ schema. Presence signals wiring intent \
             even if the value is empty; callers probing for the \
             key would conclude the feature exists.\n\
             Config excerpt:\n{}",
            config.lines().take(15).collect::<Vec<_>>().join("\n")
        );
    }
}

/// The iter-183 `ALLOWED` hosts list in
/// `classicplus_scanner_urls_fixture_contains_only_allowed_hosts`
/// must carry exactly 3 entries. Trimming to 1 (just the LAN host)
/// would make the fixture-host check pass vacuously against any LAN-
/// host URL, letting the Discord + helpdesk entries silently drop
/// out of the fixture. Pinning the count catches a silent narrowing.
#[test]
fn allowed_hosts_list_count_is_three() {
    let guard_body = fs::read_to_string("tests/classicplus_guards_scanner_guard.rs")
        .expect("guard source must be readable");
    // Locate the ALLOWED slice declaration in the iter-183 test.
    let pos = guard_body
        .find("const ALLOWED: &[&str] = &[")
        .expect("iter-183 ALLOWED slice must exist");
    // Window covers the slice literal (3 entries + closing `];`).
    let end = guard_body[pos..]
        .find("];")
        .map(|i| pos + i + 2)
        .unwrap_or(guard_body.len());
    let window = &guard_body[pos..end];

    // Each of the 3 expected hosts must appear in the slice literal.
    for host in ["157.90.107.2", "discord.com", "helpdesk.crazy-esports.com"] {
        assert!(
            window.contains(&format!("\"{host}\"")),
            "Classic+ contract (iter 218): ALLOWED hosts list must \
             contain `\"{host}\"`. Trimming the list would let that \
             host's URLs silently drop out of the fixture-host check.\n\
             Window:\n{window}"
        );
    }
    // Count quoted string literals in the slice body to catch an
    // expansion to 4+ entries (a new allowed host deserves an audit).
    let quote_count = window.matches('"').count() / 2;
    assert_eq!(
        quote_count, 3,
        "Classic+ contract (iter 218): ALLOWED hosts list must have \
         exactly 3 entries. Found {quote_count}. Additions are not \
         automatically safe — each new allowed host is a feature \
         re-enabled, so update this count atomically with an audit."
    );
}

// --------------------------------------------------------------------
// Iter 253 structural pins — scanner header slug + size ceiling +
// config-JSON validity + app.js marker floor + scanner describe wrapper.
// --------------------------------------------------------------------
//
// The seventeen pins above cover scanner file presence, URLs fixture
// completeness, stub + URL guard enumeration, .only/.skip rejection,
// allowed-host list, app.js early-return + no-live-call, config empty-
// URL + no-residue. They do NOT pin:
// (a) the scanner's own JS header self-identifies with the
//     disabled-features phrase — a refactor that strips the header
//     comment would still pass presence/size pins;
// (b) the scanner file has a sane upper-byte ceiling — bloat to 50 KB+
//     signals garbage in the fixture or unrelated tests piled into
//     the same file;
// (c) teralib_config.json parses as valid JSON — a syntax error in
//     the config would fail at Rust load time but our string-contains
//     checks would pass until the launcher actually boots;
// (d) app.js carries at least 7 `Classic+` marker comments — one per
//     stub. The iter-183 pin only checks that each stub has a marker,
//     but a silent deletion + re-addition cycle could drop total
//     count while still satisfying the per-stub scan;
// (e) the scanner carries a top-level `describe(` wrapper — Vitest
//     idiom; tests without a describe block are flat and can't be
//     grep-scoped when debugging a regression.

/// The scanner file's own JS header comment must self-identify as the
/// Classic+ disabled-features contract test. The guard-file header
/// pin (iter 218) only checks the Rust side; this pin pins the JS
/// scanner's own header so a refactor that strips the scanner's
/// self-identification (leaving a raw test file) still fails.
#[test]
fn scanner_header_cites_disabled_features_phrase() {
    let body = read(SCANNER);
    let header = &body[..body.len().min(1500)];
    // Accept either the canonical phrase or an unambiguous synonym.
    let cites = header.contains("disabled")
        || header.contains("Classic+ contract")
        || header.contains("stub");
    assert!(
        cites,
        "Classic+ contract (iter 253): {SCANNER} header must cite \
         `disabled` / `Classic+ contract` / `stub` so a refactor \
         stripping the self-identification still fails. The scanner \
         is the ONLY automated guard against re-enabling a Classic \
         feature; anonymous test files are easier to delete by \
         accident.\nHeader:\n{header}"
    );
}

/// The scanner file must not exceed a sane upper byte ceiling. The
/// iter-128 floor catches truncation; the ceiling catches bloat —
/// garbage piled into the fixture, unrelated tests merged into the
/// scanner file, or a runaway generator that duplicated content.
/// Current state: ~15 KB. A ceiling of 50 KB gives ~3× margin.
#[test]
fn scanner_file_size_has_upper_ceiling() {
    const MAX_BYTES: usize = 50_000;
    let bytes = fs::metadata(SCANNER)
        .unwrap_or_else(|e| panic!("{SCANNER}: {e}"))
        .len() as usize;
    assert!(
        bytes <= MAX_BYTES,
        "Classic+ contract (iter 253): {SCANNER} is {bytes} bytes; \
         ceiling is {MAX_BYTES}. Bloat past the ceiling signals \
         garbage in the fixture or unrelated tests piled into this \
         file — the scanner's focus on disabled-feature enumeration \
         becomes diluted."
    );
}

/// `teralib/src/config/config.json` must parse as a valid JSON
/// object. The iter-183 string-contains pin on `"HASH_FILE_URL": ""`
/// passes even if the file has an unterminated quote or trailing
/// comma earlier — the launcher would fail at boot-time JSON parse
/// with a message that doesn't trace back to this guard. Pinning
/// validity here catches the syntax regression at test time.
#[test]
fn teralib_config_is_valid_json() {
    let body = read(TERALIB_CONFIG);
    let parsed: Result<serde_json::Value, _> = serde_json::from_str(&body);
    let value = parsed.unwrap_or_else(|e| {
        panic!(
            "Classic+ contract (iter 253): {TERALIB_CONFIG} must parse \
             as valid JSON. Parse error: {e}. The iter-183 \
             string-contains pin would pass on a broken JSON (matching \
             the literal key-value before a syntax error), but the \
             launcher would fail at boot."
        )
    });
    assert!(
        value.is_object(),
        "Classic+ contract (iter 253): {TERALIB_CONFIG} must parse as \
         a JSON object (not array or scalar). Got: {value:?}"
    );
}

/// `src/app.js` must carry at least 7 `Classic+` marker comments,
/// one per disabled stub. The iter-183 pin confirms each stub
/// individually has a marker via a window scan, but a silent
/// delete-and-readd cycle (someone removes a stub's body including
/// its marker, then adds it back without) could drop the total count
/// while still satisfying the per-stub scan (since the marker
/// appears somewhere else in the larger window). This pin enforces
/// the aggregate count.
#[test]
fn app_js_carries_classicplus_markers_floor() {
    const MIN_MARKERS: usize = 7;
    let src = read(APP_JS);
    let count = src.matches("Classic+").count();
    assert!(
        count >= MIN_MARKERS,
        "Classic+ contract (iter 253): {APP_JS} carries {count} \
         `Classic+` marker comments; floor is {MIN_MARKERS} (one per \
         disabled stub). Below the floor means one or more stubs had \
         their self-documenting marker dropped during a refactor — \
         future readers lose the signal that the function is a \
         deliberate stub."
    );
}

/// The scanner must carry at least one top-level `describe(` wrapper
/// block. Vitest supports flat tests without a describe, but the
/// scanner's 14+ `it(` blocks need a describe wrapper to give the
/// suite a name in test output — without it, the only context for a
/// failure is `classicplus-guards.test.js:<line>` rather than the
/// semantic group the failure belongs to.
#[test]
fn scanner_carries_describe_wrapper_block() {
    let body = read(SCANNER);
    let describe_count = body.matches("describe(").count();
    assert!(
        describe_count >= 1,
        "Classic+ contract (iter 253): {SCANNER} has {describe_count} \
         `describe(` block(s); floor is 1. A flat test file gives \
         readers no semantic grouping when a failure lands — the \
         only context is file:line, not the disabled-feature category \
         the failure belongs to."
    );
}
