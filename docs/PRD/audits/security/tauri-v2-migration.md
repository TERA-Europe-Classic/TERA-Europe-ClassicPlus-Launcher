# Tauri 1.x → 2.x Migration Audit

**Criterion:** PRD §3.1 (umbrella) + gates 3.1.8 (anti-reverse), 3.1.9 (updater-downgrade), 3.1.12 (CSP unsafe-inline).
**Fix-plan item:** `sec.tauri-v1-eol-plan`.
**Status:** Draft — recommendation stands, milestones pending human sign-off.
**Last updated:** iter 18.

## Background

Tauri 2.0 stable shipped 2024-10-02. Tauri 1.x is in security-backport-only mode: no new features, no plugin updates, CVEs land on 2.x first and are backported only when severity warrants it. The Tauri core team has publicly set a 1.x EOL window (announced in the 2.0 release notes — end of life for 1.x security backports tied to 2.x LTS milestones).

Three PRD §3.1 items depend on 2.x-only features:

| PRD item | v2-only feature needed |
|---|---|
| 3.1.8 anti-reverse | Capability ACLs — permission-scoped command dispatch (`capabilities/*.json`) replaces the v1 global allowlist so each window can expose a strict minimal command surface. |
| 3.1.9 updater-downgrade-refuse | Tauri 2 updater (`tauri-plugin-updater`) exposes semver-aware `check().is_newer()` plus a pluggable policy hook; v1 updater trusts whatever `latest.json` claims. |
| 3.1.12 CSP unsafe-inline | v2 supports per-window CSPs and strict-nonce patterns (`csp_hash` in `tauri.conf.json`). v1 has one global CSP and looser enforcement. |

## Current 1.x surface

### Launcher (`teralaunch/src-tauri`)

**Crate deps** (`Cargo.toml`):

```
tauri = "1.0", features = [window-start-dragging, dialog-all, fs-{create-dir,exists,read-dir,read-file,remove-file,write-file}, http-request, notification-all, path-all, process-{exit,relaunch}, shell-open, window-{close,hide,maximize,minimize,show,unmaximize}, updater]
tauri-build = "1"
```

**Allowlist** (`tauri.conf.json`):
- `fs`: readFile, writeFile, readDir, createDir, removeFile, exists — scoped to `$APP/*`, `$RESOURCE/*`.
- `path`: all.
- `dialog`: all.
- `shell`: open + one `cmd /C start {validator:\S+}` scope.
- `window`: close, hide, show, minimize, maximize, unmaximize, startDragging.
- `process`: exit, relaunch.
- `http`: request — scope 8 hosts (6 prod HTTPS + 1 dev HTTP at `157.90.107.2:8090` tracked by 3.1.13).
- `notification`: all.

**Command surface**: **40 `#[tauri::command]` fns** across 7 modules (auth: 5, config: 6, download: 4, game: 4, hash: 6, mods: 9, util: 6). All registered in a single global `invoke_handler`.

**Windows**: 1 (`main` — 1282×759, transparent, decorations=false). No multi-window support implemented.

**CSP**: single global string —
```
default-src 'self'; script-src 'self' 'unsafe-inline' https://cdnjs.cloudflare.com; style-src 'self' 'unsafe-inline' ...; connect-src 'self' https://*.tera-europe.net https://*.tera-germany.de https://*.crazy-esports.com https://web.tera-germany.de https://helpdesk.crazy-esports.com https://tera-europe-classic.com http://157.90.107.2:8090
```
`'unsafe-inline'` on `script-src` tracked separately by 3.1.12.

**Updater**: active, endpoint `https://web.tera-germany.de/classic/classicplus/latest.json`, minisign pubkey pinned in `tauri.conf.json`. `main.rs::setup()` calls `app_handle.updater().check().await` when `TERA_LAUNCHER_AUTO_UPDATE` is set.

**Release profile**: `lto=true, codegen-units=1, panic="abort", strip=true, opt-level=3`. Already tight; no v1→v2 delta here.

### Plugins in use

v1 built-in features — each maps to a separate v2 plugin crate:

| v1 feature | v2 plugin |
|---|---|
| `dialog-all` | `tauri-plugin-dialog` |
| `fs-*` | `tauri-plugin-fs` (scope moves from `tauri.conf.json` to permission files) |
| `http-request` | `tauri-plugin-http` (scope moves to permission files) |
| `notification-all` | `tauri-plugin-notification` |
| `shell-open` | `tauri-plugin-shell` |
| `process-*` | `tauri-plugin-process` |
| `updater` | `tauri-plugin-updater` |
| `window-*`, `path-all` | built into `tauri` v2 core (no separate crate) |

## Breaking changes impacting us

Sourced from the official Tauri 2 migration guide + plugin release notes.

