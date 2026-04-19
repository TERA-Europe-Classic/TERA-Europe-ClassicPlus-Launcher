# Lessons Learned

Retrospective artefact for the mod-manager perfection loop. Captures patterns
worth remembering: surprises, cost drivers, anti-patterns, and validated
approaches. Capped at 200 lines — older entries migrate to
`lessons-learned.archive.md` at each retrospective.

Format: each entry is a short H3 with Date + iter, a Pattern paragraph, and
When-to-apply.

Ordering: newest at top.

---

### 2026-04-19 / iter 60 — revalidation catches what commits skip

**Pattern.** `check-troubleshoot-coverage.mjs` silently regressed at
iter 49 (tolerant catalog parse added two new `Failed to read catalog
body:` / `Catalog JSON envelope is malformed:` strings that weren't
mirrored in `TROUBLESHOOT.md`). The CI gate isn't run on every commit
— only in revalidation sweeps. Would've drifted further without the
every-20 revalidation protocol.

**When to apply.** Trust the revalidation cadence — don't over-run
every gate on every commit (too expensive, hurts the cache). But when
a commit adds new user-facing error strings, grep-think before
pushing: is there a coverage gate that I should manually fire? In
doubt, run `check-troubleshoot-coverage.mjs` + the other grep-based
scanners in the pre-commit loop.

### 2026-04-19 / iter 59 — catch flawed plans at the moment of execution

**Pattern.** The wake prompt I wrote for iter 59 said "begin Tauri v2
migration M1: port frontend JS imports only." At the moment of
starting the work I realised this would transit main through a broken
runtime state — v2 JS speaks a different invoke protocol from v1
Rust. Pivoted to a safer P1 (progress-10hz test) + opened a proper
`tauri-v2-migration-plan` follow-up requiring `sdd:brainstorm` +
`sdd:plan` on a dedicated worktree.

**When to apply.** Before committing to the first file of a
multi-commit plan, do a 30-second preflight: does each intermediate
state leave the system shippable? If any milestone's commit would
break main, the plan needs reordering or a branch-based execution.
Listen to the "this is going to break" pre-flight voice — it's
almost always right.

### 2026-04-19 / iter 57 — pause loop, revert uncommitted edits, then engage

**Pattern.** User interrupted the loop mid-edit to ask a conversation
question ("talk to me about the blockers"). I had uncommitted work in
`external_app.rs` for the progress-rate test. Reverted it cleanly via
`git checkout --` before engaging the conversation. Two iterations
later (iter 59), I redid the same work fresh — no merge complexity,
no half-done state hanging in context.

**When to apply.** When the user context-switches the loop into an
interactive question, check `git status` first. If there's
uncommitted work: revert it (unless it's substantial — then commit a
WIP tag first). Engaging on a fresh tree keeps the conversation
clean and the next loop restart tractable. Mid-edit context is just
noise.

### 2026-04-19 / iter 48 — allowlist-backed CI gates over strict gates

**Pattern.** `i18n-no-hardcoded.test.js` had 10 pre-existing
hardcoded-English leaks in mods.js + mods.html. Strict-zero would've
meant a 10-file i18n refactor + new translation keys in 4 locales —
weeks of work holding up the iteration. Instead: ship the scanner
with a documented ALLOWLIST of current-state leaks, plus a test that
fails if anyone tries to add a NEW leak (regression protection) and
another that fails if an allowlist entry no longer appears in source
(forces deletion of stale rows). P1 follow-up tracks burn-down.

**When to apply.** When a new invariant has a non-trivial existing
backlog, prefer "lock the diff + document the debt" over
"lock-to-zero + block ship." The allowlist becomes the punch list.
Strict-zero is correct for NEW code but prevents shipping CI gates
in active repos with legacy state.

### 2026-04-19 / iter 45 — source-inspection guards for behaviours the type system can't pin

**Pattern.** `toggle_command_bodies_do_not_spawn_or_kill` uses
`include_str!("mods.rs")` to grep the `pub async fn enable_mod` /
`disable_mod` bodies for `spawn_app` / `stop_process_by_name`. Fails
if anyone wires a process op into either toggle command. This is a
source-level invariant — pure-function extraction handles the
common case (helper signature structurally forbids spawn), but the
Tauri-command body wrappers could still drift. Source grep catches
that class of regression without needing runtime.

**When to apply.** When an invariant reads as "this function must
not CALL these other functions," and the surface is a
`#[cfg(not(tarpaulin_include))]` Tauri command body that can't be
unit-tested, author a sibling test that `include_str!`s the file +
greps the function body. Cheap, catches drift, doubles as
documentation.

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

*(Entries from iters 3, 13–16, 20, 22, and the meta loop-cadence note
archived to `lessons-learned.archive.md` at the iter 60 retrospective
to stay under the 200-line cap. All still valid as reference.)*

---

## Archival policy

When this file exceeds 200 lines at the next retrospective iteration,
prepend a dated banner to `lessons-learned.archive.md` and move the
oldest N lines. Never delete — archived entries are searchable for
future retrospectives.
