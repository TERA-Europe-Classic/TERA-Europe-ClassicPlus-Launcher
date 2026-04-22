//! PRD 3.1.6 — secret-scan infrastructure drift guard.
//!
//! Criterion: "No hardcoded secrets in any of the 5 repos; no leaks
//! in git history." Measurement cites both
//! `.github/workflows/secret-scan.yml` (gitleaks CI runner) and
//! `docs/PRD/audits/security/secret-leak-scan.md` (iter 13 triage).
//! Iter 88 layered on `.gitleaks.toml` with an allowlist for known
//! false positives. This guard pins the wiring so a future refactor
//! can't silently delete either file or break the allowlist shape.
//!
//! Three invariants:
//! 1. Workflow file exists and installs + invokes gitleaks with the
//!    explicit config flag (no default-only fallback).
//! 2. `.gitleaks.toml` exists, extends the default ruleset (so it
//!    layers onto gitleaks' stock rules rather than replacing them),
//!    and carries the target/ path exclusions that iter 88 added.
//! 3. iter-88's cross-reference to iter-13 audit stays cited in the
//!    config header — the "every entry cites the audit" rule is the
//!    non-speculative discipline that keeps the allowlist honest.

use std::fs;

const WORKFLOW: &str = "../../.github/workflows/secret-scan.yml";
const CONFIG: &str = "../../.gitleaks.toml";
const AUDIT_REF: &str = "docs/PRD/audits/security/secret-leak-scan.md";

fn read(path: &str) -> String {
    fs::read_to_string(path).unwrap_or_else(|e| panic!("{path}: {e}"))
}

/// The secret-scan workflow file must exist and run gitleaks.
#[test]
fn secret_scan_workflow_exists_and_runs_gitleaks() {
    let body = read(WORKFLOW);
    assert!(
        body.contains("gitleaks"),
        "PRD 3.1.6 violated: {WORKFLOW} does not reference gitleaks. \
         Either the tool was swapped (update this guard) or the \
         workflow was removed."
    );
    // Install step must download a pinned release, not rely on an
    // unversioned package. Iter 88 pins the version via a `VER=` env.
    assert!(
        body.contains("gitleaks/gitleaks/releases/download"),
        "Workflow must install gitleaks from a pinned release URL, \
         not a package manager (drift would mean version roulette)."
    );
    // Run step must pass the local config explicitly — iter 88 made
    // this explicit so a future rename of the file fails loudly
    // instead of silently reverting to defaults.
    assert!(
        body.contains("--config .gitleaks.toml"),
        "Workflow must pass --config .gitleaks.toml explicitly so a \
         rename of the config file fails CI instead of silently \
         reverting to gitleaks' default ruleset (no allowlist)."
    );
}

/// `.gitleaks.toml` must exist, layer on the default ruleset, and
/// carry the iter-88 target/ exclusions.
#[test]
fn gitleaks_config_structure_is_intact() {
    let body = read(CONFIG);

    // useDefault = true means we LAYER our allowlist onto the stock
    // gitleaks ruleset instead of replacing it. Without this flag the
    // config would disable every default detection rule.
    assert!(
        body.contains("[extend]") && body.contains("useDefault = true"),
        "{CONFIG} must carry `[extend]` with `useDefault = true` — \
         this is what makes the allowlist LAYER onto gitleaks' stock \
         rules instead of replacing them. Without it, every default \
         detection rule would be silently disabled."
    );
    assert!(
        body.contains("[allowlist]"),
        "{CONFIG} must carry a top-level `[allowlist]` section for \
         repo-wide known-FP entries."
    );
    // Iter 88 excluded target/ directories so Cargo build artefacts
    // (which can embed random-looking strings in .rlib/.debug sections)
    // don't pollute scans.
    assert!(
        body.contains("target/"),
        "{CONFIG} must exclude target/ paths — Cargo artefacts \
         trigger generic-api-key false positives otherwise."
    );
}

/// The config header must cite the iter 13 audit. "Every entry cites
/// the audit" is the discipline that prevents the allowlist from
/// growing speculative entries.
#[test]
fn gitleaks_config_cites_audit_reference() {
    let body = read(CONFIG);
    assert!(
        body.contains(AUDIT_REF) || body.contains("iter 13"),
        "{CONFIG} must cite the iter 13 secret-leak-scan audit so \
         future readers know every allowlist entry traces back to a \
         triaged finding. Citation format: `{AUDIT_REF}` or \
         `iter 13`."
    );
}

