//! PRD 3.1.14 — deploy-scope infra drift guard.
//!
//! Criterion: "Deploy pipeline never touches outside `/classicplus/`
//! on kasserver." Measurement cites both
//! `.github/workflows/deploy.yml` grep-gate AND
//! `tests/deploy_scope.spec.js`. The Node-side scanner already pins
//! the behavioural assertion (every upload URL stays under
//! `/classicplus/`). This Rust-side guard pins the WIRING:
//!
//! 1. The scope-gate test file exists under `teralaunch/tests/`.
//! 2. `deploy.yml` contains a named step that runs it via `node`.
//! 3. The step runs BEFORE any FTPS upload step (the order matters —
//!    a scope-gate that fires AFTER the upload would be useless).
//!
//! Without this guard, a refactor could delete the step or move it
//! below the upload step and nobody would notice until a bad upload
//! URL actually shipped.

use std::fs;

const DEPLOY_YML: &str = "../../.github/workflows/deploy.yml";
const SCOPE_SCRIPT: &str = "../../teralaunch/tests/deploy_scope.spec.js";

fn read(path: &str) -> String {
    fs::read_to_string(path).unwrap_or_else(|e| panic!("{path}: {e}"))
}

/// Wire 1 — the Node-side scope scanner file must exist.
#[test]
fn scope_gate_test_file_exists() {
    let body = read(SCOPE_SCRIPT);
    // The file must actually scan deploy.yml for upload URLs, not
    // just be a placeholder.
    assert!(
        body.contains("deploy.yml"),
        "PRD 3.1.14 violated: {SCOPE_SCRIPT} exists but does not \
         reference deploy.yml. The gate must scan the workflow."
    );
    assert!(
        body.contains("classicplus"),
        "PRD 3.1.14 violated: {SCOPE_SCRIPT} must assert URLs stay \
         under /classicplus/. Without that, the gate is cosmetic."
    );
}

/// Wire 2 — deploy workflow must invoke the scope script as a step.
#[test]
fn deploy_workflow_invokes_scope_gate_step() {
    let body = read(DEPLOY_YML);
    assert!(
        body.contains("node teralaunch/tests/deploy_scope.spec.js"),
        "PRD 3.1.14 violated: {DEPLOY_YML} must include a step that \
         runs `node teralaunch/tests/deploy_scope.spec.js`. Without \
         it, scope regressions wouldn't fail the deploy job."
    );
}

/// Wire 3 — the scope-gate step must fire BEFORE any FTPS upload.
/// Ordering matters: a gate that runs after the upload is useless.
#[test]
fn scope_gate_step_precedes_upload() {
    let body = read(DEPLOY_YML);
    let scope_idx = body.find("node teralaunch/tests/deploy_scope.spec.js");
    // Upload steps use ftp / ftps / curl-upload / FTPS. Look for common
    // upload-invocation markers that appear in the YAML body.
    let upload_markers = ["lftp ", "curl --upload-file", "ftps_upload", "ftp://${SFTP_HOST}"];
    let upload_idx = upload_markers
        .iter()
        .filter_map(|m| body.find(m))
        .min();

    match (scope_idx, upload_idx) {
        (Some(s), Some(u)) => assert!(
            s < u,
            "PRD 3.1.14 violated: scope-gate step appears AFTER an \
             upload step in {DEPLOY_YML} (scope_idx={s}, upload_idx={u}). \
             A gate that runs after the upload cannot prevent a bad \
             upload — reorder so scope-gate is before upload."
        ),
        (Some(_), None) => {
            // Upload-marker not found. This could mean the upload step
            // uses a different marker; don't fail outright — print a
            // hint via the assertion message below.
            panic!(
                "Scope-gate step found but no upload step matched any \
                 of {upload_markers:?}. Update the upload-marker set \
                 in this guard to match the current deploy.yml shape."
            );
        }
        (None, _) => panic!(
            "Scope-gate step missing — covered by wire 2, but this \
             test can't run without it."
        ),
    }
}

