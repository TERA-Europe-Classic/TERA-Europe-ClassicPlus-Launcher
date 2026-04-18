# Shinra / TCC Fork Setup — `TERA-Europe-Classic` org

Step-by-step playbook for creating the two external-app forks the launcher
depends on. All patches are derived from the source-audits in `DESIGN.md`.

## 0. Prereqs

- GitHub admin access to the `TERA-Europe-Classic` org.
- `gh` CLI authenticated (`gh auth status` returns `Logged in to github.com`).
- .NET 8 SDK installed locally for build verification.

## 1. Shinra — `TERA-Europe-Classic/ShinraMeter`

### 1a. Create the repo

Do **not** fork `LukasTD/ShinraMeter` — it's rooted in `michaelcarno/ShinraMeter`
and carries ~100 commits of unrelated UI-rewrite noise. Fork clean from
`neowutran/ShinraMeter@master`:

```bash
# Upstream to fork from is pinned by the "Identify correct Shinra upstream"
# research agent — see DESIGN.md. If it's michaelcarno/ShinraMeter, swap the
# source URL accordingly; the rest of the flow is identical.
gh repo fork <UPSTREAM>/ShinraMeter \
  --org TERA-Europe-Classic \
  --fork-name ShinraMeter \
  --clone=true
cd ShinraMeter
git remote add upstream https://github.com/<UPSTREAM>/ShinraMeter.git
# No separate classicplus branch — this repo exists for Classic+, main IS the
# Classic+ branch. Commit patches straight to main.
```

### 1b. Apply the Classic+ patches

The five surgical files — exact scope per `DESIGN.md § Shinra/TCC fork plan`:

1. **`DamageMeter.Sniffing/ClassicPlusSniffer.cs`** (new) — port of the
   `UnencryptedSocketLoopAsync` logic from `LukasTD/ShinraMeter@EU-Classic`:
   - Connect `TcpClient` to `127.0.0.1:7803`.
   - Read loop: `[u16 totalLenLE][u8 dir][payload]`, `totalLen` includes the
     dir byte. `dir=1` → `_decrypter.ClientToServer(payload, 0)`,
     `dir=2` → `_decrypter.ServerToClient(payload, 0)`.
   - Infinite 2s retry loop.
   - Construct `new Tera.Game.Server("Yurian", "EUC", host)` locally —
     Shinra assumes EUC, Noctenium must serve EUC-compatible opcodes.
   - **Do NOT send anything outbound** — Noctenium speaks first.

2. **`DamageMeter.Sniffing/SnifferFactory.cs`** (new) — single entry:
   ```csharp
   public static ITeraSniffer Create() => new ClassicPlusSniffer();
   ```
   All callers of `TeraSniffer.Instance` get rewired to this factory
   (see call sites in `DamageMeter.Core/PacketProcessor.cs`).

3. **`DamageMeter.Core/Processing/C_CHECK_VERSION.cs`** — route `Connected`
   write through the factory abstraction, drop `ClientProxyOverhead` /
   `ServerProxyOverhead` logging (the mirror socket has no proxy overhead).

4. **`resources/data/opcodes/protocol.<v100-build>.map`** — opcode map for
   v100.02 EU. Build number TBD from live client — log `message.Versions[0]`
   inside the first `C_CHECK_VERSION` callback to harvest the right number.
   LukasTD's branch added `protocol.281908` and `protocol.286406` for Classic
   (32-bit); v100.02 is a different build.

5. **`resources/data/servers.txt`** — add the Classic+ server entry so the
   (unused) pcap path still catalogues it, and for any logs that display
   the server name.

### 1c. Remove upstream pieces we don't need

- `DamageMeter.AutoUpdate/` — our launcher handles updates. Unreference
  from the `Tera.sln` and delete.
- `DamageMeter.UI/Resources/data/` submodule — repoint to a pin we control
  (e.g., our own fork of `TeraDpsMeterData`) or leave at the current
  `neowutran/TeraDpsMeterData` commit and accept the stale drift.