1. **Crate rename / bump.** `tauri = "1"` → `tauri = "2"`; `tauri-build = "1"` → `"2"`. Feature flags for built-in allowlist entries (`dialog-all`, `fs-*`, etc.) disappear — they become plugin crates.
2. **Allowlist → Capabilities.** The entire `tauri.allowlist` JSON block in `tauri.conf.json` is removed. Replaced by `src-tauri/capabilities/*.json` files that bind **permissions** (namespaced: `fs:allow-read-file`, `http:allow-request`, `shell:allow-open`, etc.) to a set of webview labels. Each plugin ships its own permission catalogue. **Every allowlist entry we currently use must be re-authored as an explicit permission binding**, scopes and all.
3. **Global permission -> per-window.** Capability files carry `"windows": ["main"]` (or globs). This is the mechanism 3.1.8 wants: a minimal capability set for the launcher window with no inherited global defaults.
4. **CSP per-window.** v2 enforces CSP more strictly and supports per-window CSPs. `csp` in `tauri.conf.json` stays but is now the default; per-window overrides live in window config. Hashes/nonces are recommended over `unsafe-inline` (this is what 3.1.12 fixes).
5. **Rust API diffs.**
   - `app.get_window("main")` → `app.get_webview_window("main")`.
   - `tauri::Builder::default()` + `.plugin(tauri_plugin_*::init())` required for every plugin you pulled in.
   - `app_handle.updater()` moves to `tauri_plugin_updater::UpdaterExt::updater()` (trait import).
   - `#[tauri::command]` unchanged at the macro level; function signatures unchanged.