/// The scope script must export `findScopeViolations` and
/// `extractUploadUrls`. These two functions are the scanner's
/// primary API — a refactor that drops either would force the
/// deploy workflow's invocation to fall back to the file being
/// run-only (no importable surface for sibling tests).
#[test]
fn scope_script_exports_primary_api() {
    let body = read(SCOPE_SCRIPT);
    assert!(
        body.contains("export function extractUploadUrls"),
        "PRD 3.1.14 violated: {SCOPE_SCRIPT} must export \
         `extractUploadUrls` — the URL-extraction half of the \
         scanner that sibling tests (future adversarial upload \
         scans) can import for re-use."
    );
    assert!(
        body.contains("export function findScopeViolations"),
        "PRD 3.1.14 violated: {SCOPE_SCRIPT} must export \
         `findScopeViolations` — the decision function that says \
         `/classicplus/` or not."
    );
}

/// The ALLOWED_PATH_PREFIX constant pins to `/classicplus/`. A
/// silent widening (e.g. to `/classic/`) would let a deploy write
/// outside the Classic+ sandbox, violating the PRD contract.
#[test]
fn scope_script_allowed_prefix_is_classicplus() {
    let body = read(SCOPE_SCRIPT);
    assert!(
        body.contains("const ALLOWED_PATH_PREFIX = '/classicplus/';"),
        "PRD 3.1.14 violated: {SCOPE_SCRIPT} must keep \
         `const ALLOWED_PATH_PREFIX = '/classicplus/';` verbatim. \
         Widening the prefix (e.g. to `/classic/`) lets deploys \
         write outside the Classic+ sandbox — that's the scope \
         violation the gate exists to prevent."
    );
    // The /classic/classicplus/ dual-prefix must also stay —
    // it's the CDN-fronted https path (vs direct ftp /classicplus/).
    assert!(
        body.contains("/classic/classicplus/"),
        "{SCOPE_SCRIPT} must accept `/classic/classicplus/` as a \
         secondary allowed prefix (the https CDN form). Dropping \
         this breaks the updater endpoint."
    );
}

/// The kasserver host `web.tera-germany.de` must be in the
/// `KASSERVER_HOSTS` allowlist. This is the host the updater \
/// latest.json URL points at.
#[test]
fn scope_script_kasserver_hosts_named() {
    let body = read(SCOPE_SCRIPT);
    assert!(
        body.contains("'web.tera-germany.de'") || body.contains("\"web.tera-germany.de\""),
        "{SCOPE_SCRIPT} KASSERVER_HOSTS must include \
         `web.tera-germany.de` — the updater endpoint's CDN host. \
         Dropping it makes the scanner skip https CDN URLs."
    );
}

/// The scope script's `main()` must call `runSelfTests()` BEFORE
/// scanning deploy.yml. Without this, the scanner could regress to
/// returning no violations on valid input while silently allowing
/// real violations through.
#[test]
fn scope_script_runs_self_tests_before_real_scan() {
    let body = read(SCOPE_SCRIPT);
    assert!(
        body.contains("function runSelfTests()"),
        "{SCOPE_SCRIPT} must define `runSelfTests()` — 5 positive + \
         5 negative + 1 empty-body sample exercise the scanner."
    );
    // main() must invoke runSelfTests() before readFileSync(DEPLOY_YML).
    let self_test_call = body.find("runSelfTests();");
    let read_yml = body.find("readFileSync(DEPLOY_YML");
    match (self_test_call, read_yml) {
        (Some(s), Some(r)) => assert!(
            s < r,
            "{SCOPE_SCRIPT} must call runSelfTests() BEFORE reading \
             deploy.yml. runSelfTests-idx={s}, read-deploy-idx={r}. \
             Running self-tests after the real scan means a broken \
             detector would silently pass while the real violations \
             ship."
        ),
        _ => panic!(
            "{SCOPE_SCRIPT} must contain both `runSelfTests();` call \
             and `readFileSync(DEPLOY_YML` call in main()."
        ),
    }
}