### 1d. Bump TFM + CI

- Change `TargetFrameworks` from `netcoreapp3.1;net471` to `net8.0-windows`
  across all `.csproj` files.
- Port LukasTD's `.github/workflows/build-and-release.yml` (it's the only
  thing worth copying from their branch). Trigger on tag push
  (`v*.*.*`). Publish a single zip `ShinraMeter-<tag>.zip` as a GitHub
  Release asset.

### 1e. First release

```bash
dotnet build DamageMeter.UI/DamageMeter.UI.csproj -c Release
# verify ShinraMeter.exe runs, connects to 127.0.0.1:7803, and retries when
# the port is unreachable.
git tag v3.0.0
git push origin main --tags
```

The CI publishes the zip. Grab its `sha256` and the release URL — these go
into the catalog entry (see §3).

## 2. TCC — `TERA-Europe-Classic/TCC`

### 2a. Create the repo

```bash
gh repo fork foglio1024/tera-custom-cooldowns \
  --org TERA-Europe-Classic \
  --fork-name TCC \
  --clone=true
cd TCC
# No classicplus branch — patches land on main.
```

### 2b. Apply the sniffer patch

Same connection mechanism as Shinra (see `DESIGN.md § Shinra / TCC
connection mechanism (locked)`). Copy the `ClassicPlusSniffer.cs` from the
Shinra fork into `TeraPacketParser/Sniffing/ClassicPlusSniffer.cs` with
minor adjustments: TCC's `ITeraSniffer` interface is slightly different
(event names match `TeraSniffer`, but the constructor takes a `Server`
and a `CaptureMode`).

Wire the factory at `TeraPacketParser/Sniffing/SnifferFactory.cs` to always
return the new sniffer. Add a `CaptureMode.ClassicPlus` variant, make it
the default in `App.Settings`, and keep the existing variants inert.

### 2c. Strip every write path

Full list in `DESIGN.md § TCC write paths to delete`. Summary:

- Delete `TCC.Interop/Proxy/StubClient.cs` and `StubInterface.cs`.
- Delete `TeraPacketParser/Sniffing/ToolboxSniffer.cs` + `ToolboxHttpClient.cs`.
- Delete `TCC.Interop/Cloud.cs`, `TCC.Interop/Firebase.cs`, `TCC.Interop/Moongourd/*`.
- In `TCC.Core/UI/FocusManager.cs`, stub `SendString` to no-op.
- In `TCC.Core/Update/OpcodeDownloader.cs`, disable the remote fetch; ship
  the v100.02 opcode maps in `TCC.Core/Resources/data/opcodes/`.
- Delete feature files that depend on `StubClient`:
  - `TCC.Core/ViewModels/LfgListViewModel.cs`
  - `TCC.Core/UI/Windows/LfgListWindow.xaml(.cs)`
  - `TCC.Core/UI/Controls/Chat/LfgBody.xaml(.cs)`
  - `TCC.Core/Data/ApplyCommand.cs`
- In `TCC.Core/ViewModels/Widgets/GroupWindowViewModel.cs`, remove
  `ResetInstance`, `DisbandGroup`, `LeaveGroup` + their XAML buttons.
- In `TCC.Core/ViewModels/PlayerMenuViewModel.cs`, remove every command
  that writes (see `DESIGN.md` for the exhaustive list).
- In `TCC.Core/UI/Controls/Chat/BrokerOfferBody.xaml(.cs)`, remove the
  Accept/Decline buttons.
- In `TCC.Core/Data/Chat/ActionMessagePiece.cs`, stub `ClickCommand`.

### 2d. Bump + CI

- Keep `net8-windows` (already the upstream target).
- Port the workflow from Shinra. Publish `TCC-<tag>.zip` on tag push.

### 2e. First release

