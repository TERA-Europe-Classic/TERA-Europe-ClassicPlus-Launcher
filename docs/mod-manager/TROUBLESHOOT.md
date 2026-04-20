# Mod Manager Troubleshooting

This doc maps the 10 most common mod-manager errors a user can hit. Each
section names the exact error text the launcher surfaces (so you can
`Ctrl-F` yours), explains what's going on, and gives the fix.

The error strings below are the production templates in
`teralaunch/src-tauri/src/services/mods/`. A CI grep gate (see
`scripts/check-troubleshoot-coverage.mjs`) fails if any production
template is not referenced here.

---

## 1. Download hash mismatch

**Error text:** `Download hash mismatch: expected <...>, got <...>`

**What's going on.** The launcher downloaded the file but the bytes
don't hash to the value the catalog (or signed baseline) promised. The
file is rejected before any byte touches disk.

**Fix.**
- Retry. A stray bit-flip over a flaky connection causes this
  occasionally.
- If it keeps happening: the catalog entry is stale or the mirror is
  serving a different build. File a bug with the mod id and the two
  hash values.

---

## 2. Failed to fetch catalog / Failed to download from…

**Error text:**
- `Failed to fetch catalog: <...>`
- `Catalog fetch returned HTTP <status>`
- `Failed to read catalog body: <...>`
- `Failed to download from <url>: <...>`
- `Download returned HTTP <status> from <url>`
- `Download stream failed: <...>`
- `Failed to build HTTP client: <...>`

**What's going on.** A network operation failed before the launcher
could verify anything. Could be DNS, TLS, a router reset, or the mirror
being down.

**Fix.**
- Check your connection and retry.
- If the catalog URL is consistently unreachable, check the project
  status page — the mirror may be in maintenance.
- Behind a corporate proxy? The launcher's HTTPS scope is locked to
  the known mirrors; ask IT to allow traffic to the
  `tera-europe.net` / `tera-germany.de` / `raw.githubusercontent.com`
  families.

---

## 3. Catalog JSON is malformed

**Error text:**
- `Catalog JSON is malformed: <...>`
- `Catalog JSON envelope is malformed: <...>`

**What's going on.** The catalog response parsed over HTTP but the
structure doesn't match what the launcher expects. Usually means a
bad mirror is serving a stale schema, or the response was intercepted.

The `envelope is malformed` variant specifically points at the
top-level shape — `version` / `updated_at` / `mods` — and is fatal
because individual-entry tolerance kicks in one level below. If the
envelope itself is broken, there's nothing to recover from. Individual
bad entries are dropped silently at WARN level without surfacing this
error to you.

**Fix.**
- Retry after a minute (transient CDN hiccup).
- If persistent: the schema changed and your launcher is older than
  the catalog. Update the launcher.

---

## 4. Mod container filename is unsafe — refusing to deploy

**Error text:**
- `Mod container filename '<...>' is unsafe — refusing to deploy (would escape CookedPC).`
- `Refusing to uninstall: container filename '<...>' is unsafe — would escape CookedPC.`

**What's going on.** The `.gpk` file you're trying to install has a
container name that contains `..`, a drive-letter, a path separator, or
other characters that would let it write outside the game's
`CookedPC/` directory. This is a security sandbox; the install is
aborted before anything touches disk.

**Fix.**
- Don't install the file. Report the source — it's either malformed
  or hostile.
- If you trust the source and the name is benign (rare), repair the
  embedded GPK metadata with a compatible packaging tool and try again.

---

## 5. Zip-slip / zip-archive errors

**Error text:**
- `Zip entry '<...>' escapes the archive root (zip-slip rejected)`
- `Invalid zip archive: <...>`
- `Failed to read zip entry <n>: <...>`

**What's going on.** A downloaded external-app archive had an entry
whose path tried to escape the install directory (`../evil`, absolute
paths, drive-letter), or the archive is corrupted. Either way the
extractor refuses and leaves the install dir empty.

**Fix.**
- For catalog installs: retry once — download corruption is possible.
- If persistent: the upstream release is malformed. File a bug
  with the mod id and the archive URL.

---

## 6. Composite entry for '...' not found in mapper / game version mismatch

**Error text:** `Composite entry for '<object_path>' not found in mapper. Your game version may not match the mod.`

**What's going on.** The GPK is meant for a different game build than
the one you have. Metadata-driven GPKs patch specific `(composite, object)`
slots — if your mapper doesn't contain the slot the mod expects, the
mod is silently incompatible.

**Fix.**
- Check the mod's target game version and reinstall your client to
  match, OR find a build of the mod compatible with your current
  game version.
- Don't bypass this error. Force-installing will not make the mod work.

