# Research Sweep — Iter 80 (2026-04-19)

Tenth research sweep since loop start; first since iter 70 (2026-03-?).
Worktree HEAD: `39b09e4` (tauri-v2-migration branch, awaiting user-gated
squash). All findings below are indexed against the lockfile at that
commit.

## Sources

- RustSec advisory-db — https://github.com/rustsec/advisory-db
- RustSec site — https://rustsec.org/advisories/
- GHSA DB via `gh api /repos/tauri-apps/tauri/security-advisories`
- GHSA DB via `gh api /repos/tauri-apps/plugins-workspace/security-advisories`
- TCC + Shinra upstream releases via `gh api /repos/TERA-Europe-Classic/*/releases`
- Upstream catalog commits — https://github.com/TERA-Europe-Classic/external-mod-catalog

## Resolved versions snapshot (Cargo.lock @ 39b09e4)

| Crate | Resolved | Notes |
|---|---|---|
| tauri | 2.10.3 | pin `"2"` |
| tauri-plugin-updater | 2.10.1 | pin `"2"` |
| tauri-plugin-dialog | 2.7.0 | pin `"2"` |
| tauri-plugin-http | 2.5.8 | pin `"2"` |
| tauri-plugin-shell | 2.3.5 | pin `"2"` — post-patch for CVE-2025-31477 |
| tauri-plugin-fs | 2.5.0 | pin `"2"` |
| tauri-plugin-process | 2.3.1 | pin `"2"` |
| tauri-plugin-notification | 2.3.3 | pin `"2"` |
| reqwest | 0.12.28 + **0.13.2** | dup — direct pin vs transitive via tauri |
| zip | 2.4.2 + **4.6.1** | dup — direct vs transitive |
| zeroize | 1.8.2 | pin `"1.7"` |
| zeroize_derive | 1.4.3 | transitive |
| semver | 1.0.27 | pin `"1"` |
| rustls | 0.23.36 | transitive |
| **rustls-webpki** | **0.103.9** | transitive — **flagged by 3 advisories** |
| tokio | 1.49.0 | transitive |

## Findings

### 1. rustls-webpki — three RUSTSEC advisories (all non-exploitable for us)

We resolve `rustls-webpki 0.103.9`. The following apply:

