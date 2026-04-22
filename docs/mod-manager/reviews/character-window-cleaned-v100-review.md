## Character window — cleaned v100 review

### Inputs reviewed

- current vanilla package:
  - `D:\Elinu\S1Game\CookedPC\Art_Data\Packages\S1UI\S1UI_CharacterWindow.gpk`
- catalog mod package:
  - `https://raw.githubusercontent.com/SaltyMonkey/tera-online-clean-character-bar/master/S1UI_CharacterWindow.gpk`

### Tooling used

- local prebuilt `GPK_RePack.Core.dll` from `C:\Users\Lukas\Documents\GitHub\elinu\scripts`
- local helper `dump_all_exports_detail.exe`
- local launcher converter seam `gpk-patch-converter` (used only to confirm the current converter does **not** yet emit a reviewed candidate for this package)

### Export/import surface summary

Vanilla package:

- exports: `175`
- imports: `354`
- export mix: one `GFxMovieInfo` + a large `ObjectRedirector` surface + named UI redirectors like `hp`, `mp`, `st`

Modded package:

- exports: `1`
- imports: `354`
- export mix: one `CharacterWindow` `GFxMovieInfo` only

### Conclusion

This mod is **not** a safe v1 `replace_export_payload` candidate.

Why:

1. The export table collapses from `175` exports to `1` export.
2. The import count remains stable (`354` -> `354`), which means this is **not** the same blocker shape as `Message window — cleaned`.
3. The mod intent appears to be: keep the main `CharacterWindow` movie but remove the broad redirector-driven decorative/resource surface around it.
4. Current v1 manifests cannot express reviewed bulk export removal, even when the import graph itself does not need to change.

### Status decision

- `blocked-unsupported-export-shape`

### What would unblock it

One of:

1. reviewed export-removal support that can safely delete the large redirector surface while preserving the surviving `CharacterWindow` movie export, or
2. a higher-level semantic GFx/UI patch path that reconstructs the same cleaned presentation against current vanilla without replaying the old package literally.

### Why this matters for v2 planning

This package is a useful midpoint between the existing reviewed seeds:

- smaller and simpler than `Message window — cleaned` because the import graph is unchanged,
- broader than `Boss Window` because it needs bulk export removal rather than a single export delete.

So it is a strong candidate for validating whether `remove_export` in v2 can scale from a one-off deletion to a reviewed multi-export collapse without immediately needing import-table mutation support.