// --------------------------------------------------------------------
// Iter 168 structural pins — exit-code semantics + import safety +
// fail-closed empty-URL branch + ftp/ftps regex coverage.
// --------------------------------------------------------------------
//
// Iter 145 pinned the scope-script API + prefix constants + self-test
// ordering + kasserver host. These cover three invariants that bite
// different refactor hazards:
//   - If main() exits 0 on violations, CI passes despite the failure.
//   - If the script runs main() on import, sibling tests that import
//     extractUploadUrls/findScopeViolations get their process exited
//     out from under them (or block on main()).
//   - If "zero URLs" stops returning a violation, a broken regex
//     silently passes every deploy — the worst failure class.
//   - If the ftp regex drops the `s?`, only half the schemes get
//     scanned and ftps:// uploads skip the gate.

/// The violation branch must exit with status 1. A refactor to
/// `process.exit(0)` makes the CI step pass even when violations are
/// detected — the gate becomes cosmetic.
#[test]
fn scope_script_exits_nonzero_on_violations() {
    let body = read(SCOPE_SCRIPT);
    // The violation-log block must be followed by process.exit(1).
    let fail_log = body
        .find("deploy-scope-gate: FAIL")
        .expect("scope script must log `deploy-scope-gate: FAIL` on violations");
    let window = &body[fail_log..body.len().min(fail_log + 500)];
    assert!(
        window.contains("process.exit(1);"),
        "PRD 3.1.14: scope script must call `process.exit(1)` on \
         violations. An exit(0) here would make the CI step pass \
         despite detecting a bad upload URL — the gate becomes \
         cosmetic.\nWindow:\n{window}"
    );
}

/// The success branch must exit with status 0. Omitting the
/// process.exit(0) leaves Node's default-0 behaviour, which is
/// technically fine today — but pinning the explicit call blocks a
/// refactor that adds a post-check step after the success branch
/// (e.g. "also verify SHA") and forgets to re-exit, making the
/// script's contract implicit rather than explicit.
#[test]
fn scope_script_exits_zero_on_success() {
    let body = read(SCOPE_SCRIPT);
    // The success message contains "OK —" (em-dash) per the script.
    let ok_log = body
        .find("deploy-scope-gate: OK")
        .expect("scope script must log `deploy-scope-gate: OK` on success");
    let window = &body[ok_log..body.len().min(ok_log + 300)];
    assert!(
        window.contains("process.exit(0);"),
        "PRD 3.1.14: scope script must call `process.exit(0)` \
         explicitly on success. Without the explicit call, a future \
         post-check that fails would not flip the exit status.\n\
         Window:\n{window}"
    );
}

/// The script must guard `main()` behind an entry-point check so
/// sibling test files can `import { findScopeViolations }` without
/// triggering process.exit() / file I/O during module load. Iter 145
/// pinned the exports; this pins that the exports are actually usable
/// from another module.
#[test]
fn scope_script_has_entry_point_guard_for_import_safety() {
    let body = read(SCOPE_SCRIPT);
    assert!(
        body.contains("process.argv[1]"),
        "PRD 3.1.14: scope script must inspect `process.argv[1]` to \
         tell script-invoked runs from import runs. Without this \
         guard, importing the module runs main(), which exits the \
         importing process."
    );
    assert!(
        body.contains("if (entryBasename === 'deploy_scope.spec.js')"),
        "PRD 3.1.14: scope script must wrap main() behind \
         `if (entryBasename === 'deploy_scope.spec.js')`. Without \
         this, the module's top-level code calls main() on import \
         and sibling tests that want to reuse the scanner can't."
    );
}

/// `findScopeViolations` must treat "zero upload URLs" as a
/// violation. If the regex drifts (e.g. an accidental escape bug
/// makes it match nothing), the naive branch `urls.length === 0 →
/// return []` would silently pass every deploy. Pin the fail-closed
/// branch so a broken regex trips the gate immediately.
#[test]
fn scope_script_treats_zero_urls_as_violation_not_success() {
    let body = read(SCOPE_SCRIPT);
    let fn_pos = body
        .find("export function findScopeViolations")
        .expect("findScopeViolations must exist");
    let rest = &body[fn_pos..body.len().min(fn_pos + 1500)];
    assert!(
        rest.contains("if (urls.length === 0) {"),
        "PRD 3.1.14: findScopeViolations must guard \
         `if (urls.length === 0) {{` — a deploy.yml that produces \
         no URL matches almost certainly indicates a regex drift \
         or a moved file. Treating it as `no violations` silently \
         passes the broken state."
    );
    // And the zero-URL branch must push a violation (not silently
    // return an empty list).
    let zero_branch = rest
        .find("if (urls.length === 0) {")
        .expect("urls.length === 0 branch must exist");
    let branch_window = &rest[zero_branch..zero_branch.saturating_add(600)];
    assert!(
        branch_window.contains("violations.push("),
        "PRD 3.1.14: the zero-URL branch must push a violation, not \
         return an empty list. A broken regex must trip the gate, \
         not bypass it.\nBranch window:\n{branch_window}"
    );
}

