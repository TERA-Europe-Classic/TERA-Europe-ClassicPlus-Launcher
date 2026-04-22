# TCC tray + mods search runtime checklist

Audit date: 2026-04-22

Purpose: make the final manual/runtime pass for the TCC tray fix and the mods
search shortcut behavior repeatable.

---

## Current verified state

### TCC AppData deployment

- AppData bundle path:
  - `C:\Users\Lukas\AppData\Roaming\Crazy-eSports-ClassicPlus\mods\external\classicplus.tcc`
- Updated local build artifact:
  - `TCC.Core\bin\Debug\TCC.dll`
- Updated AppData `TCC.dll` was successfully copied after clearing the process
  lock and now matches the new build timestamp/size.
- AppData `TCC.exe --toolbox` still launches and stays alive in smoke runs after
  the DLL swap.

### Search shortcut verification

- A direct browser-engine harness using the real `mods.css` search input styling
  confirmed:
  - `Ctrl+A` selects the full input value,
  - `Ctrl+C` copies,
  - `Backspace` clears the selected text,
  - `Ctrl+V` restores the copied value.

This is already sufficient to treat the search input shortcut behavior as
verified from the browser side.

---

## Remaining manual checks

### A. TCC tray icon visual verification

1. Ensure no stale `TCC.exe` remains running.
2. Launch the deployed AppData executable:
   - `C:\Users\Lukas\AppData\Roaming\Crazy-eSports-ClassicPlus\mods\external\classicplus.tcc\TCC.exe --toolbox`
3. Confirm:
   - tray icon appears,
   - double-click opens Settings / Dashboard path,
   - right-click menu opens and shows Dashboard / Settings / Close.
4. Restart Explorer / force a taskbar recreation event if practical.
5. Confirm the tray icon reappears without needing a second manual restart.

### B. Duplicate-launch behavior sanity check

1. With one `TCC.exe --toolbox` already running, start the same executable again.
2. Confirm:
   - no crash,
   - no duplicate visible TCC process pile-up,
   - existing process remains the single active instance.

### C. Launcher-integrated path

1. Use the launcher-installed external mod entry for `classicplus.tcc`.
2. Launch the game path that auto-starts external apps.
3. Confirm TCC behaves the same from the launcher path as from direct AppData
   launch.

---

## Pass criteria

The TCC tray fix can be considered visually/runtime verified when all of the
following are true:

1. AppData `TCC.exe --toolbox` launches without crash.
2. Tray icon is visible on first launch.
3. Tray icon survives or is recreated after taskbar recreation.
4. Duplicate launch does not create a second active TCC instance.
5. Launcher-driven auto-launch path behaves the same as direct AppData launch.

---

## Current blocker

The remaining gap is **visual/manual confirmation of the tray icon itself**.
The code path, build, AppData deployment, and process-liveness smoke checks are
done; the tray is the last human-visible runtime proof needed before release.
