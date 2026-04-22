## Target info — restyle v100 review

### Inputs reviewed

- current vanilla package:
  - `D:\Elinu\S1Game\CookedPC\Art_Data\Packages\S1UI\S1UI_TargetInfo.gpk`
- catalog mod package:
  - `https://raw.githubusercontent.com/teralove/TERA-UI-Mods/master/S1UI_TargetInfo_01/S1UI_TargetInfo.gpk`

### Tooling used

- local prebuilt `GPK_RePack.Core.dll` from `C:\Users\Lukas\Documents\GitHub\elinu\scripts`
- local helper `dump_all_exports_detail.exe`

### Export/import surface summary

Vanilla package:

- exports: `2`
- imports: `10`
- export mix: one `GFxMovieInfo` + one `ObjectRedirector` (`bitmap_debuff`)

Modded package:

- exports: `18`
- imports: `4`
- export mix: one `GFxMovieInfo` + broad `Texture2D` export surface (`Icon_2_TEX`, `TargetInfo_I*`)

### Conclusion

This mod is **not** a safe v1 `replace_export_payload` candidate.

Why:

1. The export table expands from `2` exports to `18` exports.
2. The import table shrinks materially from `10` imports to `4` imports.
3. The vanilla package's single redirector-based surface becomes a multi-texture export surface in the modded package.
4. Current v1 manifests cannot represent reviewed export creation plus the accompanying import-table contraction safely.

### Status decision

- `blocked-unsupported-export-shape`

### What would unblock it

One of:

1. reviewed support for export creation together with class+payload replacement and import/name table editing, or
2. a higher-level semantic texture restyle flow that reconstructs the desired target-frame textures against current vanilla instead of replaying the old package literally.

### Why this matters for v2 planning

This package is a strong proof that the `restyle` family is not just a mild
payload-swap problem:

- it needs net-new export shape,
- it changes the import graph in the opposite direction from several `ui-remover`
  packages,
- and it therefore reinforces that Wave A needs real structural operations, not
  only a broader list of payload replacements.
