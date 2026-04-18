# Mod Manager PRD — index

Four documents, one purpose: drive the mod manager to production-ready
state with zero human testing, until no further improvements are
possible.

| File | Role |
|------|------|
| [`mod-manager-production.md`](./mod-manager-production.md) | The PRD. Vision, goals, requirements, edge cases, risks. |
| [`acceptance-criteria.md`](./acceptance-criteria.md) | The oracle. Every testable statement, grouped A–O + X. |
| [`test-plan.md`](./test-plan.md) | The proof. Each criterion → named automated test + layer. |
| [`ralph-loop-instructions.md`](./ralph-loop-instructions.md) | The script. Per-iteration steps, termination gate. |

## Start the loop

```
/loop
@.specs/prd/ralph-loop-instructions.md
```

The loop stops only when every acceptance criterion has a green test,
clippy and test suites are clean, a fresh Playwright run emits zero
console noise, and `reflexion:critique` with three judges returns
"no further improvements warranted" consensus.

## Scope

Mod manager feature of the TERA Europe Classic+ Launcher (`teralaunch/`)
plus the TMM-style GPK deployer (`services/mods/tmm.rs`) and the
external mod catalog repo.

## Out of scope

Mobile/tablet layouts, user-uploaded catalog entries, dependency
resolvers, third-party mod signing. See PRD §4.
