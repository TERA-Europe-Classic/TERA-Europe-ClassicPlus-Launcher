# Mod Manager — UI/UX & Architecture Design

Status: **DRAFT — awaiting user approval before implementation.**

## Goals

- Let users pick, download, enable, disable, and delete mods from inside the launcher.
- No mods ship with the launcher.
- Two mod categories: **External App** (Shinra Meter, TCC) and **GPK** (game asset packs).
- External app mods auto-launch with TERA when enabled; reuse already-running instance.
- GPK mods use TMM-compatible install mechanics so users can share packs both ways.

## Non-goals (v1)

- No mod authoring tools. Users get mods; they don't build them here.
- No per-user mod profiles. One set of enabled mods per install.
- No load-order UI. Enabled-first-wins, surfaced as a warning (see "Conflicts").
- No NSFW/age-gated catalog.

## UI/UX

### Navigation

Add a **Mods** entry to the existing hash-based router (`router.js`). New route, new `mods.html`, new `mods.js`. Follows the same pattern as `home`.

### Layout

Two tabs inside the Mods view: **Installed** and **Browse**. Minion / r2modman style. No sidebar, no dashlets, no dual-pane.

```
┌───────────────────────────────────────────────────────────────┐
│  [Installed (5)]   [Browse]            [🔍 search] [⚙ filter] │
├───────────────────────────────────────────────────────────────┤
│                                                               │
│  EXTERNAL APPS (2)                                            │
│  ┌──┐ Shinra Meter      v3.0.0-beta.2   [● Running] [⚙] [⋯]   │
│  │SM│ by neowutran (Classic+ fork)                            │
│  └──┘ Damage meter overlay                                    │
│                                                               │
│  ┌──┐ TCC                v1.4.166       [Stopped]  [⚙] [⋯]    │
│  │TC│ by foglio1024                                           │
│  └──┘ Custom cooldowns overlay                                │
│                                                               │
│  GAME MODS (3)                                                │
│  ┌──┐ HD Minimap Pack   v2.0            [✓ Enabled] [⚙] [⋯]   │
│  │MM│ by Foglio1024                                           │
│  └──┘ Modern minimap UI                                       │
│                                                               │
│  ┌──┐ Unicast           v1.2            [Update →] [⚙] [⋯]    │
│  │UC│ by tera-private-mods                                    │
│  └──┘ Costume / appearance changer      ⚠ 2 conflicts         │
│                                                               │
│  ┌──┐ Cleaner UI Skin   v0.4            [  Enable  ]     [⋯]  │
│  │UI│ by teralove                                             │
│  └──┘ Minimal HUD                                             │
│                                                               │
│  ┌─────────────────────────────────────────────────────────┐  │
│  │ ↓ Downloads (1)   HD Minimap Pack   43%  ━━━━━━     [×] │  │
│  └─────────────────────────────────────────────────────────┘  │
└───────────────────────────────────────────────────────────────┘
```

### Per-row anatomy (~56 px tall)

- 32 px square icon
- Line 1: name + author
- Line 2: one-line description (plus inline warnings like "⚠ 2 conflicts")
- **Primary status/action cell** (right side, 128 px wide) — transforms with state
- Settings gear (per-mod config)
- Overflow `⋯` (uninstall, open folder, view on GitHub)

### Primary button state machine

External apps:
- `Stopped` (idle) — clicking **launches** the app
- `● Running` (green dot) — clicking **stops** the app
- `Install` (not downloaded) — clicking **downloads + installs**
- `Update →` (new version) — clicking **downloads + replaces**
- `Installing… 43%` — inline progress

GPK mods:
- `Enable` (installed, disabled) — clicking **enables** (writes to `CompositePackageMapper.dat`)
- `✓ Enabled` — clicking **disables** (restores backup entries)
- `Install` (catalog / not downloaded) — clicking **downloads + installs**
- `Update →` — clicking **downloads + replaces**
- `Installing… 43%` — inline progress

One button slot, one affordance at a time. Overflow menu holds the rest. CurseForge style.

### Per-mod settings (gear icon)

Opens an inline drawer beneath the row, not a modal.

