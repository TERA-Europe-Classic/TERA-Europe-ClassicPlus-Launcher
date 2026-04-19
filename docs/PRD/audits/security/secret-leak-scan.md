# Secret Leak Scan — All 5 Repos

**Criterion:** PRD §3.1.6.
**Tool:** gitleaks v8 (latest at iter 13, installed via `go install github.com/zricethezav/gitleaks/v8@latest`).
**Command:** `gitleaks detect --source <repo> --report-format json`.
**Scope:** full git history of each repo.
**Iter:** 13 of the perfection loop.
**Status:** audit done — remediation pending human decisions on two true-positives.

## Coverage

| Repo | Commits scanned | Bytes | Raw hits |
|------|-----------------|-------|----------|
| `TERA-Europe-ClassicPlus-Launcher` (private) | 244 | 5.3 MB | 6 |
| `teralib` (private) | 43 | 448 KB | 0 |
| `external-mod-catalog` (public) | 6 | 130 KB | 0 |
| `TCC` (public fork) | 4,789 | 209 MB | 26 |
| `ShinraMeter` (public fork) | 1,583 | 151 MB | 1 |
| **Total** | **6,665** | **366 MB** | **33** |

## Triage

All 33 raw hits were reviewed manually. One file per category of finding:

### ✗ True positive — ShinraMeter (1)

- **`Data/WindowData.cs:84`** (commit `ea5a3af8`, upstream-era).
  ```csharp
  TeraDpsToken = "H0XJ9RGZO8qkkP2Nl6pl_YkARjC81wkCdETY3mNLKGF";
  TeraDpsUser = "KxjWQFyQJp5CMhXmy";
  ```
  This is a hard-coded default `teradps.io` token baked into the Shinra binary. Likely a demo/seed credential from an upstream author, not an end-user token — but it's shipped with every Shinra build. Predates the Classic+ fork; inherited from `neowutran/ShinraMeter`.

  **Risk:** if `teradps.io` still validates this token, anyone running an unconfigured Shinra defaults to this account and pollutes its stats. If the service is dead (the repo has been unmaintained since Nov 2022), risk is nil.

  **Decision required from human:** (a) blank the default to `""` and push a normal commit (simplest, respects `teradps.io` if alive), (b) rotate — impossible if we don't control the account, (c) leave as-is and document the risk here. Recommend (a).

  **No history rewrite needed** — the token lives in every historical version of the file; rewriting history on a fork of a dead upstream has no security benefit because the same token is already captured in every clone of the original repo on GitHub. Containment is via blanking the current version.

### ⚠ Leaky-but-not-secret — launcher (4)

- **`teralaunch/.vs/teralaunch/config/applicationhost.config:126-127`** (commits `3d9cff31` and `36eb0c00`). Contains DPAPI-encrypted IIS Express cert binding blobs. DPAPI payloads are per-user + per-machine; decrypting them on a different machine is infeasible. Not a secret in the exfiltration sense.

  **But:** the `.vs/` directory is a Visual Studio IDE scratch dir and should never have been tracked. Remediation:
  1. Add `.vs/` to `.gitignore` (touches launcher repo, no history rewrite).
  2. `git rm --cached -r teralaunch/.vs/` in a normal commit.
  3. Leave history alone — the DPAPI blobs are useless outside this machine.

  **No credential rotation needed.**

### ✓ False positives — launcher (2)

- **`teralaunch/src-tauri/src/services/auth_service.rs:497 and :738`** — literal string `"abc123def456"` inside `#[cfg(test)] mod tests` blocks as a fixture for `parse_auth_key_valid`. Clearly fake. Gitleaks `generic-api-key` regex matched the alphanumeric length. Safe to add to gitleaks allow-list.

### ✓ False positives — TCC (26)

All 26 hits are XAML `<LinearGradientBrush x:Key="...">` / `<SolidColorBrush x:Key="...">` resource keys: `TccGreenGradient0Brush`, `TccNormalGradient1Brush`, `Tier1DungeonBrush` through `Tier5DungeonBrush`, etc. These are just WPF resource identifiers, not credentials. The `generic-api-key` regex catches alphanumeric identifiers of a certain length.

Safe to add these filenames / patterns to a `.gitleaks.toml` allow-list.

## Planned remediation

To close PRD §3.1.6 fully, the following follow-up iters are queued:

1. **fix.shinra-teradps-token** (P0, new) — blank the hard-coded `TeraDpsToken` / `TeraDpsUser` defaults in Shinra's `Data/WindowData.cs` to empty strings. Ship a normal commit. Decision: option (a) from the true-positive section.
2. **fix.launcher-vs-dir-tracked** (P0, new) — add `.vs/` to `.gitignore`, untrack `teralaunch/.vs/` tree via `git rm --cached -r`. No history rewrite.
3. **infra.gitleaks-allowlist** (P1, new) — add `.gitleaks.toml` to each scanned repo declaring allow-list entries for the known false positives (auth_service test fixtures, TCC brush keys). Keeps future CI scans focused.
4. **infra.secret-scan-ci** (P0, new) — author `.github/workflows/secret-scan.yml` for each public repo (`external-mod-catalog`, TCC, Shinra) that runs gitleaks against the PR diff and fails on any new hit that isn't allow-listed.

## Acceptance status (per PRD §3.1.6)

- [ ] `gitleaks` CI exits 0 on every repo → **pending** (items 3 and 4).
- [ ] Audit doc lists all rotated secrets → **done** (this file, though no rotation needed; see Shinra true-positive for the one relevant decision).
- [ ] History rewrite applied where required → **not applicable** (the one true-positive's mitigation is forward-only; DPAPI blobs are per-machine).

## Raw findings

Captured at `/tmp/gitleaks/*.json` on the scanning host (iter 13) for reference:
- `TERA-Europe-ClassicPlus-Launcher.json` (6 entries)
- `ShinraMeter.json` (1 entry)
- `TCC.json` (26 entries)
- `teralib.json` (empty)
- `mod-catalog.json` (empty)

Not committed — they contain the raw secret strings. If these need archival, scrub the `Secret` field first.