---

## 7. Failed to back up mapper / Failed to read backup

**Error text:**
- `Failed to back up mapper: <...>`
- `Failed to read backup: <...>`
- `CompositePackageMapper.dat not found at <path>`
- `No CompositePackageMapper.clean backup on disk — can't restore vanilla entries. Verify game files.`

**What's going on.** The launcher creates a `.clean` copy of your
vanilla `CompositePackageMapper.dat` before the first install, and
reads it back when you uninstall. Permission-denied errors here usually
mean the game was installed to `C:\Program Files\` and the launcher
isn't running with write access.

**Fix.**
- Close the game client.
- Close any other mod tool that may be holding the mapper file open.
- Run the launcher as Administrator once to create the `.clean` file.
  Subsequent runs don't need elevated privileges.
- If nothing else works, reinstall the game into a user-writable
  directory (like `C:\Games\TERA\`).

---

## 8. Mapper read/write / CookedPC copy errors

**Error text:**
- `Failed to read mapper: <...>`
- `Failed to write mapper: <...>`
- `Failed to create CookedPC dir: <...>`
- `Failed to copy mod into CookedPC: <...>`
- `Failed to remove mod gpk: <...>`

**What's going on.** The launcher needs to read, rewrite, or delete
files under `<game>/S1Game/CookedPC/`. Something else is holding those
files (running game, open editor) or permissions are denied.

**Fix.**
- Close the game.
- Run the launcher as Administrator if the game is in a protected
  directory.
- Check that your antivirus hasn't quarantined the mapper file.

---

## 9. Failed to read mod file / imported .gpk rejected

**Error text:**
- `Failed to read mod file: <...>`
- `Imported file has no deployable override metadata and no usable target filename.`
- `Mod file has no usable package name in its UE3 header or filename — can't map it to a game file.`
- `Mod file declares no composite packages to override.`
- `Mod file has a composite package with no object path — can't be installed.`
- `Mod file is too small to contain metadata`
- `Unexpected EOF while reading mod footer`
- `Mod footer references offsets past EOF`
- `Composite package offset past EOF`
- `string header past EOF`
- `ANSI string past EOF`
- `UTF-16 string past EOF`
- `Malformed string at offset <n>` / `Malformed footer size at offset <n>` / `Malformed UTF-16 at offset <n>`

**What's going on.** You tried to add a `.gpk` that doesn't carry usable
override metadata, or the file is truncated.

**Fix.**
- Verify you're importing the actual mod file, not a vanilla
  `S1Data.gpk` or an uncooked intermediate.
- Re-download from the source — a truncated download will trip this.

---

## 10. Registry / filesystem errors

**Error text:**
- `Mod registry at <path> is corrupted: <...>`
- `Failed to read mod registry at <path>: <...>`
- `Failed to write registry tmp <path>: <...>`
- `Failed to rename registry <path>: <...>`
- `Failed to create mods dir <path>: <...>`
- `Failed to serialize registry: <...>`
- `Failed to create catalog cache dir: <...>`
- `Failed to serialize catalog cache: <...>`
- `Failed to write catalog cache: <...>`
- `Failed to commit catalog cache: <...>`
- `Failed to clear <dir>: <...>`
- `Failed to create <path>: <...>`
- `Failed to write <path>: <...>`
- `Failed to spawn <path>: <...>`
- `Executable not found: <path>`
- `ShellExecuteEx failed for <path>: Win32 error <code>` (Windows-specific)
- `Catalog executable_relpath '<path>' escapes install dir`
- `Failed to create mods dir <path>: ...` (from `get_gpk_dir` failures)
- `Failed to copy to <path>: <...>` (from Add-mod-from-file)
- `Failed to read <path>: <...>` (from Add-mod-from-file)
- `Could not resolve GPK mods dir`
- `Could not resolve external apps dir`

**What's going on.** One of the launcher's own state files under
`%APPDATA%/Crazy-eSports-ClassicPlus/mods/` couldn't be read, written,
or moved.

**Fix.**
- Make sure your disk isn't full.
- Check that antivirus isn't blocking the launcher's appdata dir.
- If `registry.json` is corrupted: rename it to `registry.json.broken`
  and the launcher will start with an empty registry. You'll need to
  re-install your mods but no bytes are lost — the source `.gpk` files
  remain under `mods/gpk/`.

---

## See also

- `docs/mod-manager/ARCHITECTURE.md` — subsystem overview.
- `docs/CHANGELOG.md` — user-visible release notes.
- Logs: `%APPDATA%/Crazy-eSports-ClassicPlus/logs/` (launcher) and
  `~/.cache/teralauncher/` (tauri devtools, if enabled).