External apps expose:
- Launch arguments textbox
- "Auto-launch with TERA" toggle (default: on when first enabled)
- "Keep running after TERA exits" toggle (default: off)
- "Launch now" button

GPK mods expose:
- Target composite list (read-only, collapsed by default)
- "Overwrites files from" warning (if conflict)
- "Open mod folder" button

No full config editor — for Shinra/TCC we defer to each app's own settings window.

### Browse tab

Same row layout, same style. Primary button is always `Install`. Fetches a JSON catalog from a URL we control (see Architecture > Catalog). Search + category chip filter (`All / External / GPK / UI / Appearance / ...`). Empty state: "Catalog unavailable — try again later."

### Downloads tray

Persistent bar at bottom of view (Heroic style). Shows active + queued downloads: title, %, bar, cancel. Collapses when idle. Clicking a row in the tray scrolls to that mod.

### Game folder dialog (task #10)

Separate feature but shares state.

- On launcher start, if `[game] path` is empty **or** the stored path's `Binaries/TERA.exe` is missing, a modal blocks everything until the user picks a folder.
- If the picked folder is invalid (no `TERA.exe` under `Binaries/`), show error inline and keep the picker open.
- No default; no hardcoded fallback.

### Running-state feedback (external apps)

- Row status dot pulses green while the app process is alive.
- Sidebar Mods entry shows a `(N)` badge for running app count.
- On `Play`, enabled external apps that are not running get spawned. Already-running ones are left alone.
- Process monitoring via Tauri `tauri::async_runtime::spawn` with a `tokio::process::Child` watch loop.

### Errors

- Download / install / spawn errors → toast (top-right) + persistent red chip on the row. Click chip for a "What went wrong" dialog with fix steps.
- GPK file conflicts → amber banner on the row: "Overwrites 2 files from <other-pack>". "Show details" discloses exact object paths.
- Mapper corruption → full-screen "Launcher repair" dialog with "Restore vanilla" button that copies `CompositePackageMapper.clean` back.

## Architecture

### Rust backend (new modules)

```
teralaunch/src-tauri/src/
├── commands/
│   └── mods.rs                 # new Tauri commands
├── services/
│   ├── mods/
│   │   ├── catalog.rs          # fetch & cache remote catalog JSON
│   │   ├── external_app.rs     # Shinra/TCC download + process management
│   │   ├── gpk.rs              # GPK install/uninstall (TMM-compat)
│   │   ├── composite_mapper.rs # TMM mapper encrypt/decrypt + CRUD
│   │   ├── manifest.rs         # parse GPK trailer (magic 0x9E2A83C1)
│   │   └── registry.rs         # ModList.tmm read/write
│   └── game_folder.rs          # task #10 — set/unset + validation
└── state/
    └── mods_state.rs           # Arc<Mutex<ModsState>>
```

### New Tauri commands

| Command | Purpose |
|---|---|
| `get_mods_catalog` | Fetch remote catalog JSON (cached 24h) |
| `list_installed_mods` | Combined external + GPK list from registry |
| `install_mod` | Download + install (external or GPK) |
| `uninstall_mod` | Delete files + remove from registry |
| `enable_mod` | Enable external app OR turn on GPK (patch mapper) |
| `disable_mod` | Stop external app OR turn off GPK (restore mapper) |
| `launch_external_app` | Spawn Shinra/TCC manually |
| `stop_external_app` | Kill its process |
| `get_mod_conflicts` | Return conflicting mods for given GPK path |
| `open_mods_folder` | Open OS file explorer at mods dir |
| `get_game_folder_state` | Return `{set: bool, valid: bool}` for dialog logic |
| `set_game_folder` | Validate + save path |

### Config.ini extensions

```ini
[game]
path=<user-set, no default>

[mods]
# External app auto-launch toggles
auto_launch_shinra=true
auto_launch_tcc=false
# Enabled GPK mod filenames (comma-separated)
enabled_gpks=hd_minimap.gpk,unicast.gpk
```

GPK enable/disable state is ALSO stored in TMM's `ModList.tmm` so TMM sees our mods (format compat). `config.ini` is the source of truth for external-app settings only.

### Filesystem layout