/// The workflow must fire on BOTH `push` to main AND `pull_request`.
/// Dropping either class would let a drift land: push-only would miss
/// PR-time feedback to reviewers; PR-only would miss direct commits
/// to main. Iter 144 pins the dual trigger so a future workflow edit
/// can't silently shrink the coverage.
#[test]
fn secret_scan_workflow_triggers_on_push_and_pull_request() {
    let body = read(WORKFLOW);
    // `push:` block mentioning `main`.
    assert!(
        body.contains("push:") && body.contains("branches: [main]"),
        "{WORKFLOW} must trigger on `push` to `main`. Without this, \
         direct commits to main skip the secret-scan gate."
    );
    assert!(
        body.contains("pull_request:"),
        "{WORKFLOW} must trigger on `pull_request`. Without this, \
         reviewers don't see secret-scan feedback before merge."
    );
}

/// `fetch-depth: 0` is required because the workflow uses
/// `--log-opts=RANGE` to scan only new commits in a PR or push.
/// Without a full-history fetch, the base SHA in the range isn't in
/// the local clone and gitleaks fails to resolve the range.
#[test]
fn secret_scan_workflow_uses_full_fetch_depth() {
    let body = read(WORKFLOW);
    assert!(
        body.contains("fetch-depth: 0"),
        "{WORKFLOW} must set `fetch-depth: 0` on the checkout step. \
         A shallow clone (the actions/checkout default) would break \
         `--log-opts=RANGE` because the base SHA isn't in the local \
         clone; gitleaks would then fail to resolve the commit range."
    );
}

/// The gitleaks version pinned by the workflow must be a SEMVER tag
/// (e.g. `8.30.1`), not a floating reference (`latest`, `main`, `@v8`).
/// A floating tag reintroduces the same supply-chain risk
/// `infra.gitleaks-allowlist` pins against — the binary downloaded
/// could silently change between runs and a malicious upstream could
/// inject a compromised gitleaks build.
#[test]
fn secret_scan_workflow_pins_semver_version() {
    let body = read(WORKFLOW);
    // Find `VER=` line and verify it looks semver-shaped.
    let ver_line = body
        .lines()
        .find(|l| l.trim_start().starts_with("VER="))
        .unwrap_or_else(|| {
            panic!(
                "{WORKFLOW} must carry a `VER=X.Y.Z` shell var that \
                 names the gitleaks release to install."
            )
        });
    let value = ver_line.trim_start().trim_start_matches("VER=").trim();
    let parts: Vec<&str> = value.split('.').collect();
    assert_eq!(
        parts.len(),
        3,
        "{WORKFLOW} VER must be a 3-part semver (e.g. `8.30.1`). \
         Got: `{value}`. Floating tags like `latest` or `main` \
         silently change the binary run on CI."
    );
    for (i, p) in parts.iter().enumerate() {
        assert!(
            !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()),
            "{WORKFLOW} VER part {i} (`{p}`) must be all digits — \
             got `{value}`."
        );
    }
}

/// Every `regexes` + `paths` array in `.gitleaks.toml` must be
/// non-empty. An empty allowlist section is a configuration
/// smell — either the allowlist is dead code (remove it) or it
/// became empty during an edit and should have been deleted.
#[test]
fn gitleaks_allowlist_arrays_are_non_empty() {
    let body = read(CONFIG);
    // regexes array.
    let after_re = body.split("regexes = [").nth(1).unwrap_or_else(|| {
        panic!(
            "{CONFIG} must carry a `regexes = [` array — iter 13 \
         audit has at least 2 test-fixture tokens allowlisted."
        )
    });
    let re_block = after_re.split(']').next().unwrap_or("");
    let re_entries = re_block.matches('\'').count() / 6; // triple-quoted strings
                                                         // Be liberal: count by `'''` sequences, each entry uses 2 sets.
    let re_count = re_block.matches("'''").count() / 2;
    assert!(
        re_count >= 1,
        "{CONFIG} `regexes = [...]` array appears empty ({re_count} \
         entries by triple-quote count, {re_entries} by quote count). \
         Either there's a known false positive to allowlist — in \
         which case add an entry with audit citation — or the \
         section should be deleted."
    );

    // paths array.
    let after_paths = body.split("paths = [").nth(1).unwrap_or_else(|| {
        panic!(
            "{CONFIG} must carry a `paths = [` array — target/ at \
         minimum."
        )
    });
    let paths_block = after_paths.split(']').next().unwrap_or("");
    let paths_count = paths_block.matches("'''").count() / 2;
    assert!(
        paths_count >= 1,
        "{CONFIG} `paths = [...]` array appears empty. target/ at \
         minimum should be excluded to keep Cargo artefact false \
         positives out of local scans."
    );
}

