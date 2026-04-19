# Lessons Learned — Archive

Older entries migrated out of `lessons-learned.md` when it exceeded the
200-line cap at retrospective iterations. Prepended in reverse
chronological order (newest archive banner at top).

---

## Archived 2026-04-19 / iter 60 retrospective

Entries below migrated from `lessons-learned.md` to keep the active
file under its 200-line cap. All still valid as historical record;
search both files when mining patterns for a new iteration.

### 2026-04-19 / iter 22 — CI scanner with self-tests

**Pattern.** `tests/deploy_scope.spec.js` and `tests/http_allowlist.rs`
both ship self-tests that exercise both sides (positive samples must pass,
negative samples must fail). A silently-broken scanner that accepts
everything is worse than no scanner — it rubber-stamps a bad diff and
gives false confidence. Self-tests catch drift in the scanner itself.

**When to apply.** Any grep-based or regex-based CI gate: embed positive
AND negative test patterns that run on every invocation. If the scanner
stops finding the negative patterns, CI should turn red before the user's
actual code is even scanned.

### 2026-04-19 / iter 20 — transient flakes, not regressions

**Pattern.** One launcher test reported FAILED (697/698) on the iter-20
revalidation, immediately followed by 5 clean runs on a re-run. First
instinct was "iter-19 SHA test flaked on loopback port". Evidence: 5/5
clean re-runs of the iter-19 tests in isolation. Conclusion: pre-existing
unrelated flake, not a regression caused by iter 19's work.

**When to apply.** A single-run failure is not evidence of regression if
the same test passes deterministically on N>3 reruns. Log + continue.
But always flag-hunt first: a silent habit of "eh, flake, re-run" is
how real regressions sneak in.

### 2026-04-19 / iter 13-16 — secret-scan "CI for future, not history"

**Pattern.** gitleaks across 5 repos surfaced 33 findings (1 real, 4
DPAPI, 28 false positives). Rewriting git history of 5 repos to remove
triaged-safe historical hits has zero security upside and breaks every
downstream clone. Instead: CI workflow that only scans *new* commits
(`pull_request.base..head` or `github.event.before..sha`). Fails on
any new secret but doesn't re-trip on the historical baseline every
run.

**When to apply.** For any repo-spanning scanner (secrets, licenses,
policy checks) with non-zero historical findings: default to
commit-range scoping, not full-history. Document the triage of the
historical baseline in an audit file so future maintainers can see
what was allowlisted and why.

### 2026-04-19 / iter 3 — cargo#6313 workaround: drop unused crate-type

**Pattern.** `teralib` was declared as `crate-type = ["cdylib", "rlib"]`
but nothing actually dynamically linked against it. Under
`cargo test --release`, this forces a double-build with conflicting LTO
on path-deps (cargo#6313, still open). Fix: drop `cdylib`.

**When to apply.** Any Rust workspace member whose `Cargo.toml` declares
`crate-type = ["cdylib", "rlib"]`: audit who actually consumes the
cdylib. If nobody, drop it. Saves compile time and sidesteps the
open cargo bug.

### 2026-04-19 / meta — loop cadence lessons

**Pattern.** Stale `/loop` prompts keep firing with old iter numbers
(iter 12/13/14 prompts long after counter=30). Always re-orient from
the machine-readable header in `fix-plan.md` — trust the file, not the
prompt. The prompt is advisory.

**When to apply.** Every `/loop` wake, first Read the fix-plan header.
If prompt's iteration number ≠ header counter + 1, the prompt is stale;
compute `N = counter + 1` fresh and proceed from there.