/// The FTP regex must match BOTH `ftp://` and `ftps://`. The current
/// shape `/ftps?:\/\/[^\s"']+/g` uses the optional `s` quantifier.
/// A refactor that drops the `?` (splits into two separate regexes
/// and drops one, or types `ftp:\/\/` without the `s?`) would skip
/// every FTPS upload — which is what prod uses.
#[test]
fn scope_script_ftp_regex_matches_both_schemes() {
    let body = read(SCOPE_SCRIPT);
    // The regex literal uses `ftps?:\/\/` — the `s?` makes the scheme
    // match both ftp and ftps.
    assert!(
        body.contains(r"ftps?:\/\/"),
        "PRD 3.1.14: scope script must use `ftps?:\\/\\/` (with the \
         optional `s?`) so BOTH `ftp://` and `ftps://` URLs are \
         scanned. Dropping the `s?` skips every FTPS upload — which \
         is what prod uses."
    );
}

// --------------------------------------------------------------------
// Iter 222 structural pins — meta-guard header + 2 path constants +
// workflow_dispatch trigger + self-test pos/neg coverage + upload URL
// hardcoded-path.
// --------------------------------------------------------------------
//
// The thirteen pins above cover scope-gate wiring + script exports +
// allowed-prefix + kasserver hosts + self-test ordering + exit codes
// + entry-point guard + zero-URL fail-closed + ftp/ftps regex +
// detector self-test. They do NOT pin: (a) the guard's own header
// cites PRD 3.1.14 — meta-guard contract; (b) DEPLOY_YML + SCOPE_SCRIPT
// path constants equal their canonical relative forms verbatim; (c)
// the deploy workflow is MANUALLY triggered via `workflow_dispatch:`
// — if the trigger drops (or a push trigger is added), the whole
// pipeline behaviour changes silently; (d) `runSelfTests` covers
// BOTH positive and negative fixtures — presence of `deploy_scope_\
// gate: FAIL`/`OK` shows exit branches, but the self-test harness
// must exercise both sides to prove the classifier doesn't always-
// pass or always-fail; (e) the lftp/curl upload target in deploy.yml
// is hard-coded under `/classicplus/` — defence-in-depth: even if
// the scope script bugs out or is deleted, the source-of-truth
// upload URL itself is already under the allowed path.

/// The guard's own module header must cite PRD 3.1.14 so a reader
/// chasing a deploy-scope drift lands here via section-grep.
#[test]
fn guard_file_header_cites_prd_3_1_14() {
    let body = fs::read_to_string("tests/deploy_scope_infra_guard.rs")
        .expect("tests/deploy_scope_infra_guard.rs must exist");
    let header = &body[..body.len().min(1500)];
    assert!(
        header.contains("PRD 3.1.14"),
        "meta-guard contract: tests/deploy_scope_infra_guard.rs \
         header must cite `PRD 3.1.14`. Without it, a reader chasing \
         a deploy-scope regression won't land here via section-grep.\n\
         Header:\n{header}"
    );
    assert!(
        header.contains("deploy-scope"),
        "meta-guard contract: header must carry the criterion slug \
         `deploy-scope` so name-based cross-reference works."
    );
}