```
<APPDATA>/TERA Europe/launcher/
├── config.ini
├── mods/
│   ├── external/
│   │   ├── shinra-meter/        # extracted app dir
│   │   └── tcc/
│   └── catalog-cache.json

<GAME_PATH>/S1Game/CookedPC/
├── CompositePackageMapper.dat   # live mapper (patched)
├── CompositePackageMapper.clean # our backup (TMM-compat)
├── ModList.tmm                  # TMM-compat registry
└── <mod>.gpk                    # installed mod GPKs
```

### GPK catalog format

Hosted in a static GitHub repo we control (e.g. `TERA-Europe-Classic/external-mod-catalog`). Single `catalog.json`:

```json
{
  "version": 1,
  "updated_at": "2026-04-18T00:00:00Z",
  "mods": [
    {
      "id": "foglio.modern-minimap",
      "name": "Modern Minimap",
      "author": "Foglio1024",
      "description": "Clean minimap UI",
      "category": "ui",
      "type": "gpk",
      "source_url": "https://github.com/Foglio1024/tera-modern-ui",
      "download_url": "https://github.com/.../releases/download/.../minimap.gpk",
      "sha256": "abc...",
      "size_bytes": 412345,
      "screenshot_url": "https://...",
      "target_patch": "v100.02",
      "composite_flag": true
    },
    {
      "id": "tera-europe-classic.shinra",
      "name": "Shinra Meter (Classic+)",
      "author": "neowutran / TERA Europe Classic",
      "description": "Damage meter overlay",
      "category": "external",
      "type": "external",
      "source_url": "https://github.com/TERA-Europe-Classic/ShinraMeter",
      "download_url": "https://github.com/.../releases/latest/download/ShinraMeter.zip",
      "sha256": "def...",
      "executable_relpath": "ShinraMeter.exe",
      "auto_launch_default": true
    }
  ]
}
```

Seed sources (from research):
- `Foglio1024/tera-modern-ui` — UI packs
- `tera-private-mods/unicast` — appearance
- `teralove/TERA-UI-Mods` — misc UI
- Our own `TERA-Europe-Classic/ShinraMeter` + `TERA-Europe-Classic/TCC` forks

### GPK install mechanics (TMM-compatible)

Reimplement TMM's logic in Rust:

1. Download `.gpk` → verify SHA-256 → copy into `CookedPC/`.
2. Parse metadata trailer: last 4 bytes = `0x9E2A83C1`, walk backwards for header fields.
3. If first install ever, copy `CompositePackageMapper.dat` → `CompositePackageMapper.clean`.
4. Validate all embedded composite paths exist in the live mapper. Fail with "game may have updated" if not.
5. TFC blob extraction: assign free index from `[101..899]`, write `WorldTextures<NNN>.tfc`, rewrite TFC name strings inside the copied GPK in place.
6. Patch live mapper: replace entries keyed by `compositeName` with `{filename: <container>.gpk, offset, size}`. Write `tmm_marker` sentinel.
7. Encrypt mapper: XOR with `"GeneratePackageMapper"`, outside-in pair swap, 16-byte block permutation `{12,6,9,4,3,14,1,10,13,2,7,15,0,8,5,11}`.
8. Append entry to `ModList.tmm`.

Uninstall reverses steps 8 → 6 using `.clean` backup.

### Conflict handling

When enabling a GPK, scan all currently-enabled mods. If any `ObjectPath` matches (case-insensitive, `IncompletePathsEqual`), show the row warning banner. **Do not block.** User chooses: disable the other mod, or accept last-enabled-wins. Divergence from TMM's hard reject — our UX research strongly favors non-blocking.

### External app management

Shinra / TCC run as separate processes. Each app's fork needs to connect to a **local proxy port** exposed by `tera-proxy-server-agnitor` (v100 proxy), not pcap.

- Fork repos under `TERA-Europe-Classic` org:
  - `ShinraMeter` — fresh fork from `neowutran/ShinraMeter@master`, apply ~5 surgical patches (see "Shinra/TCC fork plan" below). Do **not** fork from `LukasTD/ShinraMeter` — that's a fork of michaelcarno with 100+ unrelated UI-rewrite commits.
  - `TCC` — fresh fork from `foglio1024/tera-custom-cooldowns`. TCC already supports toolbox proxy mode (`ToolboxSniffer`); likely just needs config pointing at the right port.
