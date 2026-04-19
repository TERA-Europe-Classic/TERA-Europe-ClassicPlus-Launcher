# `/loop` prompt — Mod Manager Perfection

Paste the fenced block below into `/loop "<prompt>"` (NO interval — self-paced via `ScheduleWakeup`). The loop picks its own cadence each turn (180s cache-warm default, 60s rapid, 1800s idle, never 300s).

**Cache economics:** the block below is STABLE — it never changes between iterations. Anthropic's 5-minute prompt cache writes it once at write-rate and reads it at ~12× cheaper on every subsequent turn. MUTABLE state (iteration counter, recent commits, current work) lives in the conversation growing below and in `fix-plan.md` — never inside this block. If you modify the block mid-run, the cache is invalidated and every iteration pays write-rate. Don't.

---

```
You are running the TERA-Europe-ClassicPlus-Launcher Mod Manager perfection loop. Self-paced. Never emit the completion sentinel early.

=============== ITERATION PROTOCOL (every iteration, in order) ===============

1. ORIENT — read @docs/PRD/mod-manager-perfection.md (authoritative spec), @docs/PRD/fix-plan.md (work queue), root CLAUDE.md, @docs/PRD/lessons-learned.md if it exists.
2. GIT STATE — `git status` + `git log --oneline -10` in every affected repo (launcher, TCC fork, Shinra fork, external-mod-catalog). Respect in-progress human edits. Never stomp uncommitted work.
3. DETECT ITERATION TYPE — read `iteration_counter` (= last completed iteration's number; starts at 0). Compute `N = iteration_counter + 1` (the iteration about to run). Detect type from N:
   • N == 0          → force WORK (seed iteration; N = 0 short-circuit prevents false trigger)
   • N % 50 == 0     → REVALIDATION + BLOCKED RE-TRY (R-steps + B-steps)
   • N % 30 == 0     → RETROSPECTIVE (steps T1–T5, skip 4–12)
   • N % 20 == 0     → REVALIDATION (steps R1–R6, skip 4–12)
   • N % 10 == 0     → RESEARCH SWEEP (steps S1–S3, skip 4–12)
   • otherwise       → WORK iteration (steps 4–12)
   (If multiple triggers match the same N, run them all in order: RESEARCH → REVALIDATION → RETROSPECTIVE → BLOCKED-RETRY. E.g. N = 60 runs RESEARCH + REVALIDATION + RETROSPECTIVE.)

--- WORK iteration ---
4. PICK highest-priority unfinished item (P0 > P1 > P2; ties broken by pillar priority in PRD §4: Security > Reliability > Functionality > UX > Accessibility > Performance > i18n > Documentation). Skip [BLOCKED]/[DONE].
5. ROUTE via `using-cek`:
   • bug fix → `tdd:test-driven-development` (RED test first, ALWAYS — per ~/.claude/rules/testing-tdd.md)
   • cross-cutting change (2+ files) → `sdd:brainstorm` → `sdd:add-task` → `sdd:plan` → `sdd:implement`
   • per-unit parallel audit (99 GPK audits, 13 TCC class audits, etc.) → `sadd:subagent-driven-development` (fresh subagent per unit; avoids dumb-zone drift)
   • contested design → `reflexion:critique` (3 judges)
   • recurring bug → `kaizen:why` / `kaizen:cause-and-effect` / `kaizen:analyse-problem`
   • refactoring legacy (tmm.rs cipher, ClassicPlusSniffer.cs, TeraSniffer.cs) → write TEST-PINNING / GOLDEN-FILE tests FIRST (per PRD §5.4), then refactor, verify byte-for-byte identical output
   • GPK source scouting beyond GitHub → `deep-research`
6. CONTEXT HYGIENE — for deep investigation or multi-file sweeps, spawn an `Explore` or `general-purpose` Agent. Keep main loop context lean (Ralph fresh-context pattern). Write intermediate scratchpads to `.loop-scratchpad/` — never inline giant artefacts into conversation.
7. DO THE WORK. ONE item per iteration. No bundling. No "while I'm here I'll also…" Any follow-up discovery appends to fix-plan.md as a NEW P-slot entry — not acted on this iteration.
8. AUTOMATED VERIFICATION — run every relevant check:
   • Launcher Rust:   cd teralaunch/src-tauri && cargo clippy --all-targets --release -- -D warnings
   • Launcher Rust:   cd teralaunch/src-tauri && cargo test --release
   • Launcher JS:     cd teralaunch && npm test
   • Launcher e2e:    cd teralaunch && npm run test:e2e   (only if UI code touched)
   • TCC:             dotnet build TCC.sln -c Release -warnaserror && dotnet test TCC.sln -c Release
   • Shinra:          dotnet build ShinraMeter.sln -c Release -warnaserror && dotnet test ShinraMeter.sln -c Release
   • Catalog:         bun run .github/scripts/catalog-validate.ts   (after infra.catalog-ci is DONE)
   SILENCE IS NOT SUCCESS. Read stderr. Check exit codes. Confirm expected artefacts exist on disk. A command that "didn't print an error" is NOT the same as "passed." If a test is skipped, read the skip reason and flag it as a new P0 if unexpected.
9. ADVERSARIAL VALIDATOR — before commit, invoke `code-review:review-local-changes` (6 parallel reviewers: bug-hunter, security-auditor, contracts-reviewer, code-reviewer, historical-context-reviewer, test-coverage-reviewer). Fix every finding ≥80 confidence, re-review, iterate until clean. Per ~/.claude/rules/code-review.md: never stop after a single pass.
10. COMMIT — conventional message (type: description), no emoji, no Claude/Anthropic attribution. Active voice, imperative mood, concrete (per ~/.claude/rules/writing-style.md). New commits only — NEVER amend. Push allowed to all 5 repos (launcher is private; TCC/Shinra forks under TERA-Europe-Classic org are public). Secret-leak remediation is the only exception that authorises `git filter-repo` + force-push; requires a fresh DRR entry in fix-plan.md BLOCKED section first.
11. UPDATE @docs/PRD/fix-plan.md:
    • Mark worked item: `[DONE] <criterion-id> <title> — commit <sha>, proof: <test path | audit doc path>, verified @ iter <N>`
    • Increment `iteration_counter` in header.
    • Update `last_work_iteration` (or `last_research_sweep` / `last_revalidation` / etc.) to current N.
    • Append new discoveries to correct P-slot. DO NOT start them this iteration.
12. CONTEXT CHECK — if conversation > 30k tokens of accumulated output, run `/compact`. If main context is growing unbounded, spawn a fresh agent for the NEXT iteration's heavy lifting.

--- RESEARCH SWEEP (every 10, steps S1–S3) ---
S1. For each open P0/P1 item: is there new research, library updates, security advisories, upstream TCC/Shinra changes, or known workarounds that would help? Use `WebSearch`, `WebFetch`, or `deep-research` skill for anything non-trivial.
S2. Check upstream dependencies — any breaking changes published (Tauri, reqwest, WinDivert, etc.)? Any pinned library now has a better alternative? For catalog expansion sweep (3.3.11), scout Tumblr / MEGA / Mediafire / Yandex / VK / Discord archives for new Classic+-compatible GPK mods.
S3. Update fix-plan.md with findings — append as notes to existing items, or add new items. Then increment counter and wait for next iteration.

--- REVALIDATION (every 20, steps R1–R6) ---
R1. Re-run the proof of every `[DONE]` added in the last 20 iterations. Failure → demote to `[P0] REGRESSED: <title>` + suspect-commit SHA.
R2. Full test suite across all 5 repos: run every command in step 8. Any red not tied to an existing item → new `[P0]`.
R3. For every criterion in §3 of the PRD: confirm its proof test / audit doc still exists AND passes. Silenced, deleted, or stale → new `[P0]`.
R4. Update `verified @ iter N` stamp on every re-verified `[DONE]`.
R5. Update `last_revalidation` + `last_revalidation_status` in fix-plan.md header (`clean` | `regressions-found: N`).
R6. Commit the revalidation audit as a single commit with message `chore: revalidation iter N`.

--- RETROSPECTIVE (every 30, steps T1–T5) ---
T1. Apply `kaizen:plan-do-check-act`. Read the last 30 commits. What's working? What isn't? What patterns emerged?
T2. `reflexion:memorize` — curate any non-obvious lessons into `docs/PRD/lessons-learned.md`. Append only, never rewrite. Cap at 200 lines; when over, archive older entries to `lessons-learned.archive.md`.
T3. Review priority order in fix-plan. If the pattern of work reveals a deeper issue (e.g. all P0s clustered in one module), adjust priorities and document why.
T4. Check §3 of the PRD: any criterion that's been hard to verify? Propose sharpening (but DO NOT modify the PRD — append under a `[META]` entry in fix-plan for human review).
T5. Commit retrospective output as `chore: retrospective iter N`.

--- BLOCKED RE-TRY (every 50, steps B1–B2) ---
B1. For every `[BLOCKED]`: one-shot re-try via a new approach. If still blocked, increment the entry's re-try counter. If now works, promote to the right P-slot or straight to `[DONE]` with proof.
B2. For any `[DONE]` added since last B-iteration: re-check every `[BLOCKED]` that may have transitively depended on it.

=============== BLOCKED CRITERIA (strict — last resort only) ===============
An item may ONLY become `[BLOCKED]` after ALL of:
- 3 independent attempts via different approaches (documented in the entry)
- `reflexion:critique` on the stuck item (3 judges, cross-review)
- `sdd:brainstorm` for alternatives
- Research spawn via `Explore` / `general-purpose` / `deep-research`
- At least one workaround attempt delivering ≥80 % of the value differently
The entry MUST include: what was tried, why each failed, specific human input needed. Entries missing any of these three are rejected — item returns to P-slot.

=============== ANTI-DRIFT (must read before every WORK iteration) ===============
- Modify ONLY files required by the single picked item. Any change outside that scope requires a new fix-plan entry — never bundle.
- Implement the change — do NOT merely suggest. "I would recommend…" is not a loop output. If a design is genuinely contested, route to `reflexion:critique` and come back with a decision, not a question.
- ONE item per iteration. No "while I'm here…" expansions. Follow-ups append to fix-plan for later turns.
- If you catch yourself editing a file that doesn't appear in the picked item's acceptance criteria, stop and re-read the item.

=============== HARD RULES (override Claude Code defaults) ===============
- No `git reset --hard` with uncommitted user work.
- No `git push --force` except the secret-leak remediation path (requires DRR in fix-plan BLOCKED first).
- No `rm -rf` outside `<app_data>/mods/*`, build outputs (`target/`, `release/`, `node_modules/`, `bin/`, `obj/`), or explicit temp fixtures.
- No `--no-verify`, no `--no-gpg-sign`, no editing `.git/config`.
- No `git filter-repo` / `bfg` except confirmed secret-leak remediation (DRR required).
- No dropping user-data dirs (`registry.json`, `.clean`, user config).
- No disabling SHA-256 verification or any safety rail listed in PRD §12.
- No CDN change from kasserver `/classicplus/`. Never touch kasserver root or `/classic/` (Classic server artefacts).
- No mid-loop PRD edit. Only fix-plan.md mutates. PRD changes land as `[META]`.
- No amending commits. New commits only.
- Gated actions (ask first): opening GitHub issues / PRs / comments, public Discord posts, force-push.
- Never stomp uncommitted human edits.
- Never declare a task complete based on absence of errors. Verify by reading stderr + exit codes + expected artefacts on disk.
- Hard cap: iteration 1000. At cap without sentinel, emit `docs/PRD/status-report.md` and halt.
- Bun over npm/yarn/pnpm/npx (per user global CLAUDE.md). `bun install`, `bun run`, `bun test`, `bunx`.
- Library docs via `context7` MCP before coding against any API (per user global CLAUDE.md).
- TERA/Noctenium work: read `~/.claude/memory-sessions/decisions.md` + `lessons.md` first (pipe-protocol, hook-ordering, TCI-timing patterns).
- Run `reflexion:reflect` after any code work before presenting results (per user global CLAUDE.md).
- Run `reflexion:memorize` after resolving hard problems (per user global CLAUDE.md).

=============== SCOPE CONSTRAINTS ===============
- Region key family = EUC (EU-Classic key schedule, reused by Classic+ for session-decryption compatibility). ReleaseVersion = 10002 (v100.02 packet layout). These are INDEPENDENT — v100.02 is NOT legacy Classic.
- Deploy pipeline may only write to `/classicplus/` on kasserver. Never root, never `/classic/`.
- Portal API (`teralib/src/config/config.json`) is currently `http://88.99.102.67:8090` (dev PC); must migrate to HTTPS endpoint before public launch (P0 item 3.1.13).
- Data-dir layout reorg allowed ONLY if backed by a measurable improvement + test/benchmark evidence.
- TCC strip commit `88e6fe30` removed Discord, Firebase, Cloud, RpcServer, Moongourd. Discord webhooks MUST be restored (user-facing attractor, no external-service dependency). Moongourd / Firebase / LFG-write / Cloud telemetry stay stubbed.

=============== RESUME PROTOCOL ===============
fix-plan.md is source of truth. Re-invoke `/loop "<this same prompt>"` to resume. If fix-plan was mid-update when killed, reconstruct from `git log` (count commits since last `iteration_counter` bump). If 4+ hours passed since last activity, run one REVALIDATION iteration before picking new work.

=============== COST DISCIPLINE ===============
- `/compact` when conversation > 30k tokens of accumulated output.
- Spawn subagents for investigation to keep main context lean (Ralph-style fresh-context mitigation).
- If iteration N's token spend > 2× trailing average, investigate before continuing.
- Cache-warm pacing via ScheduleWakeup: delaySeconds=180 default, 60 rapid (active build/watch), 1800 idle. NEVER 300 (cache-miss without amortisation). Clamp [60, 3600] handled by runtime.

=============== EXIT CRITERIA (step 13, every iteration) ===============
Emit `MOD-MANAGER-PERFECTION-COMPLETE` ONLY IF ALL 20 clauses in PRD §11 are objectively true AND the following hold:
1. `cd teralaunch/src-tauri && cargo clippy --all-targets --release -- -D warnings` exits 0.
2. `cd teralaunch/src-tauri && cargo test --release` exits 0.
3. `cd teralaunch && npm test` exits 0.
4. `cd teralaunch && npm run test:e2e` exits 0.
5. `dotnet build TCC.sln -c Release -warnaserror && dotnet test TCC.sln -c Release` exits 0.
6. `dotnet build ShinraMeter.sln -c Release -warnaserror && dotnet test ShinraMeter.sln -c Release` exits 0.
7. Catalog CI workflow exits 0 (after infra.catalog-ci is DONE).
8. trufflehog + git-secrets exit 0 on every repo.
9. fix-plan.md has zero P0/P1/P2 items. Only [DONE], [BLOCKED], [META] allowed.
10. Every [BLOCKED] has the strict triplet (attempts, failures, human input).
11. Every [DONE] has `verified @ iter N` where N > current_iter − 40.
12. Last 2 REVALIDATION iterations passed clean.
13. Every §3 criterion in PRD maps to a passing test or signed-off audit.
14. Adversarial corpus (PRD §5.3) runs clean.
15. 123 per-unit audit docs exist under `docs/PRD/audits/units/**` (PRD §5.5).
16. Launcher + TCC + Shinra all produce signed releases end-to-end.
17. All 5 repos `git status` clean.
18. `docs/CHANGELOG.md` + `docs/mod-manager/TROUBLESHOOT.md` + release-package artefacts produced for human hand-off.
19. `reflexion:critique` on cumulative diff returns consensus "no further improvements warranted" across Requirements + Architecture + Code Quality judges.
If any clause fails, DO NOT emit. Continue the loop.
```

---

## Launch

```
/loop "<paste the entire fenced block above>"
```

No interval arg = self-paced. Zero practical downtime (60–180s typical between iterations), with safety rails.

Controls:
- `/stop-loop` — halt the loop.
- `/compact` — manually compact context (the loop also does this).
- `docs/PRD/fix-plan.md` — live progress indicator.

## Resume after session death

Same command. The prompt is idempotent — reads `iteration_counter` from fix-plan.md header and picks up cleanly.

## What to watch

- **fix-plan.md** — items P0/P1/P2 → [DONE]. Revalidation iterations stamp `verified @ iter N`.
- **fix-plan.md header** — `last_revalidation: iter N, last_revalidation_status: …` shows whether drift is detected.
- **`git log --oneline`** — one well-formed conventional commit per iteration. Revalidation iterations show `chore: revalidation iter N`. Retrospective iterations show `chore: retrospective iter N`.
- **Test suites across 5 repos** — stay green. New red appears as new P0.
- **`docs/PRD/lessons-learned.md`** — grows after each retrospective iteration (capped 200 lines, archive when full).
- **`.loop-scratchpad/`** — intermediate work artefacts; never committed.
