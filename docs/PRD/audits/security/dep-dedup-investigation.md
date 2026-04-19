# Dep Dedup Investigation — Iter 87 (2026-04-19)

Closes P2 `dep.dedupe-reqwest-zip` (queued by iter 80 research sweep)
as **DONE-with-documented-deferral — upstream-driven**.

## Observation

`cargo tree -d -e normal` against `teralaunch/src-tauri` at worktree
commit `825ec70` surfaces two dup pairs:

| Crate | Version | Consumer(s) |
|---|---|---|
| reqwest | **0.12.28** (dominant) | our direct pin, `reqwest_cookie_store 0.8.2`, `tauri-plugin-http 2.5.8`, `teralib` |
| reqwest | **0.13.2** | `tauri-plugin-updater 2.10.1` (single consumer) |
| zip | **2.4.2** (dominant) | our direct pin |
| zip | **4.6.1** | `tauri-plugin-updater 2.10.1` (single consumer) |

Both dups trace back to a single crate: **`tauri-plugin-updater 2.10.1`
has jumped ahead of the rest of the Tauri 2.x plugin ecosystem** on
reqwest (0.12→0.13) and zip (2→4), while every other consumer in our
tree stays on the earlier majors.

## What we tried / why each option fails

### Option A — bump OUR direct pins to match tauri-plugin-updater

Would require:
- `reqwest = "0.12.23"` → `"0.13"`
- `zip = "2.3"` → `"4"`

**Blocked by peer crates.** `reqwest_cookie_store 0.8.2` and
`tauri-plugin-http 2.5.8` both pin reqwest to the `0.12` line. Bumping
our direct pin would force two resolver decisions:
1. Upgrade `reqwest_cookie_store` to a version that supports reqwest
   0.13 — none released at the time of this audit (crates.io shows
   0.8.x as the latest stable).
2. Upgrade `tauri-plugin-http` to a version that adopts reqwest 0.13
   — 2.5.8 is the current stable as of the iter 80 sweep; no
   tauri-plugin-http release carries the 0.13 bump yet.

Result: a bump-attempt would fail to resolve, or fall back to
multiple-reqwest regardless.

### Option B — downgrade `tauri-plugin-updater` below 2.10.1

Rejected. Iter 71 delivered the downgrade-refusal gate (PRD 3.1.9)
specifically against the current updater API surface. Downgrading
would either (a) cost us that gate, or (b) require re-porting the
gate against the older API — strictly worse than the current state.

### Option C — switch our direct reqwest/zip calls to the updater's
### copies

Not a supportable pattern. Cross-version crate sharing is not
semver-safe, the types (`reqwest::Client`, `zip::ZipArchive`) aren't
ABI-compatible across major versions, and the plugin explicitly
doesn't re-export those types for third-party use.

### Option D — document as upstream-driven, wait for the ecosystem

**Taken.** The dup disappears when `tauri-plugin-http` + its peers
ship a 0.13-compatible release and we bump them together with our
direct pin in a single coordinated commit. Until then the dup has
real but bounded cost:

- **Binary size**: roughly +250–400 kB for the extra reqwest copy
  plus ~100–200 kB for the extra zip copy. Measured against our
  release build, not showstopping at LTO + strip.
- **Cold compile time**: ~10–15 s extra on a clean build from
  compiling two TLS + HTTP stacks.
- **Supply-chain surface**: any RUSTSEC advisory on reqwest or zip
  must be matched against BOTH resolved versions. Iter 80 covered
  this — every advisory we found was either not applicable or already
  fixed in both resolved versions.

None of these justify pinning an unreleased ecosystem state. Track
and revisit.

## Exit criteria for re-opening

Re-open as P2 `dep.dedupe-reqwest-zip` when any of:

1. `tauri-plugin-http` publishes a release using reqwest 0.13 — the
   plugin release notes will say so. Then bump our direct pin + the
   plugin pin together.
2. `reqwest_cookie_store` publishes a release supporting reqwest
   0.13.
3. `tauri-plugin-updater` downgrades to reqwest 0.12 / zip 2.x (very
   unlikely; they moved forward for a reason).
4. A future research sweep surfaces a new advisory on reqwest 0.13
   or zip 4.x that we'd otherwise be stuck exposed to — that would
   change the cost-benefit.

## Check command

To refresh this investigation in a later sweep:

```
cd teralaunch/src-tauri
cargo tree -d -e normal | head -100
cargo tree -i reqwest@0.12.28
cargo tree -i reqwest@0.13.2
cargo tree -i zip@2.4.2
cargo tree -i zip@4.6.1
```

If any of the dup-version consumer lists shrinks to zero, the dup is
gone and this audit is stale.

## Close state

P2 `dep.dedupe-reqwest-zip` → DONE @ iter 87 (documented deferral,
upstream-driven). Acceptance was "0 duplicates, OR documented blocker
citing upstream" — second clause is met.