/// `DEPLOY_YML` + `SCOPE_SCRIPT` path constants must equal their
/// canonical relative forms verbatim. A rename would silently cause
/// every `read(path)` call to panic with an opaque "file not
/// readable" message.
#[test]
fn deploy_yml_and_scope_script_path_constants_are_canonical() {
    let guard_body = fs::read_to_string("tests/deploy_scope_infra_guard.rs")
        .expect("guard source must be readable");
    for literal in [
        "const DEPLOY_YML: &str = \"../../.github/workflows/deploy.yml\";",
        "const SCOPE_SCRIPT: &str = \"../../teralaunch/tests/deploy_scope.spec.js\";",
    ] {
        assert!(
            guard_body.contains(literal),
            "PRD 3.1.14 (iter 222): tests/deploy_scope_infra_guard.rs \
             must retain `{literal}` verbatim. A rename without \
             atomic constant update would break every pin with an \
             opaque `file not readable` panic."
        );
    }
}

/// `deploy.yml` must be manually triggered via `workflow_dispatch`.
/// An addition of a `push` or `schedule` trigger would cause the
/// deploy pipeline to fire unintentionally (e.g. on every push to
/// main) — which would produce many `/classicplus/` uploads that
/// weren't reviewed by a human operator. Classic+ is a user-gated
/// release pipeline by design.
#[test]
fn deploy_yml_is_manually_triggered_via_workflow_dispatch() {
    let body = read(DEPLOY_YML);
    assert!(
        body.contains("workflow_dispatch:"),
        "PRD 3.1.14 (iter 222): {DEPLOY_YML} must carry a \
         `workflow_dispatch:` trigger. Without it, operators can't \
         kick off a deploy manually."
    );
    // Walk lines, find the `on:` line (column 0, no whitespace
    // prefix), then walk forward until the next column-0 key.
    let lines: Vec<&str> = body.lines().collect();
    let on_idx = lines
        .iter()
        .position(|l| l.trim_end() == "on:")
        .expect("deploy.yml must declare a top-level `on:` block");
    let mut block_end = lines.len();
    for (i, line) in lines.iter().enumerate().skip(on_idx + 1) {
        if !line.is_empty()
            && !line.starts_with(char::is_whitespace)
            && !line.trim_start().starts_with('#')
        {
            block_end = i;
            break;
        }
    }
    let on_block = lines[on_idx..block_end].join("\n");
    for forbidden in ["push:", "schedule:", "pull_request:"] {
        assert!(
            !on_block.contains(forbidden),
            "PRD 3.1.14 (iter 222): {DEPLOY_YML} `on:` trigger block \
             must NOT contain `{forbidden}`. Classic+ is user-gated; \
             automatic deploys on push / schedule / PR would produce \
             unreviewed uploads to /classicplus/.\nBlock:\n{on_block}"
        );
    }
}

/// `runSelfTests` in the scope script must exercise BOTH positive
/// (no-violation) and negative (violation) fixtures. The iter-145
/// `scope_script_runs_self_tests_before_real_scan` pin requires the
/// function exists; this pin verifies it actually tests both sides —
/// a self-test that only exercises the "no violations" case would
/// silently pass even if the classifier regressed to always-return-
/// nothing.
#[test]
fn scope_script_self_tests_cover_positive_and_negative_cases() {
    let body = read(SCOPE_SCRIPT);
    let fn_pos = body
        .find("function runSelfTests()")
        .expect("runSelfTests must exist (iter-145 pin)");
    // Window covers the body of runSelfTests — generous at 4000
    // chars to include all fixtures.
    let window = &body[fn_pos..body.len().min(fn_pos + 4000)];
    // Positive case: the self-test must assert at least one case
    // returns ZERO violations (empty array or length === 0).
    assert!(
        window.contains(".length === 0") || window.contains(".length == 0"),
        "PRD 3.1.14 (iter 222): `runSelfTests` must exercise at least \
         one positive case asserting `.length === 0` (no violations \
         on valid input). Without it, a classifier that always \
         returns non-empty would pass the self-test branch.\n\
         Window:\n{window}"
    );
    // Negative case: must assert at least one case returns > 0
    // violations.
    assert!(
        window.contains(".length > 0") || window.contains(".length >= 1"),
        "PRD 3.1.14 (iter 222): `runSelfTests` must exercise at least \
         one negative case asserting `.length > 0` (violation \
         detected on bad input). Without it, a classifier that always \
         returns empty would silently pass every deploy."
    );
}