- Each repo publishes versioned zips via GitHub Releases.
- Launcher downloads, extracts under `<APPDATA>/mods/external/<id>/`, runs executable.
- Process lifecycle: `tokio::process::Child`; detect exit, update UI state; on TERA launch, skip spawn if `find_process_by_name(exe)` returns alive.

### Shinra/TCC fork plan

**Same connection mechanism applies to both.** TCC's native toolbox mode is NOT reused — user has not modified TCC before and wants parity with the Shinra EU-Classic approach. The exact mechanism is pending the source-read agent (see "Shinra connection mechanism" section). Once that spec is written, we apply the same socket/handshake logic to TCC.

**Shinra (fresh from `neowutran/master`):**
1. New `DamageMeter.Sniffing/SnifferFactory.cs` — switchable between pcap and proxy mode based on config. Default: proxy.
2. `DamageMeter.Sniffing/TeraSniffer.cs` — add the EU-Classic proxy-mode sniffer (exact mechanism TBD from source-read).
3. `DamageMeter.Core/Processing/C_CHECK_VERSION.cs` — route through abstraction, drop proxy-overhead logging.
4. `resources/data/servers.txt` + opcode map for v100.02 build (TBD — log `message.Versions[0]` on live client to confirm).
5. Hardcode region `"EU-EN"` matches submodule `regions-EU-EN.tsv`.
6. Bump TFM to `net8.0-windows`. Remove `DamageMeter.AutoUpdate` (our launcher handles updates).
7. CI: GitHub Actions workflow building + publishing zip on tag push.

**TCC (fresh from `foglio1024/master`):**
1. New `TeraPacketParser/Sniffing/ClassicPlusSniffer.cs` — implements the same EU-Classic connection mechanism as Shinra's equivalent file. Not `ToolboxSniffer`.
2. Wire the factory in `Sniffing/SnifferFactory.cs` to return `ClassicPlusSniffer` when mode is Classic+.
3. Default `CaptureMode` = new `ClassicPlus` variant (keep existing `Npcap`/`RawSockets`/`Toolbox` available but unused).
4. Ship default `server-overrides.txt` + opcode map for v100.02 build (shared with Shinra).
5. Same CI pattern as Shinra.
6. **Strip every non-read-only code path.** Full enumeration below under "TCC write paths to delete". Delete the files / methods / UI buttons — don't gate them behind flags.

Shared between both forks: the framing/handshake code should live in a small C# helper that we can copy-paste into each repo (or publish as a tiny NuGet under the org) — the mechanism is identical, only the packet-processing pipelines downstream differ.

### Frontend (vanilla JS)

- New route in `router.js`: `mods: { title: "Mods", file: "mods.html", public: false, init: initModsView }`
- New `mods.html`, `mods.js`, styles folded into `modern.css` (or a new `mods.css` imported by `mods.html`).
- No framework. Direct DOM manipulation, `document.createElement`, the same style as `app.js`.
- Event-driven updates via `window.__TAURI__.event.listen(...)` for `mods_changed`, `mod_download_progress`, `external_app_status_changed`.
- Bottom-aligned download tray as a single DOM node that swaps visibility.

### Testing

Rust:
- `commands/mods.rs` — unit tests using `MockHttpClient` + `MockEventEmitter`.
- `services/mods/composite_mapper.rs` — round-trip encrypt/decrypt test with a fixture mapper blob.
- `services/mods/manifest.rs` — GPK trailer parser against fixture mods.
- `services/mods/gpk.rs` — install/uninstall round-trip with a temp game dir.

JS:
- `tests/mods.test.js` — render, toggle, filter, search.
- Playwright: one end-to-end test that installs a fixture external app and verifies state transitions.

## Decisions (resolved)