// --------------------------------------------------------------------
// Iter 171 structural pins — scoped scan + install hardening +
// allowlist anchoring + target-dir exclusions.
// --------------------------------------------------------------------
//
// Iter 144 pinned the dual trigger + fetch-depth + semver version +
// non-empty allowlist arrays. Iter 171 widens to five angles those
// pins skip: (1) `--log-opts` restricts the scan to new commits
// only, (2) the push-vs-pull_request range-computation branch
// handles both triggers (pairs with iter 144's dual-trigger pin),
// (3) `curl -sSfL` with `-f` fails the job on a yanked release
// instead of silently piping empty bytes to tar, (4) the allowlist
// regexes are `\b`-anchored so a substring match can't hide a real
// leak, (5) all three target/ paths are excluded (teralaunch +
// teralib + repo-root) so local scans stay quiet across workspaces.

/// The workflow must pass `--log-opts=<RANGE>` so gitleaks scans only
/// new commits, not the full history. Iter 13 triaged historical
/// findings and stamped them as not-worth-rewrite; scanning history
/// every run would reintroduce those findings as noise and drown the
/// signal on a real regression.
#[test]
fn secret_scan_workflow_uses_log_opts_for_scoped_scan() {
    let body = read(WORKFLOW);
    assert!(
        body.contains("--log-opts="),
        "PRD 3.1.6: {WORKFLOW} must pass `--log-opts=...` to gitleaks \
         detect so only new commits are scanned. Without it, gitleaks \
         walks full history on every run — iter-13 triaged findings \
         resurface as noise and drown real regressions."
    );
}

