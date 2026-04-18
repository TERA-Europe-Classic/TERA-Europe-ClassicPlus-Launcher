# Ralph Loop Instructions — Mod Manager Drive-to-Done

This document is the **script** the ralph loop runs against. Each
iteration reads the current state of `acceptance-criteria.md`, picks the
next open checkbox, and applies the CEK pipeline until that checkbox can
be flipped to `[x]`. The loop never stops on its own — termination is
defined by the gate at the bottom.

---

## Starting the loop

Invoke via the Claude Code `/loop` slash command, self-paced:

```
/loop
@.specs/prd/ralph-loop-instructions.md
```

The loop prompt re-reads the instructions each iteration, so updates to
this file take effect on the next wake.

## Per-iteration script

Execute these steps **in order** every iteration. Do not skip steps.
Skills are bound by `/using-cek` — consult that before implementation
work (cache the router output mentally).

### 1. Check the gate

Run the termination check at the bottom of this file first. If all
conditions are met, **stop and summarise** — the loop is done.

### 2. Pick the next target

Open `acceptance-criteria.md`. Find the first unchecked `[ ]` entry by
section (A → B → C → … → O, then X-series, in that order). That
criterion is the iteration's target.

If every `[ ]` is already `[x]`, skip to step 9 (final validation pass).

### 3. Look up the test

Open `test-plan.md`. Find the row whose criterion column matches the
target. The `Test` column names the automated test that proves the
criterion. The `Layer` column tells you where to write it:

- **Rust unit** → add `#[test]` in the matching module under
  `teralaunch/src-tauri/src/`.
- **Rust integration** → add a test file under
  `teralaunch/src-tauri/tests/` (create the directory if missing).
- **Vitest** → add to the appropriate file under `teralaunch/tests/`.
- **Playwright** → add to `teralaunch/tests/e2e/` using the filename
  suggested in the Edge-cases section.

### 4. Write the failing test first (TDD)

Invoke `tdd:test-driven-development`. Write the test. Run it. Confirm
it **fails** with a message that identifies what's missing. Never
proceed without seeing the failure.

### 5. Implement the smallest change that makes the test pass

Follow `kaizen:kaizen` + `ddd:software-architecture`. Minimum viable
change. No scope creep. If the fix requires touching > 2 files, split
the work through `sdd:add-task` → `sdd:plan` → `sdd:implement` inside
this iteration.

### 6. Re-run the full suite

From `teralaunch/`:

```bash
cd src-tauri && cargo clippy --all-targets --release -- -D warnings \
  && cargo test --release \
  && cd .. \
  && npm test \
  && npm run test:e2e
```

Any failure → fix the regression before touching anything else. Do not
advance with red tests.

### 7. Review

Run `code-review:review-local-changes`. Address every finding with
confidence ≥ 80 before proceeding. Confidence < 80 findings are logged
as comments on the criterion line for later triage — they do not block
the check.

### 8. Reflect + commit

- `reflexion:reflect` on the change before presenting.
- Flip the acceptance checkbox to `[x]` with the test name appended:
  `- [x] **A3** Filter chips narrow the list. *(catalog_fetch_and_render)*`
- `git:commit` with a conventional message; no emoji; one-line subject
  + short body citing the acceptance criterion id.

### 9. Final validation (only when every checkbox is `[x]`)

Before declaring the loop done, run the full validation once more on a
clean checkout:

```bash
git status                             # must be clean
cd teralaunch
npm ci                                 # reproducible install
cd src-tauri && cargo clean && cargo test --release
cd .. && npm run test:e2e -- --reporter=line
```

Then run `reflexion:critique` with the three judges (Requirements,
Architecture, Code Quality) against the cumulative diff since the last
release tag. Consensus must be "no further improvements warranted"
across all three judges. If any judge surfaces a real improvement,
append it as a new acceptance criterion and resume the loop.

Finally, tag a release via `gh workflow run deploy.yml -f bump=minor`
and verify the release lands (signed updater, GH release, `latest.json`
live on kasserver). Mark the PRD status as `shipped`.

## Never

- Never flip a checkbox without a green automated test.
- Never commit with failing tests or warnings.
- Never introduce `window.confirm` or other WebView2 quirks — use
  `modalConfirm`.
- Never touch the game `CompositePackageMapper.dat` without ensuring
  the `.clean` backup exists.
- Never hard-code English strings outside `translations.json`.
- Never skip a step to "save time" — the loop is the point.

## Escalation

If the same criterion fails three iterations in a row:

1. Run `fpf:propose-hypotheses` on the failure.
2. Post the resulting Decision Rationale Record to
   `.specs/prd/decisions/<YYYY-MM-DD>-<slug>.md`.
3. Either rewrite the acceptance criterion to match reality with a note
   explaining why the original statement was wrong, or mark the
   criterion as `[~]` with a link to the DRR and move on.
4. Never silently drop a criterion.

## Termination gate

The loop stops (and only stops) when **all** of the following hold:

1. Every `[ ]` in `acceptance-criteria.md` is `[x]` or `[~]` with an
   attached DRR.
2. `cargo clippy --all-targets --release -- -D warnings` exits 0.
3. `cargo test --release` exits 0.
4. `npm test` exits 0.
5. `npm run test:e2e` exits 0.
6. A full Playwright happy-path run produces zero
   `console.warn`/`console.error`.
7. `reflexion:critique` returns consensus "no further improvements"
   across three judges against the cumulative diff.
8. Deploy workflow produces a signed release end-to-end.
9. PRD status field is updated to `shipped`.

When the gate fires, the loop emits a single final summary describing
what was achieved, what's shipped, and closes.