1. **Catalog hosting** — static JSON hosted in a dedicated GitHub repo `TERA-Europe-Classic/external-mod-catalog`, fetched via `raw.githubusercontent.com`. NOT on the API server.
2. **Shinra proxy connection** — NOT a plain TCP connect to a known port. The EU-Classic branch uses a specific mechanism that must be read out of the source before the Classic+ fork can be specced. Targeted source-read in progress; spec will be appended to this doc under "Shinra connection mechanism" below when complete.
3. **Repo names** under `TERA-Europe-Classic` org — `ShinraMeter` and `TCC` (short names, no `-classicplus` suffix).
4. **GPK row content** — two-letter initial tile is the *icon fallback only*. Each row still shows `name + author + short description` on its face, and clicking the row expands an inline detail panel with: long description, screenshot gallery, version + target patch, file size, "View on GitHub" link, and (for GPK) the list of composite paths the mod overwrites. Detail panel populated from catalog entry fields; for user-imported local GPKs that have no catalog entry, the panel falls back to what the GPK trailer exposes (ModName, ModAuthor, composite list, file size).
5. **Local import** — v1 supports "Add mod from file…" for local `.gpk` files. Mirrors TMM's `Add` button. Imported mods are treated identically to catalog-installed ones except they show "Local mod" in place of the catalog "source" link.
6. **External app uninstall** — uninstall flow detects whether the app has a user settings folder (e.g. `%APPDATA%/ShinraMeter/`). If yes, prompts the user: "Also delete app settings?" with `[Keep] [Delete]`. If no settings folder exists, uninstall proceeds silently.

## Catalog entry schema (revised after Q4 answer)

```json
{
  "id": "foglio.modern-minimap",
  "name": "Modern Minimap",
  "author": "Foglio1024",
  "short_description": "Clean minimap UI",
  "long_description": "Multi-paragraph markdown body…",
  "category": "ui",
  "type": "gpk",
  "source_url": "https://github.com/Foglio1024/tera-modern-ui",
  "license": "MIT",
  "credits": "Originally by Foglio1024. GPK packing tooling by GPK_RePack (lunchduck).",
  "download_url": "https://github.com/.../releases/download/.../minimap.gpk",
  "sha256": "abc…",
  "size_bytes": 412345,
  "version": "2.0",
  "target_patch": "v100.02",
  "icon_url": "https://.../icon.png",
  "screenshots": [
    "https://.../shot1.png",
    "https://.../shot2.png"
  ],
  "composite_flag": true,
  "updated_at": "2026-03-01T00:00:00Z"
}
```

External-app entries drop `composite_flag` / `target_patch` and add `executable_relpath`, `auto_launch_default`, `has_settings_folder` (path template for uninstall prompt).

### Credit / attribution requirements (non-negotiable)

Every catalog entry MUST populate `author` and SHOULD populate
`source_url`, `license`, and `credits`. These three fields are rendered
prominently in the mod detail slide-over panel so users can:

- Trace a mod back to its original author (GitHub repo, forum post, Discord).
- See the redistribution license before installing.
- Discover acknowledgments the fork maintainer owes upstream.

GPK mods forked from community authors MUST credit the original packer
in `credits`. If the license is unknown, set `"license": "Unknown"` —
do not omit the field — so the UI shows an honest "license unknown"
row rather than silently dropping attribution.

Review PRs to `external-mod-catalog` for missing attribution before
merging.

## Shinra / TCC connection mechanism (locked)

Extracted from `LukasTD/ShinraMeter@EU-Classic` source (`TeraSniffer.cs`, `SnifferFactory.cs`, `PacketProcessor.cs`, `C_CHECK_VERSION.cs`, `App.xaml.cs`). This is the full spec for both the Classic+ Shinra fork **and** the Classic+ TCC fork — the sniffer component is identical in both, only downstream packet processing differs.

### Socket

- **Target:** `127.0.0.1:7803` TCP, no TLS. Hardcoded in the default `TeraSniffer` constructor. No env var, no config, no CLI flag, no fallback port. Recompile to change.
- **Direction of first byte:** the client (sniffer) writes **nothing** outbound. The proxy speaks first. The sniffer only reads.
- **Liveness:** detected via `stream.Read` returning 0 or throwing. `TcpClient.Connected` is unreliable and not used.

### Frame format

Wire layout, little-endian:

