## Message window — cleaned v100 review

### Inputs reviewed

- current vanilla package:
  - `D:\Elinu\S1Game\CookedPC\Art_Data\Packages\S1UI\S1UI_Message.gpk`
- catalog mod package:
  - `https://raw.githubusercontent.com/SaltyMonkey/tera-online-clean-onscreen-messages/master/S1UI_Message.gpk`

### Tooling used

- local prebuilt `GPK_RePack.Core.dll` from `C:\Users\Lukas\Documents\GitHub\elinu\scripts`
- local helper `dump_all_exports_detail.exe`

### Export/import surface summary

Vanilla package:

- exports: `181`
- imports: `366`

Modded package:

- exports: `1`
- imports: `414`

Vanilla starts with a large `Message` `GFxMovieInfo` plus many `ObjectRedirector`
exports. The modded file collapses the package to a single `Message`
`GFxMovieInfo` export while materially changing the import surface.

### Conclusion

This mod is **not** a safe v1 `replace_export_payload` candidate.

Why:

1. The export table shape changes dramatically (`181` → `1`).
2. The import table shape also changes materially (`366` → `414`).
3. Current v1 patch manifests cannot represent reviewed export deletion or the
   broader movie/import graph rewrite this mod appears to rely on.

### Status decision

- `blocked-unsupported-export-shape`

### What would unblock it

One of:

1. structural export-list / import-list editing support in the reviewed patch
   model, or
2. a higher-level semantic GFx patch flow that rewrites the current vanilla
   `Message` movie for the desired visibility behavior without replaying the
   old package literally.
