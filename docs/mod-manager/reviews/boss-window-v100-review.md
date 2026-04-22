## Boss Window v100 review

### Inputs reviewed

- current vanilla package:
  - `D:\Elinu\S1Game\CookedPC\Art_Data\Packages\S1UI\S1UI_GageBoss.gpk`
- catalog mod package:
  - `https://raw.githubusercontent.com/foglio1024/UI-Remover/master/remove_BossWindow/S1UI_GageBoss.gpk`

### Tooling used

- local prebuilt `GPK_RePack.Core.dll` from `C:\Users\Lukas\Documents\GitHub\elinu\scripts`
- local helper `dump_all_exports_detail.exe`

### Export surface summary

Vanilla exports:

- `GageBoss` → `Core.GFxMovieInfo`
- `GageBoss_I1C` → `Core.ObjectRedirector`

Modded exports:

- `GageBoss` → `Core.GFxMovieInfo`

Import surface summary:

- vanilla imports: 8
- modded imports: 8

### Conclusion

This mod is **not** a safe v1 `replace_export_payload` candidate.

Why:

1. The modded package removes the `GageBoss_I1C` export entirely.
2. Current v1 patch manifests do not support reviewed export deletion.
3. Even though the import surface count stays the same, the export table shape does not.

The likely intent is still simple — hide the default boss gauge — but the current patch-manifest schema cannot represent “remove this redirector export / rewire the owning movie” safely.

### Status decision

- `blocked-unsupported-export-shape`

### What would unblock it

One of:

1. export deletion support in the reviewed patch-manifest/applier model, or
2. a higher-level semantic GFx patch path that rewrites the movie/export relationship without replaying the old package literally.
