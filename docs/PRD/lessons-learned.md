# Lessons Learned

Retrospective artefact for the mod-manager perfection loop. Captures patterns
worth remembering: surprises, cost drivers, anti-patterns, and validated
approaches. Capped at 200 lines — older entries migrate to
`lessons-learned.archive.md` at each retrospective.

Format: each entry is a short H3 with Date + iter, a Pattern paragraph, and
When-to-apply.

Ordering: newest at top.

---

### 2026-04-19 / iter 30 — bin crate integration-test boundary

**Pattern.** `teralaunch/src-tauri` has no `[lib]` target, so integration
tests in `src-tauri/tests/*.rs` cannot import launcher-private items
(`Registry`, `SpawnDecision`, `IntegrityResult`, etc.). Two workable options:
(a) put the real behavioural test in the module's `#[cfg(test)]` block;
(b) add a mirror integration test in `tests/` that pins the *external
contract* (serde JSON shape, algorithmic invariants) without importing the
type.

We've standardised on both: option (a) owns the behaviour assertion,
option (b) catches silent drift from third-party crates (serde rename,
`zeroize` bump, `sha2` algorithm change). The mirror tests are small —
they don't duplicate behaviour, they pin shape.

**When to apply.** Any time the PRD prescribes a test path like
`tests/foo.rs::specific_name`, read it as a naming intent, not a filesystem
mandate: if the helper under test is crate-private, put the real assertion
in-module and use the PRD-named file as a symbolic integration-level pin.
Call out the deviation in the fix-plan DONE entry so reviewers see why.

### 2026-04-19 / iter 29 — extract pure predicate before testing multi-site invariant

**Pattern.** When the same rule lives at two call sites (iter 29:
`if !is_process_running { spawn }` duplicated in `launch_external_app_impl`
and `spawn_auto_launch_external_apps`), the two can silently diverge under
refactor. Factor the rule into a single pure predicate (`decide_spawn(bool)
-> SpawnDecision`) that both sites call, then test the predicate directly.
Both call sites are now one-liners that a reviewer can eyeball.

**When to apply.** Before writing a test for any "behaviour X must hold at
every call site Y", scan for duplicated conditionals. If they exist, extract
a pure function first. The test surface becomes trivial and you get
regression protection at the refactor level (renames of either call site
still route through the predicate).

### 2026-04-19 / iter 28 — recovery passes belong in load(), not in boot orchestration

**Pattern.** `Registry::recover_stuck_installs()` initially looked like it
should live in main.rs setup, right after `Registry::load()`. Moving it
*inside* `load()` makes recovery implicit on every startup — no chance of
"we forgot to call recover after loading". Idempotency guarantee carries
the test: calling recover twice is a no-op.

**When to apply.** When a recovery sweep is mandatory after every persist-
then-load cycle, bake it into `load`. Leaving it as an optional caller-step
invites bugs where a new `load()` site forgets to recover. Idempotency is
the price of admission.

### 2026-04-19 / iter 27 — fail-closed integrity checks need the prompt copy in-source

**Pattern.** `services::self_integrity::REINSTALL_PROMPT` is a `pub const
&str` — the user-facing message is in the source tree, not the localisation
files. Rationale: when the integrity check fails, the localisation infra
itself might be the thing that got tampered with. Keeping the prompt as a
Rust `const` means it's embedded in the signed binary and shown via native
`MessageBoxW` *before* Tauri initialises.

Also important: the prompt contains no raw hash values. Users can't
validate a hex digest; attackers could, via a phishing clone of our
support page. `reinstall_prompt_is_user_safe` asserts no "sha"/"SHA"
substring in the copy.

**When to apply.** Any pre-startup user-facing error copy (crash dialog,
integrity fail, bootstrap fail): hard-code in source; never include raw
cryptographic material in user-visible prompts.

### 2026-04-19 / iter 26 — Zeroize derive cost: partial moves forbidden

**Pattern.** Adding `#[derive(Zeroize, ZeroizeOnDrop)]` to a struct
implements `Drop`, which forbids moving fields out of the struct
individually. Existing code like `guard.auth_key = info.auth_key;`
breaks with `E0509 cannot move out of Drop type`. Fix: whole-struct
swap (`*guard = info;`) or clone (`guard.auth_key = info.auth_key.clone();`).

Also: `zeroize = "1.7"` needs the `"zeroize_derive"` feature explicitly
in `Cargo.toml` or the derive macros are unresolved.

**When to apply.** Before applying `ZeroizeOnDrop` to an in-tree struct,
grep for `x = struct.field;` patterns. Rewrite as whole-struct assignment
or explicit `.clone()`. Usually whole-struct is cleaner anyway.

### 2026-04-19 / iter 24 — real-vulnerability-in-audit pattern

**Pattern.** PRD items framed as "verify and implement if missing" (iter 24:
`3.1.4.gpk-deploy-sandbox`) routinely turn up real vulnerabilities. Here
`install_gpk` joined attacker-controlled `modfile.container` into a filesystem
path without sanitisation. The test the PRD asked for became the regression
test for the fix.

**When to apply.** For every PRD item with "verify (and implement if missing)"
language: read the actual code *before* starting to write the test. If the
sanitisation truly exists, the test pins it. If it doesn't, the fix comes
first. Either way the PRD item closes.

Side-benefit: the test file acts as the audit artefact — a reviewer can
read the vector list and see what threats we considered. More vectors than
the PRD asks for is cheap defence in depth (iter 24: 15 vectors vs PRD's
required 5).

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

---

## Archival policy

When this file exceeds 200 lines at the next retrospective iteration,
prepend a dated banner to `lessons-learned.archive.md` and move the
oldest N lines. Never delete — archived entries are searchable for
future retrospectives.
