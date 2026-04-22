## Flight Gauge v100 review

### Inputs reviewed

- current vanilla package:
  - `D:\Elinu\S1Game\CookedPC\Art_Data\Packages\S1UI\S1UI_ProgressBar.gpk`
- catalog mod package:
  - `https://raw.githubusercontent.com/foglio1024/UI-Remover/master/remove_FlightGauge/S1UI_ProgressBar.gpk`

### Tooling used

- local prebuilt `GPK_RePack.Core.dll` from `C:\Users\Lukas\Documents\GitHub\elinu\scripts`
- local helper `dump_all_exports_detail.exe`

### Export surface summary

Vanilla export classes:

- `ProgressBar` → `Core.GFxMovieInfo`
- `ProgressBar_I14` → `Core.ObjectRedirector`
- `ProgressBar_I16` → `Core.ObjectRedirector`
- `ProgressBar_I1B` → `Core.ObjectRedirector`
- `ProgressBar_I22` → `Core.ObjectRedirector`
- `ProgressBar_I25` → `Core.ObjectRedirector`
- `ProgressBar_I3` → `Core.ObjectRedirector`
- `ProgressBar_I36` → `Core.ObjectRedirector`
- `ProgressBar_I3A` → `Core.ObjectRedirector`
- `ProgressBar_I3E` → `Core.ObjectRedirector`
- `ProgressBar_I45` → `Core.ObjectRedirector`
- `ProgressBar_I5` → `Core.ObjectRedirector`
- `ProgressBar_I8` → `Core.ObjectRedirector`
- `ProgressBar_IC` → `Core.ObjectRedirector`
- `ProgressBar_IF` → `Core.ObjectRedirector`

Modded export classes:

- `ProgressBar` → `Core.GFxMovieInfo`
- `ProgressBar_I14` → `Core.Texture2D`
- `ProgressBar_I16` → `Core.Texture2D`
- `ProgressBar_I1B` → `Core.Texture2D`
- `ProgressBar_I22` → `Core.Texture2D`
- `ProgressBar_I25` → `Core.Texture2D`
- `ProgressBar_I3` → `Core.Texture2D`
- `ProgressBar_I36` → `Core.Texture2D`
- `ProgressBar_I3A` → `Core.Texture2D`
- `ProgressBar_I3E` → `Core.Texture2D`
- `ProgressBar_I45` → `Core.Texture2D`
- `ProgressBar_I5` → `Core.Texture2D`
- `ProgressBar_I8` → `Core.Texture2D`
- `ProgressBar_IC` → `Core.Texture2D`
- `ProgressBar_IF` → `Core.Texture2D`

Import surface summary:

- vanilla imports: 32
- modded imports: 4

### Conclusion

This mod is **not** a safe v1 `replace_export_payload` candidate.

Why:

1. The changed exports are not the same class between current vanilla and modded package.
2. Current vanilla uses `ObjectRedirector` exports for the `ProgressBar_I*` entries.
3. The modded file replaces those with real `Texture2D` exports.
4. The import table shape also changes materially (`32` → `4`).

That means the mod's intent is likely “replace the gauge textures / collapse the gauge visuals,” but expressing it safely against the current client requires more than payload replacement:

- export-class change support
- import/name table mutation support
- or a higher-level texture/redirector-aware semantic patch step

### Status decision

- `blocked-unsupported-export-shape`

### What would unblock it

One of:

1. extend patch manifests beyond payload replacement so they can safely rebind class/import/name metadata for reviewed exports, or
2. implement a texture-focused conversion path that reconstructs the redirector/texture relationship against the current vanilla package instead of replaying the old package literally.