```
  offset 0   offset 2   offset 3
  +--------+----------+---------- … --+
  | u16    | u8       | payload      |
  | totalLen| direction| (totalLen-1)|
  +--------+----------+---------- … --+
```

- `totalLen` is `payload.Length + 1` (includes the direction byte).
- `direction`: `1` = client→server, `2` = server→client, anything else → frame dropped + warning logged.
- `totalLen == 0` → silently skipped.
- Max frame: `ushort.MaxValue` (65535) bytes total.
- No sentinel / keepalive frames. An idle socket just blocks on read.

### Handshake

**None from the sniffer.** After `ConnectAsync` succeeds, sniffer enters read-only mode. The proxy is responsible for replaying the full TERA session-key exchange (what a pcap would see in-band) as the first framed messages before any game traffic:

1. First `dir=1` frame: 128-byte client session-key chunk.
2. First `dir=2` frame: 128-byte server session-key chunk.
3. Subsequent `dir=1` / `dir=2` frames: normal game packets, still encrypted.

The sniffer constructs `ConnectionDecrypter(region: "EUC")` locally and feeds every frame through it. **Payloads are not pre-decrypted by the proxy.** Classic+ must preserve this: the proxy only reframes, it never decrypts.

### Decryption

Region string is hardcoded `"EUC"` in the `new Tera.Game.Server("Yurian", "EUC", host)` call inside the sniffer. Changing this requires opcode data for a different region key under `resources/data/regions/` and `resources/data/opcodes/`. For Classic+ we stay on `"EUC"` (matches `regions-EU-EN.tsv` opcode naming).

Opcode version is learned in-stream from the first `C_CHECK_VERSION` packet, which triggers `OpcodeDownloader.DownloadIfNotExist(message.Versions[0], ...)`. This is orthogonal to the socket mechanism — works as-is.

### Reconnect

Infinite retry loop. 2000 ms `Task.Delay` between attempts. No exponential backoff, no max-attempts, no give-up. Only cancellation via `Enabled = false` / `CleanupForcefully()` stops it. The proxy must tolerate repeated connect/disconnect cycles, and must restart the key-replay from current stream state on each new connection — the sniffer's `ConnectionDecrypter` is rebuilt fresh per connection.

### Lifecycle

- Sniffer starts unconditionally when `Sniffer.Enabled = true`, which `App.xaml.cs` sets during app startup. No process detection, no button click, no lazy start.
- Shinra is itself started by the tera-toolbox side via `spawn(meterPath, ['--toolbox'], ...)` once TERA is up — the **proxy launches Shinra**, not vice versa. For our launcher this means the launcher plays the toolbox role: we spawn Shinra/TCC when we spawn TERA, and the proxy component is expected to be listening on `7803` by then.

### Unexpected bits to preserve

- `--toolbox` CLI flag is passed by the toolbox integration but ignored by the EU-Classic `App.xaml.cs`. Keep it as a no-op marker for now; we may use it later.
- `Connected` gets set twice — once when the socket opens, again inside `C_CHECK_VERSION`. The second one is what actually enables message decoding. Expect a small window between `OnNewConnection` and first decoded packet.
- Pcap code still compiles in but `SnifferFactory` never returns it. Leave it in place for now — removing is a separate cleanup.

### What the Classic+ proxy component must do

We need a component on the Classic+ side that:

1. Listens on `127.0.0.1:7803` (or whatever we standardise — 7803 keeps the fork changes minimal).
2. Accepts TCP connect without requiring any client-sent handshake.
3. Pushes the TERA session-key exchange as the first two frames (`dir=1` client key, `dir=2` server key).
4. Reframes every subsequent TCP chunk between TERA client and TERA server as `[u16 totalLen LE][u8 dir][payload]`, `totalLen = payloadLen + 1`.
5. Tolerates sniffer disconnect/reconnect every 2 s and re-replays the key exchange each time.
6. Speaks "EUC" region opcodes (or lets `C_CHECK_VERSION` + OpcodeDownloader fetch them at runtime).

**Resolved:** Noctenium (v2) hosts the listener. From the launcher + forks' perspective this is a given — we implement against the Shinra contract (connect, read frames) and trust Noctenium delivers. No launcher-side work to produce the stream.

