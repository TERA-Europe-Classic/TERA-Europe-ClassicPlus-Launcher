## Community Window v100 review

### Inputs reviewed

- current vanilla package:
  - `D:\Elinu\S1Game\CookedPC\Art_Data\Packages\S1UI\S1UI_CommunityWindow.gpk`
- catalog mod package:
  - `https://raw.githubusercontent.com/foglio1024/tera-restyle/master/CommunityWindow/p90/S1UI_CommunityWindow.gpk`

### Tooling used

- local prebuilt `GPK_RePack.Core.dll` from `C:\Users\Lukas\Documents\GitHub\elinu\scripts`
- local helper `dump_all_exports_detail.exe`

### Export/import surface summary

Vanilla package:

- exports: `63`
- imports: `132`
- export mix: one `GFxMovieInfo` + many `ObjectRedirector`

Modded package:

- exports: `64`
- imports: `6`
- export mix: one `GFxMovieInfo` + broad `Texture2D` replacement surface

### Conclusion

This mod is **not** a safe v1 `replace_export_payload` candidate.

Why:

1. The export table shape changes (`63` → `64`).
2. The import table shape changes materially (`132` → `6`).
3. Most redirector exports become real `Texture2D` exports in the modded file.
4. Current v1 manifests cannot represent reviewed export creation/class-change/import-table mutation safely.

### Status decision

- `blocked-unsupported-export-shape`

### What would unblock it

One of:

1. reviewed support for export creation/deletion plus import/name table editing, or
2. a higher-level semantic texture patch flow that reconstructs the desired UI restyle against the current vanilla package instead of replaying the old package literally.