Same pattern as Shinra. Verify: TCC launches, shows its overlay,
abnormality/cooldown widgets render from observed traffic, **no outbound
packets** (use wireshark or procmon on the TCC process to confirm zero TCP
writes except ARP/DNS).

## 3. Catalog repo — `TERA-Europe-Classic/external-mod-catalog`

```bash
gh repo create TERA-Europe-Classic/external-mod-catalog --public --clone
cd external-mod-catalog
```

Create `catalog.json`:

```json
{
  "version": 1,
  "updated_at": "<ISO-8601>",
  "mods": [
    {
      "id": "tera-europe-classic.shinra",
      "kind": "external",
      "name": "Shinra Meter",
      "author": "neowutran (Classic+ fork)",
      "short_description": "Damage meter overlay",
      "long_description": "Real-time DPS + combat log overlay ...",
      "category": "overlay",
      "version": "3.0.0",
      "download_url": "https://github.com/TERA-Europe-Classic/ShinraMeter/releases/download/v3.0.0/ShinraMeter-v3.0.0.zip",
      "sha256": "<fill after release>",
      "size_bytes": 0,
      "source_url": "https://github.com/TERA-Europe-Classic/ShinraMeter",
      "executable_relpath": "ShinraMeter.exe",
      "auto_launch_default": true,
      "settings_folder": "%APPDATA%/ShinraMeter",
      "updated_at": "<ISO-8601>"
    },
    {
      "id": "tera-europe-classic.tcc",
      "kind": "external",
      "name": "TCC",
      "author": "foglio1024 (Classic+ read-only fork)",
      "short_description": "Cooldowns / abnormalities overlay",
      "long_description": "Class cooldowns, buff timers, party HP/MP mirror ...",
      "category": "overlay",
      "version": "1.4.166",
      "download_url": "https://github.com/TERA-Europe-Classic/TCC/releases/download/v1.4.166/TCC-v1.4.166.zip",
      "sha256": "<fill after release>",
      "size_bytes": 0,
      "source_url": "https://github.com/TERA-Europe-Classic/TCC",
      "executable_relpath": "TCC.Loader.exe",
      "auto_launch_default": false,
      "settings_folder": "%APPDATA%/TCC",
      "updated_at": "<ISO-8601>"
    }
  ]
}
```

Commit to `main`. The launcher fetches this via
`https://raw.githubusercontent.com/TERA-Europe-Classic/external-mod-catalog/main/catalog.json`
(hardcoded in `teralaunch/src-tauri/src/services/mods/catalog.rs::CATALOG_URL`).

## 4. Verification checklist

Before merging Phase B to the launcher's main branch:

- [ ] `ShinraMeter.exe` launches, retries `127.0.0.1:7803` every 2s when
      Noctenium isn't running, logs no outbound writes in wireshark.
- [ ] `TCC.Loader.exe` same behaviour. Confirm LFG window is gone, group
      window has no Leave/Disband/Reset buttons, player context menu is
      display-only.
- [ ] Catalog JSON is fetched successfully from raw.githubusercontent.com
      (test via `curl` and via the launcher's Browse tab).
- [ ] Installing Shinra from Browse creates
      `<APPDATA>/TERA Europe/launcher/mods/external/tera-europe-classic.shinra/`
      with `ShinraMeter.exe` at the root.
- [ ] Enabling Shinra with `auto_launch=true`, then clicking Play in the
      launcher, spawns both `TERA.exe` and `ShinraMeter.exe`.
- [ ] Uninstalling Shinra terminates its process and wipes the install dir.

## 5. Future work (not required for Phase B)

- Mirror opcode maps under `TERA-Europe-Classic/tera-data` so both forks
  can depend on our copy instead of `neowutran/TeraDpsMeterData`.
- Publish a tiny shared NuGet `TeraEuropeClassic.Sniffer` so the
  `ClassicPlusSniffer.cs` lives in one place and both repos reference it.