### Applied to both forks

Shinra and TCC both get the same `TeraSniffer` / `ClassicPlusSniffer` class that implements this mechanism. Extract the shared sniffer to a tiny C# helper library under `TERA-Europe-Classic/tera-classicplus-sniffer` (NuGet-packaged or git-submoduled into both forks) so bug fixes land in one place.

## Proposed build order

Phase A (game folder + foundation):
1. Task #10 — game folder set/unset dialog + validation
2. `commands/mods.rs` skeleton + `mods_state.rs` + config.ini `[mods]` extension
3. Frontend Mods route stub + empty Installed tab

Phase B (external apps):
4. `services/mods/external_app.rs` — download/extract/spawn/monitor
5. Fork `ShinraMeter` + `TCC` under the org, cut first releases
6. Wire external rows: install, enable (= auto-launch toggle), launch-now, stop
7. Splice into `handle_launch_game` for auto-launch-with-game

Phase C (GPK):
8. `services/mods/composite_mapper.rs` — encrypt/decrypt + CRUD
9. `services/mods/manifest.rs` — GPK trailer parser
10. `services/mods/gpk.rs` — install/uninstall/enable/disable
11. Wire GPK rows + conflict warnings + mapper-update recovery

Phase D (catalog + polish):
12. Build catalog repo, seed from Foglio1024 / tera-private-mods / teralove
13. Browse tab + catalog fetch + cache
14. Downloads tray
15. Error surfaces + toasts
16. Tests (Rust + Vitest + 1 Playwright)

Phase B alone gets Shinra + TCC working end-to-end and is useful even if C/D slip.

## TCC write paths to delete (Classic+ fork)

Full enumeration from source audit of `foglio1024/tera-custom-cooldowns@master`.

### Primary chokepoint

`TCC.Interop/Proxy/StubClient.cs` — every outbound RPC routes through this class. Delete the class and its `StubInterface` wrapper. All call sites elsewhere become compile errors that point directly at the feature code to remove.

### Features (and their files) that die with the chokepoint

LFG system (entire feature, chat integrations, widgets):
- `TCC.Core/ViewModels/LfgListViewModel.cs`
- `TCC.Core/UI/Windows/LfgListWindow.xaml.cs`
- `TCC.Core/UI/Controls/Chat/LfgBody.xaml.cs`
- `TCC.Core/Data/ApplyCommand.cs`
- LFG dashboard tiles and menu entries

Group window write actions (keep read-only party display):
- In `TCC.Core/ViewModels/Widgets/GroupWindowViewModel.cs`, delete `ResetInstance`, `DisbandGroup`, `LeaveGroup`
- In the group-window XAML, remove buttons that invoked those commands

Player context menu write actions:
- In `TCC.Core/ViewModels/PlayerMenuViewModel.cs`, delete `InspectCommand`, `AddRemoveFriendCommand`, `BlockUnblockCommand`, `GroupInviteCommand`, `GrantInviteCommand`, `DelegateLeaderCommand`, `GroupKickCommand`, `GuildInviteCommand`, `AskInteractive`, `WhisperCommand`
- Remove corresponding menu items from the player-menu XAML

Broker chat interactive buttons:
- `TCC.Core/UI/Controls/Chat/BrokerOfferBody.xaml.cs::Accept` / `Decline` — delete, leave display-only

Clickable chat hyperlinks:
- `TCC.Core/Data/Chat/ActionMessagePiece.cs::ClickCommand` — strip to no-op. Hyperlink rendering stays; clicking does nothing rather than re-injecting packets.

Extended tooltip / non-DB item info:
- Remove callers of `RequestExTooltip` / `RequestNonDbItemInfo`. Tooltip widget falls back to its cached DB lookup only.

Slash-command bridge:
- `StubClient.InvokeCommand` users — delete. TCC slash-commands that shell to toolbox are dead weight in our setup.

### Kill Toolbox sniffer path entirely