| Advisory | Fixed in | Vector | Exploitable here? |
|---|---|---|---|
| [RUSTSEC-2026-0049](https://rustsec.org/advisories/RUSTSEC-2026-0049.html) | 0.103.10 | CRL distribution-point matching fails — provided CRLs not consulted | **No** — reqwest's default rustls config does not use CRL; our GET-on-allowlist never passes CRL data |
| [RUSTSEC-2026-0098](https://rustsec.org/advisories/RUSTSEC-2026-0098.html) | 0.103.12 | URI name-constraint bypass on X.509 | **No** — public Web PKI does not use URI names; our TLS targets (GitHub raw, update mirror) are public CA-signed |
| [RUSTSEC-2026-0099](https://rustsec.org/advisories/RUSTSEC-2026-0099.html) | 0.103.12 | Wildcard cert name-constraint bypass | **No** — same reason; requires attacker-controlled CA asserting name constraints on a wildcard |

**Action**: queue dep bump as P2 — trivial to close (`cargo update -p
rustls-webpki --precise 0.103.12`) and clears all three advisories at
once. Non-exploitable today, but an open advisory row on any future
`cargo audit` CI gate would fail the build.

### 2. tauri-plugin-shell CVE-2025-31477 — already patched

[GHSA-c9pr-q8gx-3mgp / CVE-2025-31477](https://github.com/tauri-apps/plugins-workspace/security/advisories/GHSA-c9pr-q8gx-3mgp)
— `shell.open()` default scope accepted arbitrary protocols (`file://`,
`smb://`, `nfs://`) → RCE. Affected ≤2.2.0, patched ≥2.2.1. **We run
2.3.5 — post-patch.**

However, our [`capabilities/migrated.json`](../../../../teralaunch/src-tauri/capabilities/migrated.json)
lists `"shell:allow-open"` bare — the default scope. The advisory
recommends explicitly pinning the scope in `tauri.conf.json`:

```json
"plugins": {
    "shell": { "open": true }
}
```

`true` means "only mailto, http, https." On 2.3.5 the default already
does this, but an explicit pin (or migration to the recommended `opener`
plugin — the `shell.open` endpoint is formally deprecated) would be
defence-in-depth against a future default regression.

Frontend call sites in `src/app.js:2259,2261,5025` pass either a
localized URL constant or a link-click `event.target.href` value —
no untrusted-user-input path today.

**Action**: queue as P2 defence-in-depth.

### 3. Tauri core — GHSA-57fm-592m-34r7 / CVE-2024-35222 (iFrame IPC bypass)

Remote-origin iFrames could access Tauri IPC without being in
`dangerousRemoteDomainIpcAccess` / `capabilities`. **We do not embed any
iFrame** in the webview, so non-exploitable. Recorded for completeness.

### 4. zip — CVE-2025-29787 (already fixed in our resolved 2.4.2)

[GHSA-94vh-gphv-8pm8 / CVE-2025-29787](https://github.com/advisories/GHSA-94vh-gphv-8pm8)
— zip-slip via symlink canonicalization. Affected 1.3.0–2.2.x, fixed
2.3.0. We run 2.4.2 + 4.6.1 — both past the fix.

### 5. reqwest — no advisories since 2025-10-01

Advisory-db `crates/reqwest/` directory returns 404. No findings.

### 6. zeroize / semver / tokio — no advisories since 2025-10-01

Clean. tokio's most recent advisory is RUSTSEC-2025-0023 (2025-04-07,
before cutoff). rustls core's most recent is RUSTSEC-2024-0399.

### 7. Dep duplication — reqwest 0.12.28+0.13.2, zip 2.4.2+4.6.1

`cargo tree` surfaces duplicate reqwest (direct `0.12.28` pin vs
transitive `0.13.2` pulled by tauri 2.10.3) and duplicate zip (direct
`2.4.2` for external-mod extraction vs transitive `4.6.1` somewhere in
the Tauri tree). Each dup adds:

- ~200–400 kB to the release binary
- A second attack surface to audit for every future advisory (each
  resolved version must separately prove non-vulnerable)
- Slower cold builds (already measurable on LTO+CFG pipeline)

Unlikely to be cleanly fixable without a tauri minor/patch bump that
aligns the transitive pin with our direct pin, but worth investigating
whether our direct `reqwest = "0.12.23"` pin in `Cargo.toml` can be
relaxed to `"0"` to pick up 0.13.2 once the reqwest API stabilises. For
zip, the transitive 4.6.1 probably comes from `tauri-runtime` — similar
story.

**Action**: P2 investigation item, low priority. Mainly binary-size
hygiene.

### 8. Vitest — we're on 2.1.8, latest stable is 4.x

No security advisories on the 2.x line. 3.x brought `spy.mockReset`
changes and `@sinonjs/fake-timers` v14 — low-risk upgrade. 4.x further
breaking changes. Not blocking anything; queue for post-squash
"bump-everything" iter if we want test-infra currency.

### 9. Playwright — we're on `^1.58.0`, latest stable is 1.58.2

No CVEs surfaced. 1.58 added `isLocal` optimisation for `connectOverCDP`
— not relevant to our single-host e2e setup. Safe to stay.

### 10. gitleaks — we pinned 8.30.0 in iter 13

Search didn't surface direct rule-update news between 8.30.0 and today.
Not blocking; the gitleaks action in `.github/workflows/secret-scan.yml`
pulls a recent binary at CI time anyway (no lock on the rule pack). If
the CI gate starts flagging new patterns that are project false
positives, queue a `.gitleaks.toml` burn-down (already tracked as
`infra.gitleaks-allowlist` P1 in fix-plan — no change needed).

### 11. NSIS / SmartScreen

No NSIS-specific advisory since last sweep. Industry guidance unchanged:
(a) sign installer + uninstaller with an EV-or-equivalent code-signing
cert, (b) give SmartScreen reputation time (~24–48 h after each
release), (c) use `!finalize` + `!uninstfinalize` for the signing hook.
Our current pipeline (builder.ps1 + signtool) follows this, and the
Tauri NSIS bundler since 2.10.x emits `!finalize`-friendly scripts.

### 12. Shinra / TCC upstream — catalog is current

| Mod | Catalog entry version | Upstream release | Match? |
|---|---|---|---|
| TCC | v2.0.1-classicplus | v2.0.1-classicplus (2026-04-18T22:17Z) | ✔ |
| Shinra Meter | v3.0.0-classicplus | v3.0.0-classicplus (2026-04-18T17:05Z) | ✔ |

Catalog commit [33cf584](https://github.com/TERA-Europe-Classic/external-mod-catalog/commit/33cf584)
(2026-04-18T22:19Z) bumped TCC — the catalog is in sync.

## Recommendations → fix-plan action items

| Priority | ID | Acceptance |
|---|---|---|
| **P2** | `dep.rustls-webpki-bump` | `cargo update -p rustls-webpki --precise 0.103.12` lands on worktree; `Cargo.lock` shows `0.103.12`; RUSTSEC-2026-0049, -0098, -0099 cleared. |
| **P2** | `sec.shell-scope-hardening` | Either (a) pin `"shell": { "open": true }` in `tauri.conf.json`, or (b) migrate to `tauri-plugin-opener` and drop `shell:allow-open`. Add test at `tests/shell_scope_pinned.rs` pinning the chosen form. |
| **P2** | `sec.shell-open-call-sites-pinned` | Author `teralaunch/tests/shell-open-callsite.test.js` grepping `src/` for `shell.open(X)` — every X must be a string literal, localized constant, or `event.target.href` from our `<a>` anchors (no arbitrary `fetch()` response value, no URL constructed from user input). |
| **P2** | `dep.dedupe-reqwest-zip` | Investigate whether relaxing `reqwest = "0.12.23"` pin or a tauri patch bump aligns transitives. Acceptance: `cargo tree -d` shows 0 duplicates among reqwest + zip, OR documented blocker + dependency on upstream tauri. |
| **P3** | `dep.vitest-bump-post-squash` | After squash + 1-week stability window, bump `vitest`/`@vitest/coverage-v8` to 4.x; migration per official guide. Acceptance: 431/431 JS tests still green post-bump. |

P1 vs P2 rationale: none of the rustls-webpki advisories is exploitable
in our TLS pattern (no CRL consult, no URI-name cert chain, no wildcard
asserting name constraints). CVE-2025-31477 we're already past. The
shell hardening is defence-in-depth, not a live exposure. So everything
drops to P2 — no bleeding, but worth queueing for the first post-squash
batch where we can bundle all dep moves into one version-bump commit.

## Cross-sweep delta (vs iter 70 sweep)

- Iter 70 flagged M6 CFG rustc coverage (deferred to CI RUSTFLAGS
  scope); that remains deferred — no movement.
- New this sweep: three rustls-webpki advisories (all published Jan–Apr
  2026, after iter 70), one Tauri shell plugin advisory (published
  2025-04, pre-cutoff but caught this time because we sweep the GHSA DB
  directly rather than relying on Web indexing).
- Catalog has since iter 70 grown from 12 to 101 entries (iter-6 audit
  item); no new security concerns on the mod side.

## Next sweep

Scheduled for iter 90 per N%10=0 cadence. Focus suggestion: Cargo.lock
diff against iter 80 baseline + any Tauri 2.11 or 3.0 signals.