6. **JS API diffs.**
   - `@tauri-apps/api/tauri` → `@tauri-apps/api/core` (e.g. `invoke`, `convertFileSrc`).
   - `@tauri-apps/api/fs`, `/dialog`, `/http`, `/notification`, `/shell`, `/process`, `/updater` → `@tauri-apps/plugin-*` packages installed separately (one `npm install` per plugin).
   - `@tauri-apps/api/window` → `@tauri-apps/api/webviewWindow` (the `WebviewWindow` class is the v2 equivalent of v1's `appWindow`).
   - `withGlobalTauri: true` in `tauri.conf.json` still works but the shape of `window.__TAURI__` changed (plugin namespaces instead of flat globals).
7. **Updater manifest.** v1's `latest.json` fields (`name`, `notes`, `pub_date`, `platforms.<target>.signature`, `platforms.<target>.url`) are compatible with v2's default format. However, v2 adds `format: "nsis" | "app" | "wix" | ...` for clarity and supports `deploymentStatus`-style custom resolvers via `updater.endpoints` returning a JSON the plugin parses. Our minisign key format is unchanged (v2 still uses minisign).
8. **Bundle config.** `tauri.bundle.*` mostly unchanged but some keys renamed (e.g. `windows.nsis.installMode: "perMachine"` → `windows.nsis.perMachine: true`). Every current key needs a 1:1 check against the v2 schema.
9. **Event system.** `listen` / `emit` signatures unchanged at call sites but the wire format / payloads are typed more strictly — any code that constructs raw `tauri::Event` structs needs review. We don't have much of this.
10. **Devtools.** We pull in a separate `devtools = "0.3.3"` crate for remote tracing — v2 ships first-class devtools so this dep may be removable (not required for migration).

## Risks of staying on 1.x

- **Security backports only.** Non-security bugs and new CVEs land on 2.x first. Backport window shrinks every quarter. Any embargoed disclosure that lands 2.x-only during the 1.x tail window forces a forced-march migration.
- **Plugin ecosystem moves on.** Third-party plugins (including ours, if we ever add one) drop v1 targets.
- **Blocks PRD items.** 3.1.8, 3.1.9, 3.1.12 cannot be resolved idiomatically on v1. Workarounds exist (hand-rolled CSP strict mode, manual semver check in JS before invoking updater.download, command-level `#[cfg]` gating) but each one is a custom layer the v2 platform already provides.
- **Team onboarding debt.** The Tauri docs site defaults to v2 now; v1 examples are moving to an archive. Every new contributor reads v2 guides and has to translate back to v1.

## Risks of migrating

- **Surface area.** 40 commands + 7 plugin migrations + 1 frontend (ES modules + bundled dist) + installer (NSIS) + signing (minisign) + a custom deep-link handler (`teraclassicplus://`) + an automatic-updater bootstrap path. Moderate.
- **NSIS installer.** `installMode: "perMachine"` → `perMachine: true` is a key rename. The installer passed through two hand-signed public releases (0.1.11, 0.1.12); must re-verify on v2.
- **Deep-link.** We parse deep-link URIs from argv in `main()` before the Tauri builder starts. v2 has a `tauri-plugin-deep-link` plugin that handles this — the current argv-parse path can stay (it's OS-level, pre-Tauri) or be replaced with the plugin. Replacing is cleaner; leaving is zero risk.
- **Updater cutover.** Existing 0.1.x user installs use the v1 updater. When they pull the first v2 build, the v1 updater does the download/install. Subsequent updates use the v2 updater. `latest.json` schema changes must be additive or dual-format for one release.
- **Frontend breakage.** `@tauri-apps/api/tauri` → `/core` + 7 plugin imports is a find/replace across `teralaunch/src/**/*.js`. Test surface is 417 Vitest unit tests + 70 Playwright e2e tests — good coverage.
- **Third-party crate churn.** Our Rust deps don't depend on Tauri internals so unaffected.

## Migration scope estimate

Rough order-of-magnitude, not a commitment:

| Milestone | Scope | Risk |
|---|---|---|
| **M1 — preflight** | Run `npx @tauri-apps/cli@next migrate` in a scratch branch; triage output; add baseline audit to `docs/PRD/audits/security/tauri-v2-migration-preflight.md`. | Low. |
| **M2 — Cargo + conf flip** | Bump `tauri`/`tauri-build` to 2; split features into plugin crates; rewrite `tauri.conf.json` (drop allowlist, add capability file stubs, move updater endpoint, adjust bundle keys). | Medium — compile breakage expected; fix until clean. |
| **M3 — plugin registrations + command re-export** | `Builder::default().plugin(...)` for each of 7 plugins; update any Rust-side `updater()` call sites; touch `main.rs::setup` for the `get_webview_window` rename. Rebuild `cargo test --release` to prove command dispatch still works. | Low–medium. |
| **M4 — capability authoring** | Author `src-tauri/capabilities/launcher.json` with the minimum permission set derived from the v1 allowlist table above. Verify the launcher runs end-to-end with capabilities alone; unset permissions should fail with a clear denied error. | Medium — first real capability file we've written. |
| **M5 — Frontend JS migration** | Replace `@tauri-apps/api/tauri` imports with `/core` + plugin packages; run Vitest + Playwright; fix breakage. | Medium. |
| **M6 — Updater cutover** | Publish a dual-format `latest.json` (v1 + v2 compatible). Ship a 0.2.0 v2 build. Confirm 0.1.12 → 0.2.0 upgrade works via v1 updater, and 0.2.0 → 0.2.1 works via v2 updater. | Medium — one-way door on user installs, staged rollout recommended. |
| **M7 — CSP tighten + capability narrow** | With v2 in prod, close PRD 3.1.8, 3.1.9, 3.1.12. | Low — scaffolding done by M1–M6. |

No calendar promises here — this is a scoping sketch, not a plan.

## Recommendation

**Migrate to Tauri 2.x**. Reasons:

1. 1.x is not a destination — backport window is finite and visible.
2. Three PRD §3.1 items are structurally gated on v2 features. Workarounds exist but duplicate platform code.
3. The 40-command surface + single-window shape makes this a small-to-moderate migration, not a rewrite. v2 has a codemod (`npx @tauri-apps/cli migrate`) that handles most of the JSON and Cargo churn.
4. NSIS + minisign continuity is preserved — bundle + signing stack unchanged.

**Human decision gates** (blocking before M2):
1. Approve migration (vs stay-on-1 with compensating controls).
2. Pick a target v2 release version (latest stable at start-of-M1, pinned for the remainder).
3. Confirm 0.2.0 is acceptable as the version bump that cuts over.
4. Decide dual-format `latest.json` duration (1 release? 3 releases?).

## Compensating controls if we stay on 1.x

Not recommended, but documented for completeness. Each has a PRD item that must be opened if we take this path:

- **3.1.8 anti-reverse on v1**: hand-author a global allowlist to the absolute minimum; gate sensitive commands with build-time `#[cfg(debug_assertions)]`. Acceptance bar lower than v2 capabilities.
- **3.1.9 updater-downgrade on v1**: compare `current_version` against `update.version()` in JS before calling `update.downloadAndInstall()`. Proof: integration test mocks an older `latest.json`. Misses the v2 plugin-level defence-in-depth.
- **3.1.12 CSP on v1**: remove `'unsafe-inline'` from `script-src` by refactoring every inline `<script>` to external modules. Doable but no per-window isolation.

## Acceptance

This audit closes `sec.tauri-v1-eol-plan` when:
1. Human signs off the recommendation (migrate, with milestones; or stay-on-1 with the compensating-controls PRD items opened).
2. If migrate: M1–M7 become concrete P0/P1 items in fix-plan.md with owner + target-release.
3. If stay-on-1: PRD items 3.1.8/3.1.9/3.1.12 are amended to reference v1-only acceptance bars.

## References

- Tauri 2.0 release notes: <https://tauri.app/blog/tauri-20/>
- v1 → v2 migration guide: <https://v2.tauri.app/start/migrate/from-tauri-1/>
- Plugins catalogue: <https://v2.tauri.app/plugin/>
- Capability / permission reference: <https://v2.tauri.app/security/capabilities/>
- `cargo-tauri migrate` CLI: <https://v2.tauri.app/reference/cli/#migrate>