- Force `TeraPacketParser/Sniffing/SnifferFactory.cs` to always return the read-only `ClassicPlusSniffer` (same mechanism as Shinra's `TeraSniffer` — connect to `127.0.0.1:7803` and parse the `[u16 len][u8 dir][payload]` frames).
- Delete `TeraPacketParser/Sniffing/ToolboxSniffer.cs` along with `ToolboxHttpClient.cs` — they're the only callers of `addHooks` / `removeHooks` / `dumpMapSync` / `getServerInfo` / `getReleaseVersion` / `getLanguage` / `getProtocolVersion`.
- Remove `Toolbox` from `CaptureMode` enum, or replace the enum with a single-variant constant.

### Third-party network I/O (not strictly required for read-only, but unwanted on a private server)

- `TCC.Interop/Cloud.cs` — telemetry POST to `https://foglio.ns0.it/tcc/api/usage-stats/post`. Delete the class and its wire-up.
- `TCC.Interop/Firebase.cs` — webhook register/fire to `cloudfunctions.net`. Delete.
- `TCC.Interop/Moongourd/*` — moongourd parse lookups (outbound HTTP GETs). Delete the folder.
- `TCC.Core/UI/FocusManager.cs::SendString` — `InputInjector.PasteString` synthesizes Win32 keystrokes into the TERA window, used by whisper/"ask" menu items to auto-fill chat and produce `C_CHAT` when the user presses Enter. Stub the method to no-op after the player-menu commands are removed.
- `TCC.Core/Update/OpcodeDownloader.cs` — GETs from `raw.githubusercontent.com/tera-toolbox/...` and `neowutran/TeraDpsMeterData`. Repoint to our `TERA-Europe-Classic/external-mod-catalog` or ship opcode `.map` files in-tree. Current Shinra-EU-Classic mirrors ship opcodes in `resources/data/opcodes/` — same approach.

### What survives after the cuts (features to keep)

Class cooldown tracker, abnormalities/buff timers, party-member HP/MP mirror, boss HP tracker, chat display (read-only from `S_CHAT`/`S_WHISPER`), flight-energy tracker, dungeon cooldown dashboard, notification sounds, all on-screen overlays.

### Opcode shipping

TCC only parses `C_*` opcodes from sniffed client→server traffic (never serializes them). The `MessageFactory.OpcodesList` explicitly excludes `C_CHECK_VERSION`, `C_LOGIN_ARBITER`, `C_PLAYER_LOCATION`, `C_PLAYER_FLYING_LOCATION` from the emitted opcodes list — confirming read-only use. The Classic+ fork ships the v100.02 `protocol.<version>.map` in-tree and drops the OpcodeDownloader.

## Task #10 — Game folder set/unset + validation (implemented)

Backend:
- `teralaunch/src-tauri/src/services/config_service.rs` → `tera_config.ini` already shipped with `path=` blank. No default path.
- `teralaunch/src-tauri/src/commands/config.rs::save_game_path_to_config` now calls `game_service::validate_game_installation` (exists + is directory + `Binaries/TERA.exe` present) instead of the weaker `config_service::validate_game_path` (dir exists only). Rejects folders that don't contain TERA.exe with a specific error.
- New command `get_game_folder_state` → `{ set, valid, path, error }` is the single source of truth the frontend polls.

Frontend:
- New `App.ensureGameFolderValid()` dispatches on `get_game_folder_state`:
  - `!set` → shows existing first-launch welcome modal (language + folder picker).
  - `set && !valid` → forces `openGameDirectoryDialog({required:true, errorMessage})` — a non-dismissable variant with a red banner showing the backend's specific error (e.g. "TERA.exe not found in Binaries folder").
- `openGameDirectoryDialog` accepts `{currentPath, required, errorMessage}`; `closeGameDirectoryDialog` no-ops when `required=true`; `saveGameDirectory` detects the required state and resumes launcher init (`completeFirstLaunch` or `initializeAndCheckUpdates`) after a valid save.
- Wired into both startup paths (`App.init` and `initializeHomePage`) — replaces the `isFirstLaunch`-only gate so a user whose game folder moves/breaks after first launch also gets re-prompted.

Tests: all 667 Rust unit tests + 417 frontend Vitest tests pass (the single flaky `state::download_state::tests::test_hash_cache_lock` is a pre-existing parallel-globals race documented in memory-sessions/lessons.md; passes in isolation).