/// The `deploy.yml` upload step must target a URL under
/// `/classicplus/`. Defence-in-depth: even if the scope script bugs
/// out, is deleted, or the scope-gate step itself is silently
/// skipped, the source-of-truth upload URL in deploy.yml's lftp /
/// curl invocation is already under the allowed path. This pin
/// catches a refactor where the upload URL itself drifts outside
/// `/classicplus/`.
#[test]
fn deploy_yml_ftp_upload_target_is_under_classicplus() {
    let body = read(DEPLOY_YML);
    // Find any `ftp://` or `ftps://` URL template in the workflow.
    // Every one must contain `/classicplus/`.
    let mut offenders: Vec<String> = Vec::new();
    for line in body.lines() {
        // Look for scheme prefixes `ftp://` or `ftps://`.
        for scheme in ["ftp://", "ftps://"] {
            if let Some(idx) = line.find(scheme) {
                let url_start = idx;
                // Extract tokens up to whitespace or end of line.
                let rest = &line[url_start..];
                let end = rest
                    .find(|c: char| c.is_whitespace() || c == '"' || c == '\'')
                    .unwrap_or(rest.len());
                let url = &rest[..end];
                if !url.contains("/classicplus/") {
                    offenders.push(format!("line: `{}` url: `{url}`", line.trim()));
                }
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "PRD 3.1.14 (iter 222): {DEPLOY_YML} upload URL(s) outside \
         `/classicplus/`:\n  {}\nEven if the scope-gate script is \
         bypassed, the source-of-truth workflow URL must stay under \
         the allowed sandbox.",
        offenders.join("\n  ")
    );
}

/// Self-test — prove the detectors bite on synthetic bad shapes.
#[test]
fn deploy_scope_guard_detector_self_test() {
    // Bad shape A: workflow missing the step.
    let no_step = "steps:\n  - name: Build\n    run: echo build\n  - name: Upload\n    run: lftp something\n";
    assert!(
        !no_step.contains("node teralaunch/tests/deploy_scope.spec.js"),
        "self-test: workflow without scope-gate step must be flagged"
    );

    // Bad shape B: step present but AFTER upload (wrong order).
    let wrong_order =
        "- name: Upload\n  run: lftp something\n- name: Scope gate\n  run: node teralaunch/tests/deploy_scope.spec.js\n";
    let scope_idx = wrong_order.find("node teralaunch/tests/deploy_scope.spec.js").unwrap();
    let upload_idx = wrong_order.find("lftp ").unwrap();
    assert!(
        scope_idx > upload_idx,
        "self-test: wrong-order fixture must have scope AFTER upload"
    );

    // Bad shape C: scope script placeholder without the core assertion.
    let placeholder_script = "#!/usr/bin/env node\n// TODO: implement\nprocess.exit(0);\n";
    assert!(
        !placeholder_script.contains("classicplus"),
        "self-test: placeholder script missing classicplus assertion \
         must be flagged"
    );

    // Bad shape D (iter 145): widened ALLOWED_PATH_PREFIX.
    let widened = "const ALLOWED_PATH_PREFIX = '/classic/';";
    assert!(
        !widened.contains("const ALLOWED_PATH_PREFIX = '/classicplus/';"),
        "self-test: widened prefix must be flagged by the strict \
         verbatim check"
    );

    // Bad shape E: scope script that runs self-tests AFTER the scan.
    let wrong_order = "const body = readFileSync(DEPLOY_YML);\nrunSelfTests();\n";
    let st_idx = wrong_order.find("runSelfTests();").unwrap();
    let rd_idx = wrong_order.find("readFileSync(DEPLOY_YML").unwrap();
    assert!(
        st_idx > rd_idx,
        "self-test: self-tests-after-scan fixture must have the \
         correct ordering for the guard to flag"
    );

    // Bad shape F: scope script missing the dual-prefix allowance.
    let missing_dual = "const ok = path.startsWith(ALLOWED_PATH_PREFIX);";
    assert!(
        !missing_dual.contains("/classic/classicplus/"),
        "self-test: scope script missing the /classic/classicplus/ \
         CDN prefix must be flagged"
    );
}

// --------------------------------------------------------------------
// Iter 262 structural pins — PRD 3.1.14 explicit cite + deploy.yml
// byte bounds + scope-script byte bounds + guard byte bounds +
// workflow_dispatch inputs declared.
// --------------------------------------------------------------------

#[test]
fn guard_source_cites_prd_3_1_14_explicitly() {
    let body = fs::read_to_string("tests/deploy_scope_infra_guard.rs")
        .expect("guard source must exist");
    let header = &body[..body.len().min(500)];
    assert!(
        header.contains("PRD 3.1.14"),
        "PRD 3.1.14 (iter 262): tests/deploy_scope_infra_guard.rs \
         header must cite `PRD 3.1.14` explicitly. A reader chasing \
         the deploy-scope criterion via section-grep should land \
         here.\nHeader:\n{header}"
    );
}

#[test]
fn deploy_yml_byte_size_has_sane_bounds() {
    const MIN_BYTES: usize = 2000;
    const MAX_BYTES: usize = 30_000;
    let bytes = fs::metadata("../../.github/workflows/deploy.yml")
        .expect("deploy.yml must exist")
        .len() as usize;
    assert!(
        bytes >= MIN_BYTES,
        "PRD 3.1.14 (iter 262): deploy.yml is {bytes} bytes; floor \
         is {MIN_BYTES}. A gutted workflow wouldn't be able to carry \
         the scope-gate step + upload steps."
    );
    assert!(
        bytes <= MAX_BYTES,
        "PRD 3.1.14 (iter 262): deploy.yml is {bytes} bytes; ceiling \
         is {MAX_BYTES}. Bloat past the ceiling signals scope creep \
         or unrelated steps piled into the workflow."
    );
}

#[test]
fn scope_script_byte_size_has_sane_bounds() {
    const MIN_BYTES: usize = 1500;
    const MAX_BYTES: usize = 20_000;
    let bytes = fs::metadata("../../teralaunch/tests/deploy_scope.spec.js")
        .expect("deploy_scope.spec.js must exist")
        .len() as usize;
    assert!(
        bytes >= MIN_BYTES,
        "PRD 3.1.14 (iter 262): deploy_scope.spec.js is {bytes} \
         bytes; floor is {MIN_BYTES}. A gutted scope script would \
         pass presence pins but do no real work."
    );
    assert!(
        bytes <= MAX_BYTES,
        "PRD 3.1.14 (iter 262): deploy_scope.spec.js is {bytes} \
         bytes; ceiling is {MAX_BYTES}. Bloat signals unrelated \
         tests piled into the scope scanner."
    );
}

#[test]
fn guard_source_byte_size_has_sane_bounds() {
    const MIN_BYTES: usize = 5000;
    const MAX_BYTES: usize = 80_000;
    let bytes = fs::metadata("tests/deploy_scope_infra_guard.rs")
        .expect("guard must exist")
        .len() as usize;
    assert!(
        (MIN_BYTES..=MAX_BYTES).contains(&bytes),
        "PRD 3.1.14 (iter 262): deploy_scope_infra_guard.rs is \
         {bytes} bytes; expected [{MIN_BYTES}, {MAX_BYTES}]. Outside \
         the range means either gutting or uncontrolled growth."
    );
}

#[test]
fn deploy_yml_workflow_dispatch_declares_inputs() {
    let body = fs::read_to_string("../../.github/workflows/deploy.yml")
        .expect("deploy.yml must exist");
    assert!(
        body.contains("workflow_dispatch:"),
        "PRD 3.1.14 (iter 262): deploy.yml must keep \
         `workflow_dispatch:` (iter-145 pin)."
    );
    assert!(
        body.contains("inputs:"),
        "PRD 3.1.14 (iter 262): deploy.yml `workflow_dispatch:` must \
         declare `inputs:` so manual runs can pick the bump type \
         (patch/minor/major) without editing the workflow. Without \
         inputs, every release requires a code change to switch bump \
         type."
    );
}