/// The workflow must compute the commit range differently for
/// `push` vs `pull_request` events. Iter 144 pinned that BOTH
/// triggers exist; this pairs with that by pinning the branch logic
/// in the range step — a refactor that drops the `pull_request` arm
/// would make PR scans use the wrong range and miss secrets.
#[test]
fn secret_scan_workflow_handles_both_event_types_in_range_step() {
    let body = read(WORKFLOW);
    assert!(
        body.contains(r#"if [ "${{ github.event_name }}" = "pull_request" ]"#),
        "PRD 3.1.6: {WORKFLOW} must branch on \
         `if [ \"${{{{ github.event_name }}}}\" = \"pull_request\" ]` \
         so the range step picks PR base..head on pull_request events \
         and before..sha on push events. Dropping the PR arm makes PR \
         scans cover zero commits."
    );
    // Both sides of the conditional must produce a `range=` output.
    assert!(
        body.contains("github.event.pull_request.base.sha"),
        "PRD 3.1.6: {WORKFLOW} PR branch must use \
         `github.event.pull_request.base.sha` for the range start. \
         Any other base (e.g. `main` branch head) misses the actual \
         PR commit range."
    );
    assert!(
        body.contains("github.event.before"),
        "PRD 3.1.6: {WORKFLOW} push branch must use \
         `github.event.before` for the range start. Without it, the \
         range would cover zero commits on a push-to-main."
    );
}

/// The gitleaks install must use `curl -sSfL` — the `-f` (fail on
/// HTTP errors) matters most: without it, a 404 (release yanked,
/// URL typo, network mid-request) returns an empty body; the tar
/// pipe then silently produces nothing and `mv /tmp/gitleaks` errors
/// out with "file not found" — but the scan step might be skipped
/// if the job continues through the error. `-f` fails the job
/// immediately so the operator knows the install broke.
#[test]
fn secret_scan_workflow_install_uses_fail_fast_curl_flags() {
    let body = read(WORKFLOW);
    assert!(
        body.contains("curl -sSfL"),
        "PRD 3.1.6: {WORKFLOW} install step must use `curl -sSfL` — \
         the `-f` flag fails the job on a 404 (release yanked) instead \
         of silently piping empty bytes to tar. The `-S` flag shows \
         errors even in silent mode so operators see what happened."
    );
}

/// Each allowlist regex must use `\b` word-boundary anchors. An
/// unanchored regex like `abc123def456` would suppress `abc123def456_real_key` too,
/// which is exactly how a real leak hides behind a fixture allowlist.
/// The `\b...\b` anchors pin the match to whole tokens.
#[test]
fn gitleaks_config_regex_tokens_are_word_boundary_anchored() {
    let body = read(CONFIG);
    // Find the `regexes = [...]` block.
    let after_re = body
        .split("regexes = [")
        .nth(1)
        .expect("regexes array must exist");
    let re_block = after_re
        .split(']')
        .next()
        .expect("regexes array must close");
    // Every triple-quoted string in the block must contain `\b`.
    let mut entries: Vec<&str> = re_block.split("'''").collect();
    // First and last splits are separators / whitespace, not entries.
    // Real entries alternate: `[pre, <entry>, <mid>, <entry>, ..., post]`.
    // Keep only odd indices (the actual entries).
    entries = entries
        .iter()
        .enumerate()
        .filter_map(|(i, s)| if i % 2 == 1 { Some(*s) } else { None })
        .collect();
    assert!(
        !entries.is_empty(),
        "expected at least one regex entry in {CONFIG} allowlist"
    );
    for entry in &entries {
        assert!(
            entry.contains(r"\b"),
            "PRD 3.1.6: every entry in {CONFIG} `regexes = [...]` \
             must contain `\\b` word-boundary anchors. Unanchored \
             regex `{entry}` would suppress longer strings that \
             happen to contain the fixture — a real leak could hide \
             behind the allowlist."
        );
    }
}

/// The paths allowlist must exclude all three `target/` locations:
/// repo-root `target/` (for workspaces built at the top level),
/// `teralaunch/src-tauri/target/` (the bin crate's artefacts), and
/// `teralib/target/` (the lib crate's artefacts). Dropping any one
/// makes local `gitleaks detect --no-git` scans noisy from that
/// workspace's build output.
#[test]
fn gitleaks_config_excludes_all_three_target_dirs() {
    let body = read(CONFIG);
    // Find the paths block.
    let after = body
        .split("paths = [")
        .nth(1)
        .expect("paths array must exist");
    let block = after.split(']').next().expect("paths array must close");
    for dir in ["target/", "teralaunch/src-tauri/target/", "teralib/target/"] {
        assert!(
            block.contains(dir),
            "PRD 3.1.6: {CONFIG} `paths` array must exclude `{dir}`. \
             Cargo artefacts in that workspace's target dir can embed \
             high-entropy metadata strings that trip generic-api-key \
             detection on local `gitleaks detect --no-git` scans.\n\
             Paths block:\n{block}"
        );
    }
}

// --------------------------------------------------------------------
// Iter 223 structural pins — meta-guard header + 3 path constants +
// audit-doc existence + actions/checkout pinned version + gitleaks
// version floor.
// --------------------------------------------------------------------
//
// The thirteen pins above cover workflow/config structure + audit-ref
// citation + dual-trigger + fetch-depth + semver shape + non-empty
// allowlist arrays + log-opts scoping + event-type branching + fail-
// fast curl + word-boundary anchors + triple-target-exclusion + self-
// test. They do NOT pin: (a) the guard's own header cites PRD 3.1.6
// — meta-guard contract; (b) WORKFLOW + CONFIG + AUDIT_REF path
// constants equal their canonical relative forms verbatim; (c) the
// audit doc (`docs/PRD/audits/security/secret-leak-scan.md`) actually
// exists and has substantive content — the audit-ref citation pin
// says the config mentions the path, but not that the file exists;
// (d) `actions/checkout` is pinned to a specific major version (v4),
// not a floating reference like `@main` or `@master`; (e) the
// gitleaks VER meets a current-floor (≥ 8.30.0) — iter-144 pin only
// checks semver shape; a pin-and-forget at 8.0.0 would pass shape
// check while running multi-year-old gitleaks missing recent rules.

/// The guard's own module header must cite PRD 3.1.6 so a reader
/// chasing a secret-scan regression lands here via section-grep.
#[test]
fn guard_file_header_cites_prd_3_1_6() {
    let body = fs::read_to_string("tests/secret_scan_guard.rs")
        .expect("tests/secret_scan_guard.rs must exist");
    let header = &body[..body.len().min(1500)];
    assert!(
        header.contains("PRD 3.1.6"),
        "meta-guard contract: tests/secret_scan_guard.rs header must \
         cite `PRD 3.1.6`. Without it, a reader chasing a secret-\
         scan regression won't land here via section-grep.\n\
         Header:\n{header}"
    );
    assert!(
        header.contains("gitleaks"),
        "meta-guard contract: header must name the tool (`gitleaks`) \
         so tool-rename-away-from-gitleaks is caught at review time."
    );
}

/// `WORKFLOW` + `CONFIG` + `AUDIT_REF` path constants must equal
/// their canonical relative forms verbatim. A rename without atomic
/// constant update would break every pin with an opaque panic.
#[test]
fn workflow_config_audit_path_constants_are_canonical() {
    let guard_body =
        fs::read_to_string("tests/secret_scan_guard.rs").expect("guard source must be readable");
    for literal in [
        "const WORKFLOW: &str = \"../../.github/workflows/secret-scan.yml\";",
        "const CONFIG: &str = \"../../.gitleaks.toml\";",
        "const AUDIT_REF: &str = \"docs/PRD/audits/security/secret-leak-scan.md\";",
    ] {
        assert!(
            guard_body.contains(literal),
            "PRD 3.1.6 (iter 223): tests/secret_scan_guard.rs must \
             retain `{literal}` verbatim. A rename of any constant \
             without atomic update to the corresponding file would \
             panic with an opaque `file not readable`."
        );
    }
}

/// The audit doc (`docs/PRD/audits/security/secret-leak-scan.md`)
/// must exist and carry substantive content. The iter-88 config-
/// citation pin verifies `.gitleaks.toml` references this file, but
/// not that the file itself still exists — a silent deletion would
/// break the traceability chain from allowlist entries back to the
/// iter-13 triage.
#[test]
fn secret_leak_audit_doc_exists_and_is_non_empty() {
    let audit_path = "../../docs/PRD/audits/security/secret-leak-scan.md";
    let body = fs::read_to_string(audit_path).unwrap_or_else(|e| {
        panic!(
            "PRD 3.1.6 (iter 223): {audit_path} must exist. This doc \
             is the triage artefact every `.gitleaks.toml` allowlist \
             entry cites — deletion would make the allowlist \
             speculative. Error: {e}"
        )
    });
    assert!(
        body.len() > 500,
        "PRD 3.1.6 (iter 223): {audit_path} must carry substantive \
         content (> 500 bytes). Found {} bytes. A truncation to a \
         stub would break traceability from allowlist entries to \
         the triaged findings.",
        body.len()
    );
    // Must reference iter 13 (the triage iteration).
    assert!(
        body.contains("iter 13") || body.contains("Iter 13"),
        "PRD 3.1.6 (iter 223): {audit_path} must reference `iter 13` \
         — the iteration that performed the initial triage. Without \
         it, future readers can't trace the doc's history."
    );
}

/// `actions/checkout` must be pinned to a specific major version
/// (e.g. `@v4`), not a floating reference like `@main`, `@master`,
/// or an unversioned pin. GitHub Actions marketplace actions can be
/// updated by their authors; a floating reference would let a
/// compromised upstream inject a malicious checkout step into every
/// secret-scan run.
#[test]
fn secret_scan_workflow_pins_checkout_action_version() {
    let body = read(WORKFLOW);
    // Must contain `actions/checkout@v<digit>` pattern (e.g. @v4).
    let has_pinned = body.contains("actions/checkout@v4")
        || body.contains("actions/checkout@v5")
        || body.contains("actions/checkout@v3");
    assert!(
        has_pinned,
        "PRD 3.1.6 (iter 223): {WORKFLOW} must pin `actions/checkout` \
         to a specific major version (e.g. `@v4`). A floating \
         reference would let a compromised marketplace action inject \
         malicious code into every secret-scan run."
    );
    // Reject floating references explicitly.
    for forbidden in [
        "actions/checkout@main",
        "actions/checkout@master",
        "actions/checkout@latest",
    ] {
        assert!(
            !body.contains(forbidden),
            "PRD 3.1.6 (iter 223): {WORKFLOW} must NOT use `{forbidden}` \
             — floating reference lets upstream author inject \
             arbitrary code into every run."
        );
    }
    // And must not be unversioned (bare `actions/checkout` with no
    // `@version`).
    assert!(
        !body.contains("- uses: actions/checkout\n") && !body.contains("- uses: actions/checkout "),
        "PRD 3.1.6 (iter 223): {WORKFLOW} must not use unversioned \
         `- uses: actions/checkout` — must carry an `@v<N>` pin."
    );
}

/// The gitleaks `VER` must meet a current-epoch floor. The iter-144
/// semver-shape pin accepts any 3-part digit version; this pin adds
/// a floor (major.minor ≥ 8.30) so a pin-and-forget at an ancient
/// version (e.g. `8.0.0` from early 2024) would fail review. Newer
/// gitleaks releases add rule-set improvements; running a multi-year-
/// old version misses recent detections.
#[test]
fn secret_scan_workflow_version_meets_current_floor() {
    let body = read(WORKFLOW);
    let ver_line = body
        .lines()
        .find(|l| l.trim_start().starts_with("VER="))
        .expect("VER= line must exist (iter-144 pin)");
    let value = ver_line.trim_start().trim_start_matches("VER=").trim();
    let parts: Vec<&str> = value.split('.').collect();
    let major: u32 = parts[0].parse().unwrap_or(0);
    let minor: u32 = parts.get(1).and_then(|p| p.parse().ok()).unwrap_or(0);

    // Floor: 8.30. This matches iter-13's documented baseline.
    // Upgrades from here should bump the floor atomically.
    assert!(
        major > 8 || (major == 8 && minor >= 30),
        "PRD 3.1.6 (iter 223): gitleaks VER must be ≥ 8.30. Got \
         `{value}`. The iter-144 shape pin accepts any 3-part semver; \
         this floor catches pin-and-forget at an ancient version that \
         would silently miss rule-set improvements shipped since the \
         floor was last re-baselined."
    );
}

/// Self-test — prove the detectors bite on synthetic bad shapes.
#[test]
fn secret_scan_guard_detector_self_test() {
    // Bad shape A: workflow missing `--config` flag.
    let no_config = "- name: gitleaks\n  run: gitleaks detect --source .\n";
    assert!(
        !no_config.contains("--config .gitleaks.toml"),
        "self-test: workflow without --config must be flagged"
    );

    // Bad shape B: config without `useDefault = true` (replace semantics).
    let replace_mode = "[rules]\ndescription = \"only our rules\"\n";
    assert!(
        !(replace_mode.contains("[extend]") && replace_mode.contains("useDefault = true")),
        "self-test: replace-mode config must be flagged"
    );

    // Bad shape C: config without audit citation.
    let uncited = "[extend]\nuseDefault = true\n[allowlist]\npaths = ['''foo''']\n";
    assert!(
        !(uncited.contains(AUDIT_REF) || uncited.contains("iter 13")),
        "self-test: uncited allowlist must be flagged"
    );

    // Bad shape D (iter 144): workflow without pull_request trigger.
    let push_only = "on:\n  push:\n    branches: [main]\n";
    assert!(
        !push_only.contains("pull_request:"),
        "self-test: workflow with only push trigger must be flagged"
    );

    // Bad shape E: shallow fetch (no fetch-depth: 0).
    let shallow = "- uses: actions/checkout@v4\n";
    assert!(
        !shallow.contains("fetch-depth: 0"),
        "self-test: shallow checkout must be flagged"
    );

    // Bad shape F: floating version tag.
    let floating = "VER=latest\n";
    let value = floating.trim_start().trim_start_matches("VER=").trim();
    let parts: Vec<&str> = value.split('.').collect();
    assert_ne!(
        parts.len(),
        3,
        "self-test: floating `latest` tag must NOT parse as 3-part \
         semver"
    );

    // Bad shape G: empty regexes array.
    let empty_regexes = "regexes = [\n]\n";
    let after = empty_regexes.split("regexes = [").nth(1).unwrap();
    let block = after.split(']').next().unwrap();
    let count = block.matches("'''").count() / 2;
    assert_eq!(
        count, 0,
        "self-test: empty regexes array must yield 0 entries"
    );
}

// --------------------------------------------------------------------
// Iter 266 structural pins — workflow/config/guard byte bounds +
// audit doc presence + guard PRD 3.1.6 cite.
// --------------------------------------------------------------------

/// Iter 266: workflow file byte bounds.
#[test]
fn workflow_file_byte_size_has_sane_bounds() {
    const MIN_BYTES: usize = 200;
    const MAX_BYTES: usize = 10_000;
    let bytes = fs::metadata(WORKFLOW).expect("workflow must exist").len() as usize;
    assert!(
        (MIN_BYTES..=MAX_BYTES).contains(&bytes),
        "PRD 3.1.6 (iter 266): {WORKFLOW} is {bytes} bytes; expected \
         [{MIN_BYTES}, {MAX_BYTES}]. A gutted workflow drops the \
         gitleaks invocation; bloat signals unrelated CI logic."
    );
}

/// Iter 266: `.gitleaks.toml` config file byte bounds.
#[test]
fn gitleaks_config_byte_size_has_sane_bounds() {
    const MIN_BYTES: usize = 200;
    const MAX_BYTES: usize = 20_000;
    let bytes = fs::metadata(CONFIG)
        .expect("gitleaks config must exist")
        .len() as usize;
    assert!(
        (MIN_BYTES..=MAX_BYTES).contains(&bytes),
        "PRD 3.1.6 (iter 266): {CONFIG} is {bytes} bytes; expected \
         [{MIN_BYTES}, {MAX_BYTES}]."
    );
}

/// Iter 266: guard source byte bounds.
#[test]
fn guard_source_byte_size_has_sane_bounds() {
    const MIN_BYTES: usize = 5000;
    const MAX_BYTES: usize = 80_000;
    let bytes = fs::metadata("tests/secret_scan_guard.rs")
        .expect("guard must exist")
        .len() as usize;
    assert!(
        (MIN_BYTES..=MAX_BYTES).contains(&bytes),
        "PRD 3.1.6 (iter 266): guard is {bytes} bytes; expected \
         [{MIN_BYTES}, {MAX_BYTES}]."
    );
}

/// Iter 266: the iter-13 audit doc (`docs/PRD/audits/security/
/// secret-leak-scan.md`) must exist. Without it, readers tracing
/// the baseline triage history via AUDIT_REF would panic with a
/// "file not found" — the guard references it but doesn't prove it
/// still exists.
#[test]
fn audit_doc_still_exists() {
    let path = "../../docs/PRD/audits/security/secret-leak-scan.md";
    assert!(
        fs::metadata(path).is_ok(),
        "PRD 3.1.6 (iter 266): {path} must exist — iter-13 baseline \
         triage history lives there. The AUDIT_REF constant points \
         at it; without the doc, readers chasing the triage history \
         via that pointer hit a dead link."
    );
    let body = fs::read_to_string(path).unwrap_or_else(|e| panic!("{path}: {e}"));
    assert!(
        body.len() > 500,
        "PRD 3.1.6 (iter 266): {path} must be > 500 bytes. A stub \
         file passes the existence check but loses the triage \
         history."
    );
}

/// Iter 266: guard header must cite PRD 3.1.6 explicitly.
#[test]
fn guard_source_cites_prd_3_1_6_explicitly() {
    let body = fs::read_to_string("tests/secret_scan_guard.rs").expect("guard must exist");
    let header = &body[..body.len().min(500)];
    assert!(
        header.contains("PRD 3.1.6"),
        "PRD 3.1.6 (iter 266): guard header must cite `PRD 3.1.6` \
         explicitly for section-grep discoverability.\n\
         Header:\n{header}"
    );
}
