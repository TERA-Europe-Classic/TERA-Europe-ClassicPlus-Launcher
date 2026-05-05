# TERA GPK Modding — Deep-Dive Research Reference

> Compiled 2026-05-01 from primary sources: VenoMKO/TMM source (commit `f4cdc0e`, v1.20),
> VenoMKO/RealEditor source + wiki (30 pages), VenoMKO/TeraCoreLib headers (`Include/Tera/CoreVersion.h`,
> `CoreTMM.h`, `Core.cpp`, `FStructs.cpp`, `FPackage.cpp`, `FMapper.cpp`),
> GoneUp/GPK_RePack source (`GPK_RePack.Core/IO/MapperTools.cs`, `Model/Payload/GfxMovieInfo.cs`,
> `GpkStore.cs`, `DataTools.cs`) and its wiki (4 pages), `foglio1024/tera-restyle` repository
> (commit on master 2023-10-20), and `foglio1024/tera-modern-ui` `Modern_Resources/index.js`.
> Independent corroboration via Gemini deep-research summarising community sources.

This document is the canonical reference for the launcher's 32→64-bit UI-mod port pipeline.
It supersedes the prior assumption (audit dated 2026-04-30) that compiled an LZO-compressed
candidate GPK from the splicer; that artifact was deemed unusable in-game and is not the
shipping path.

---

## 1. The 64-bit transition (v100.02 / 2020-08)

Patch 97 introduced the composite-package architecture. Before patch 97 every UI surface
lived in its own `S1UI_*.gpk` in `S1Game/CookedPC/...`; after patch 97 every UI surface is
a *slice* of a giant container `.gpk` (filenames like `ff54e3e4_04.gpk`,
`c7a706fb_*.gpk`, `ffe86d35_*.gpk`) that holds dozens to hundreds of sub-packages
concatenated back-to-back. The legacy folder-structure GPKs in `Art_Data/Packages/S1UI`
still exist but their exports are now `Core.ObjectRedirector` shells that point at the
composite-stored real package.

The Unreal Engine 3 file version was bumped at the same time:

| Constant | Value | Used by |
|---|---|---|
| `VER_TERA_CLASSIC` | 610 | 32-bit pre-patch-97 |
| `VER_TERA_MODERN`  | 897 | 64-bit patch-97+ (incl. v100.02 / Classic+) |
| `VER_UDK_LATEST`   | 868 | UDK reference |

(`TeraCoreLib/Include/Tera/CoreVersion.h`)

Two `.dat` mapper files in `S1Game/CookedPC` are the engine's lookup tables:

- **`PkgMapper.dat`** — maps logical UID `S1UI_X.Y` to composite UID
  `c7a706fb_<hash>_<n>.Y_dup`. Plaintext form (verbatim from MapperTools.cs):
  `S1UI_SelectServer.SelectServer_I4C,ffe86d35_cb268950_1e082.SelectServer_I4C_dup|`
- **`CompositePackageMapper.dat`** — maps composite UID to physical
  `(filename, offset, size)`. Plaintext: 
  `<filename>?<objectPath>,<compositeUID>,<offset>,<size>,|...|!<filename>?...!`
  Example:
  `c7a706fb_6a349a6f_1d212.Chat2_dup,c7a706fb_6a349a6f_1d212,92291307,821218,|`

Both are encrypted on disk. Cipher (verbatim from TMM `Model/CompositeMapper.cpp`):

```cpp
const char Key1[16] = { 12, 6, 9, 4, 3, 14, 1, 10, 13, 2, 7, 15, 0, 8, 5, 11 };
const char Key2[21] = "GeneratePackageMapper";
```

Decrypt order (inverse of encrypt):

1. **Inverse 16-byte permute**: for every full 16-byte block at offset `O`,
   `decrypted[O+i] = encrypted[O + Key1[i]]`. Trailing `len % 16` bytes are copied as-is.
2. **Middle-out swap**: `a=1, b=size-1; for i in 0..((size/2+1)/2): swap(buf[a], buf[b]); a+=2; b-=2;`
   (own inverse).
3. **XOR Key2 mod 21**: `buf[i] ^= Key2[i % 21]`.

The launcher's existing Rust port in `services/mods/gpk.rs::decrypt_mapper` /
`encrypt_mapper` matches byte-for-byte. Round-trip verified by
`encrypt_then_decrypt_is_identity`.

A synthetic marker row `tmm_marker → tmm_marker?tmm_marker,tmm_marker,...` is added by TMM
on first-touch. Its presence proves the dat has been seen by TMM; absence after a
TERA repair/update is the trigger to re-back up the dat to `.clean`.

---

## 2. GPK file format (concise)

Authoritative source: `TeraCoreLib/Src/FStructs.cpp::operator<<(FStream&, FPackageSummary&)`.

### Header (97 bytes minimum, plus FString folder)

```
0    u32  Magic = 0x9E2A83C1
4    u16  FileVersion (610 or 897)
6    u16  LicenseVersion (typically 17 for Tera 64-bit)
8    i32  HeaderSize
12   FString FolderName ("None", or "MOD:<full_object_path>" for composite mods)
+0   u32  PackageFlags (0x02000000 = Compressed; 0x40000000 = Stripped; etc.)
+4   u32  NamesCount  (Classic-quirk: NameOffset is added on disk; subtract NameOffset on read)
+8   i32  NamesOffset
+12  u32  ExportsCount
+16  i32  ExportsOffset
+20  u32  ImportsCount
+24  i32  ImportsOffset
+28  i32  DependsOffset
[Modern-only, FileVersion > 610:]
+32  i32  ImportExportGuidsOffset
+36  u32  ImportGuidsCount
+40  u32  ExportGuidsCount
+44  u32  ThumbnailTableOffset
+48  16   FGuid
+64  i32  GenerationsCount
... per generation: i32 Exports, i32 Names, i32 NetObjects (12 bytes each)
... i32 EngineVersion (4206 default)
... i32 ContentVersion (76 default)
... u32 CompressionFlags (0=None, 1=ZLIB, 2=LZO, 4=LZX)
... per chunk header (16 bytes): DecompOffset, DecompSize, CompOffset, CompSize
... u32 PackageSource
... FStr[] AdditionalPackagesToCook
[Modern only:] FTextureAllocations TextureAllocations
```

### Body sections (in order)

1. **Name table** — one `FName` per entry: `i32 length; bytes[length]; i64 flags`. Length sign
   bit selects ASCII (positive) vs UTF-16 (negative).
2. **Import table** — fixed 28 bytes per entry: `i64 ClassPackage`, `i64 ClassName`, `i32
   Outer`, `i64 ObjectName`.
3. **Export table** — variable size: `i32 Class`, `i32 Super`, `i32 Outer`, `i32
   ObjectName`, `i64 Unk1`, `i64 Unk2`, `i32 SerialSize`, `i32 SerialOffset`, `i32 ExportFlags`,
   `i32 UnkHeaderCount`, `i32 Unk4`, `bytes[16] Guid`, `bytes[UnkHeaderCount*4]
   UnkExtraInts`.
4. **Depends table** — one `i32` per export, usually all zeros.
5. **Export bodies** — at each export's `SerialOffset`, see "Property block" below.

### Property block (shared by every UObject body)

Each property:

```
i64 NameIndex
i64 TypeIndex
i32 Size
i32 ArrayIndex
[value bytes per type]
```

Terminator: write a single `i64` "None" name index.

| Type            | Value bytes                                                  |
|-----------------|--------------------------------------------------------------|
| `IntProperty`   | `i32`                                                        |
| `FloatProperty` | `f32`                                                        |
| **`BoolProperty` (x64)** | **1 byte** (Size field in header is 0)              |
| **`BoolProperty` (x32)** | **4 bytes** (Size field in header is 0)             |
| **`ByteProperty` (x64)** | **`i64 enumType` then 1 byte or 8 bytes**           |
| **`ByteProperty` (x32)** | **1 byte or 8 bytes (no enumType prefix)**          |
| `NameProperty`  | `i32 nameIndex; i32 padding`                                 |
| `ObjectProperty`| `i32 objIndex` (positive = export, negative = import, 0 = none) |
| `StrProperty`   | `i32 length; bytes[]`                                        |
| `StructProperty`| `i64 innerType nameIndex; bytes[Size]`                       |
| `ArrayProperty` | raw `bytes[Size]` (parser does NOT recurse into elements)    |

The 1-byte vs 4-byte BoolProperty and the int64 enumType prefix on ByteProperty are the
two points where x64 export bytes are NOT byte-portable to/from x32. SWF payloads inside
ArrayProperty<byte> are byte-portable because `ArrayProperty` is opaque.

### Composite detection

After reading one complete GPK from a file, check the next 4 bytes against `0x9E2A83C1`.
If yes, another sub-GPK follows. The launcher's `services/mods/gpk_package.rs` parser
implements this.

When a sub-GPK is part of a TMM-style mod (or is the v100 vanilla composite slice
extracted as a standalone), its `FolderName` is `MOD:<full_object_path>` — e.g.
`MOD:c7a706fb_268926b3_1ddcb.PaperDoll_dup`. That string is what TMM keys off when
parsing a mod file (TMM `Model/Mod.cpp:218–225`).

### Header obfuscation flag (Classic only)

Per Gildor's UModel reverse-engineering: when `PackageFlags & 0x8` (BrokenLinks) is set in
a Classic-era GPK, `NameCount` is stored on disk as `actualNameCount + NameOffset` and
must be unobfuscated on read. Modern (897) packages do NOT carry this quirk; the
launcher's parser handles both.

---

## 3. RealEditor authoring workflow ("Create a composite mod")

Verbatim from `Create-a-composite-mod.md` (RealEditor wiki, six steps). Six high-level
operations:

1. **Generate `ObjectDump.txt`** — `Edit → Dump all composite objects`. ~1.6 GB; index of
   every composite-stored object across all containers. Re-run after a TERA patch.
2. **Open the GPK that owns the export you want to mod.** Right-click the export →
   **Bulk import...**. Pick `ObjectDump.txt` if asked.
3. **Configure the import action.** Select the *Import* tab, *Browse*, choose the new
   asset (texture file, sound file, **or arbitrary binary** for SWF/RawData), *Add*.
4. **Repeat** for additional resources if needed.
5. **Continue.** Pick output folder. Optional *Generate TFC* combines duplicate textures
   into one `WorldTextures<N>.tfc` (TMM v1.10+ required to install). Disable on crash.
6. **`File → Create a mod...`.** Select all generated GPKs (and any TFCs from step 5),
   enter Latin-only Name + Author, Save to... `MyMod.gpk`. **The output file is the
   TMM-deployable mod.**

Save options dialog (`CookingOptions.cpp`):

- **Embed composite information** — writes `MOD:<ObjectPath>` into FolderName. Auto-on
  for composite packages. Required by `CreateCompositeMod`.
- **Disable texture caching** — for region-free mods; pulls hi-res mips into the
  package. Default on for VER_TERA_MODERN packages without `PKG_NoSource`.
- **Preserve offsets** — forced on, not user-toggleable. Keeps unchanged objects at their
  original SerialOffset.
- **Compress package** — forced state (LZO if source was LZO). Can be edited indirectly
  via the source package's compression. **TMM decompresses on install, not at runtime —
  so the file in CookedPC is uncompressed.**

### "RawData" property edit (the SWF entry point)

`ObjectProperties.cpp::AByteArrayProperty::DisplayEditorDialog` opens
`BArrayPropEditDialog` with Export/Import buttons. **Import** (line 892):

```cpp
wxString path = wxFileSelector("Import property data", ...);
// resize bytes to file's size, replace contents
Value->Property->Size = size + 4;   // ArrayProperty: 4-byte count + N bytes
MarkDirty();
```

This is **the canonical UI operation for replacing the SWF bytes inside a
`Core.GFxUI.GFxMovieInfo` export's RawData property**. Property wrapper preserved; only
the inner byte array is swapped. Identical to what
`bin/splice-x32-payloads.rs::build_gfx_swap_payload` does in this repo.

### TMM-mod output format (`FCompositeMeta`)

Layout written by `FPackage::CreateCompositeMod` (`TeraCoreLib/Src/FPackage.cpp:362–505`):

```
1. Mod descriptor (synthetic GPK)
   - FolderName = "MOD:TMM version <maj>.<min>"
   - FileVersion = 897, LicenseVersion = 17
   - One UTextBuffer "ReadMe": "Mod: <name>\nAuthor: <author>\n..."
2. Per-package payloads (LZO-compressed if FILEMOD >= 3 && compression beats raw size)
   - Each is a complete GPK with FolderName "MOD:<ObjectPath>"
3. Embedded TFCs (FILEMOD >= 2)
4. FCompositeMeta trailer (read backwards from EOF)
   - PayloadCRC + MetaCRC patched after the rest is written
```

FILEMOD versions:

| Version | Constant | Features |
|---:|---|---|
| 1 | `VER_TERA_FILEMOD_INIT` | First release, plain trailer |
| 2 | `VER_TERA_FILEMOD_ADD_TFC` | + TFC embedding (TMM v1.10+) |
| 3 | `VER_TERA_FILEMOD_NEW_META` | + leading magic, CRCs, descriptor, per-package compression. Public TMM v1.20 emits a forward-compat warning |

Public TMM v1.20 reads v1 + v2. Internal RealEditor (1.x → 2.x) emits v3.

### Trailer layout (FILEMOD v2, read in reverse from EOF)

```
[EOF - 4]    u32   PACKAGE_MAGIC = 0x9E2A83C1            (tail magic)
[EOF - 8]    i32   metaSize
[EOF - 12]   i32   compositeCount
[EOF - 16]   i32   offsetsOffset   (file offset to package-offsets table)
[EOF - 20]   i32   containerOffset (file offset to Container FString)
[EOF - 24]   i32   nameOffset      (file offset to Name FString)
[EOF - 28]   i32   authorOffset    (file offset to Author FString)
[EOF - 32]   i32   regionLock      (0 or 1)
[EOF - 36]   i32   version         (PACKAGE_MAGIC for v1; integer 2/3 for v2+)
[v2+ only:]
[EOF - 40]   i32   compositeEnd
[EOF - 44]   i32   tfcOffsetsCount
[EOF - 48]   i32   tfcOffsetsOffset
[EOF - 52]   i32   tfcEnd
```

Strings are Unreal `FString`: signed i32 length, then ASCII (positive) or UTF-16 (negative).

If the trailing magic is *missing*, TMM falls back to: read the whole file as a single
GPK, extract its `MOD:<path>` FolderName, treat as a 1-package mod (Mod.cpp:170–177). So
**a bare x64 GPK with the right `MOD:` FolderName is a valid TMM mod with no footer**.

---

## 4. TMM install algorithm

Source: `TMM/UI/ModWindow.cpp::InstallMod` (lines 737–1005) and `TurnOnMod` (lines
1007–1069).

```
1. Read mod file: parse footer → ModFile { Container, Packages[], TfcPackages[], RegionLock, ... }
2. Validate every package: FileVersion == 897 (else REJECT — 32-bit mods are explicitly
   rejected with a user-facing dialog, line 793).
3. Validate every package's ObjectPath exists in the live CompositeMap:
   - if RegionLock: GetEntryByObjectPath (exact match)
   - else: GetEntryByIncompleteObjectPath (cross-region fuzzy match — strips numeric
     suffix on composite filename, e.g. S1UI_PaperDoll_0.X matches S1UI_PaperDoll_3.X)
4. dest = GetModsDir() / (Container + ".gpk")    // ModsDir == S1Game/CookedPC
5. Conflict scan: same (ModName, ModAuthor) prompts "update?"; same Container existing
   prompts "replace?".
6. Copy source file to dest.
7. For each TfcPackage: pick a free WorldTextures<idx>.tfc slot in [101, 899], read tfc
   payload from source mod, write WorldTextures<newIdx>.tfc into ModsDir.
8. TurnOnMod(mod):
   a. Conflict check across enabled mods (no two mods may target the same composite
      slot). On conflict: REJECT with message naming the other mod.
   b. For each package: find the existing CompositeEntry; OVERWRITE
      Filename = mod.Container, Offset = package.Offset, Size = package.Size.
      Keep CompositeName (the unique map key) unchanged.
9. Patch TFC names: for each remapped TFC, rewrite the name-table entry
   "WorldTextures<oldIdx>" → "WorldTextures<newIdx>" inside the dest GPK in-place.
10. Append ModEntry to ModList; SaveGameConfig (binary ModList.tmm).
11. CommitChanges: encrypt CompositeMap → write CompositePackageMapper.dat.
    (Or hold pending if WaitTera mode is on — KR/TW/JP only.)
```

`TurnOffMod`: for each package, look up the entry from `BackupMap` (the in-memory
decrypted `.clean`), call `CompositeMap.AddEntry(backup_entry)` (overwrites the modded
row). Delete the dest file. Delete WorldTextures<idx>.tfc files. Save mapper.

`OnResetClicked` (full vanilla restore): turn off every mod, copy `.clean` over `.dat`.

`IncompletePathsEqual` (`Utils.cpp:9–39`): the cross-region matcher. Compares two object
paths by `(composite_prefix_before_last_underscore, object_path_after_first_dot)`,
ignoring the trailing numeric region suffix. Implementation already mirrored in this
repo's `services/mods/gpk.rs::incomplete_paths_equal`.

### Key files / paths in the live game

| Path | Used by |
|---|---|
| `S1Game/CookedPC/CompositePackageMapper.dat` | live mapper (mutated) |
| `S1Game/CookedPC/CompositePackageMapper.clean` | TMM's vanilla backup |
| `S1Game/CookedPC/PkgMapper.dat` | logical-name → composite-UID lookup (rarely mutated by TMM) |
| `S1Game/CookedPC/PkgMapper.clean` | TMM's vanilla backup |
| `S1Game/CookedPC/<Container>.gpk` | dropped mod files (live next to vanilla) |
| `S1Game/CookedPC/WorldTextures<idx>.tfc` | mod TFCs (idx 100–899; 0–99 reserved for vanilla) |
| `S1Game/CookedPC/ModList.tmm` | TMM's enabled-mods list |
| `%LOCALAPPDATA%/TMM/Settings.ini` | TMM app settings |

**TMM's documented strategy is mapper redirection only.** It does NOT splice mod payloads
into vanilla composite containers. It drops a standalone `.gpk` into `CookedPC` with a
new filename and rewrites the mapper row to point at it. The vanilla composite container
(e.g. `ff54e3e4_04.gpk`) is left untouched.

### Why mapper redirection is safe for multi-slice packages

A composite container is `N` concatenated GPK sub-files. The mapper says "object X lives
at (file F, offset O, size S)". When TMM redirects X to a new file F' with offset 0 and
size S', the engine reads the new file from offset 0 and parses it as a complete GPK.
Other slices in the original container are untouched — their mapper entries still point
to the original file F at their original offsets. **No offset cascade occurs.**

The launcher's prior audit (2026-04-30, `foglio-paperdoll-resource-blocker.md`) flagged
"standalone mapper redirection unsafe for multi-slice packages." That conclusion was
drawn from a specific failure mode where an in-place container slice patch shrank
`ff54e3e4_04.gpk` and stale `.clean` mapper entries pointed beyond live container EOF.
That failure mode does not apply to mapper redirection, only to in-place patching.
**Mapper redirection is what TMM does and is the documented-safe path.**

---

## 5. The PaperDoll mod, end to end

### Source (foglio1024/tera-restyle, p95)

Repository: `https://github.com/foglio1024/tera-restyle` (default branch `master`,
last push 2023-10-20). The PaperDoll directory is laid out per TERA major patch (p79,
p83, p85, p87, p90, p93, p95). The newest is **p95**, which holds:

| File | Size | Role |
|---|---:|---|
| `PaperDoll/p95/S1UI_PaperDoll.gpk` | 1,245,088 | foglio's modded x32 GPK (FileVersion 610, uncompressed, no MOD: folder) |
| `PaperDoll/p95/S1UI_PaperDoll.gpk_original` | 1,242,275 | vanilla x32 baseline (for diffing) |
| **`PaperDoll/p95/mod.gfx`** | **493,899** | **the loose modded SWF — Scaleform GFx file** |
| `PaperDoll/scripts/PaperDoll.js` | 217,695 | JPEXS P-code dump of the SWF |
| `PaperDoll/scripts/frame_1/DoAction.as` | 101,084 | high-level AS2 source for frame 1 |

The local `.gpk-mod-cache/7f890f4b...` (1,245,088 bytes) is byte-identical to
`p95/S1UI_PaperDoll.gpk`.

The loose `mod.gfx` SWF is exactly what gets injected into the vanilla GPK's
`GFxMovieInfo.RawData` ArrayProperty — it's portable across architectures because
Scaleform GFx is a runtime-format binding into the engine, not an engine-compiled blob.

### SWF format

`mod.gfx` magic: `47 46 58 09` (`GFX\x09`) — Scaleform GFx, SWF-version-9 frame, AS2
era. Header tail `00 07 d0 00 00 17 70 00 00 18 01 00 13 fa 64` is a fixed 800x600
stage descriptor shared across foglio's gfx files.

The SWF authors against a fixed contract with the engine (verified from
`scripts/frame_1/DoAction.as`):

- Outbound (SWF → game) via `getURL("FSCommand:<name>", arg)`:
  `ToGame_PaperDoll_Init`, `ToGameShowPaperDoll`, `ToGameRotatePaperDoll`,
  `ToGame_PaperDoll_RequestOpenEnchantUI`, `ToGame_PaperDoll_CloseUI`,
  `ToGame_PaperDoll_RightClick`, `ToGame_PaperDoll_LeftClickSlot`,
  `ToGame_PaperDoll_RequestPVP`, `ToGame_PaperDoll_TabFocus`,
  `ToGame_PaperDoll_SaveItemSet`, `ToGame_PaperDoll_RequestStyleInfo`, …
- Inbound (game → SWF) via `myListener` listener attached on `_global.EventBroadCaster`:
  `OnGameEventShowWindow`, `OnGameEventUpdatePaperDollSlotList`,
  `OnGameEvent_PaperDoll_SetSilhouette(sex, race, _theOther)`,
  `OnGame_PaperDoll_UserName`, `OnGame_PaperDoll_SetItemLevel`,
  `OnGame_PaperDoll_AddReputation`, `OnGame_PaperDoll_SetTitleInfo`,
  `OnGame_PaperDoll_SetVIP`, …

The contract is **identical to vanilla TERA** — foglio re-skins, doesn't extend behavior.
A v100.02 client whose engine still calls these listener names will exercise the modded
SWF the same way the vanilla SWF was exercised.

### Runtime resource dependency

At line 1380–1400 of `DoAction.as` (verbatim from foglio source):

```as
myListener.OnGameEvent_PaperDoll_SetSilhouette = function(sex, race, _theOther) {
   if(Number(_theOther) != Number(isTheOther)) { return undefined; }
   var _loc2_ = flash.display.BitmapData.loadBitmap(
       "img://__S1UIRES_Skin.PaperDoll_" + race + "_" + sex);
   container_mc.uiBgMc2.attachBitmap(_loc2_, 1);
   ...
}
```

The `img://` URI scheme is Scaleform's hook into the host engine's asset registry.
`__S1UIRES_Skin.PaperDoll_<race>_<sex>` resolves at runtime to a `Texture2D` named
`PaperDoll_<race>_<sex>` inside the `S1UIRES_Skin` package. **If the v100.02 vanilla
`S1UIRES_Skin` does not contain those textures, the silhouettes will render blank.**

Foglio's wiki for the Profile (PaperDoll) window says verbatim:
> "Requires **S1UIRES_Skin** and **S1UIRES_Component** too."

The v100.02 vanilla `S1UIRES_Skin.gpk` is **7,037 bytes** (audit from
`foglio-paperdoll-resource-blocker.md`); foglio's p85 `S1UIRES_Skin.gpk` is
**9,754,339 bytes**. So the v100.02 vanilla atlas is heavily slimmed and may or may not
expose the per-race silhouette textures. **First port pass should ship without those
resources** — the modded UI layout will render, the silhouettes may render with vanilla
fallback textures or empty. A second pass can rebuild `S1UIRES_Skin` if needed (gated by
an x64-native Texture2D serializer, see §7).

### Foglio's own x64 stance

`foglio1024/tera-modern-ui/Modern_Resources/index.js` opens with:

```js
exports.ClientMod = class {
    constructor(mod) {
        this.install = function(installer) {
            if (mod.clientInterface.info.arch === 'x64') return;
            mod.warn("This mod will be disabled without showing any warning after
                      64-bit client patch hits. ...");
            ...
            if (mod.majorPatchVersion >= 93) {
                installer.gpk("85/S1UIRES_Skin.gpk");
                installer.gpk("93/S1UIRES_Component.gpk");
                installer.gpk("91/S1UIRES_Atlas.gpk");
            }
            ...
        };
    }
}
```

Two early returns: x64 → silent skip; majorPatchVersion > 93 → no-op. Foglio considered
re-enabling but never shipped it. There is **no x64 variant of `S1UIRES_Skin.gpk` in
either repo**. Porting paperdoll to v100.02 therefore needs to either:

1. Accept blank silhouettes (or whatever vanilla v100.02 falls back to),
2. Build a custom v100.02 `S1UIRES_Skin` using vanilla as a base and re-injecting the
   foglio DDS textures (gated on Texture2D serializer),
3. Or ship a minimal Texture2D that overrides only `PaperDoll_<race>_<sex>` slots.

This document scopes path 1 (UI-only port). Path 2/3 is future work.

---

## 6. The port: concrete plan

Author the x64 PaperDoll mod as a single composite-mod GPK, then install via mapper
redirection. No in-place container patching.

### Inputs

- **Vanilla x64 PaperDoll_dup composite slice** — extracted at
  `C:/Users/Lukas/Documents/GitHub/TERA EU Classic/.gpk-rebuild/vanilla/S1UI_PaperDoll.PaperDoll_dup.gpk`
  (8,336,198 bytes, FileVersion 897, uncompressed, FolderName already set to
  `MOD:c7a706fb_268926b3_1ddcb.PaperDoll_dup`).
- **Foglio modded SWF** — `C:/Users/Lukas/AppData/Local/Temp/tera-restyle-clone/PaperDoll/p95/mod.gfx`
  (493,899 bytes, raw Scaleform GFx).
- **Live mapper** — `D:/Elinu/S1Game/CookedPC/CompositePackageMapper.dat` (verified clean,
  matches `.clean` baseline). Logical entry:
  `S1UI_PaperDoll.PaperDoll → c7a706fb_268926b3_1ddcb.PaperDoll_dup`.
  Composite entry: `file=ff54e3e4_04 offset=0 size=8,336,198`.

### Authoring algorithm (Rust binary `port-paperdoll-x64`)

```
1. Read vanilla x64 PaperDoll_dup bytes (already uncompressed standalone GPK).
2. Parse the GPK summary + name + import + export tables.
3. Locate the export named "PaperDoll_dup" with class "Core.GFxUI.GFxMovieInfo".
   - SerialOffset, SerialSize give the export body region.
4. Inside the export body, scan property block until property name "RawData"
   (or, simpler: scan the body bytes for the GFX magic 0x47 0x46 0x58 + version
   byte 0x07..0x0C). 4 bytes before the magic = ArrayProperty count;
   12 bytes before = ArrayProperty Size in the property header.
5. Read foglio mod.gfx into memory (493,899 bytes).
6. Synthesize new export body:
   - prefix = vanilla[..GFX_offset]
   - new_swf = foglio_mod_gfx_bytes
   - suffix = vanilla[GFX_offset + old_count ..]
   - patch ArrayProperty count at GFX_offset - 4 = new_swf.len()
   - patch ArrayProperty Size at GFX_offset - 12 = new_swf.len() + 4
   - new body = prefix + new_swf + suffix
7. Recompute SerialSize = new body length. Update export entry's SerialSize.
8. Recompute downstream offsets:
   - Every export with SerialOffset > old PaperDoll_dup SerialOffset shifts by
     (new SerialSize - old SerialSize).
9. Re-emit GPK:
   - Header (rewrite NamesOffset/ExportsOffset/ImportsOffset/DependsOffset to reflect
     new positions; or, since the body order is name-import-export-depends-bodies and
     only export bodies move, leave the table positions fixed and shift only export
     SerialOffsets — simpler).
   - Keep CompressionFlags = 0 (uncompressed). TMM-style mods land uncompressed in
     CookedPC.
   - Keep FolderName = "MOD:c7a706fb_268926b3_1ddcb.PaperDoll_dup".
   - Keep PackageFlags as vanilla.
10. Self-verify: parse the emitted file, assert names/imports/exports counts unchanged,
    PaperDoll_dup export's payload contains the new SWF, no offsets out of bounds.
11. Write to <output>/RestylePaperdoll.gpk (Latin-only filename per RealEditor rule).
```

### Optional FILEMOD-v2 footer

For TMM-tool-loadable installation. Not required for the launcher's own install path
(which redirects mapper directly). If we add it: 1 composite package, no TFCs,
Container="RestylePaperdoll", Name="Foglio Restyle PaperDoll", Author="foglio1024
(ported)".

### Install algorithm (Rust binary `install-paperdoll-x64`)

```
1. Verify D:/Elinu state:
   - CompositePackageMapper.dat == .clean (or accept divergence with a flag).
   - ff54e3e4_04.gpk == ff54e3e4_04.gpk.vanilla-bak (or .vanilla-bak missing →
     create it now).
2. Copy RestylePaperdoll.gpk to D:/Elinu/S1Game/CookedPC/RestylePaperdoll.gpk
   (atomic: write to .tmp, fsync, rename).
3. Decrypt CompositePackageMapper.dat → text → parse → CompositeMap.
4. Find entry by object_path "c7a706fb_268926b3_1ddcb.PaperDoll_dup":
   - Old: filename=ff54e3e4_04, offset=0, size=8,336,198.
   - New: filename=RestylePaperdoll, offset=0, size=<emitted file length>.
5. Add tmm_marker row if missing.
6. Re-serialize CompositeMap → encrypt → write CompositePackageMapper.dat (atomic).
7. (Don't touch PkgMapper.dat — only CompositePackageMapper.dat needs rewriting.)
8. Self-verify: re-decrypt, find entry, confirm new filename/offset/size.
9. Print confirmation.
```

### Uninstall (rollback)

```
1. Decrypt CompositePackageMapper.dat → CompositeMap.
2. Read .clean → BackupMap.
3. For object_path "c7a706fb_268926b3_1ddcb.PaperDoll_dup":
   replace CompositeMap entry with BackupMap entry.
4. Encrypt + write CompositePackageMapper.dat.
5. Delete D:/Elinu/S1Game/CookedPC/RestylePaperdoll.gpk.
6. Verify: SHA(CompositePackageMapper.dat) == SHA(.clean).
```

---

## 7. Why prior attempts failed (post-mortem)

The prior candidate at `.gpk-rebuild/publish/foglio1024.restyle-paperdoll.S1UI_PaperDoll.gpk`
(SHA `a8e7ea9e...`, 1,481,742 bytes) was **LZO-compressed**. The header reports
`compression_flags=2`, `chunk_count=1`, chunk decompresses 8,339,520 bytes from
1,481,561 compressed bytes.

But **TMM's documented behavior decompresses on install**: the file landing in
`CookedPC` should be uncompressed (`FPackage::InstallCompositeMod`,
TeraCoreLib `Src/FPackage.cpp:507–681`, line 581:
`dst.Compression = COMPRESS_None`). Vanilla composite slices in v100.02 CookedPC are
stored uncompressed at the slice level; the engine's runtime LZO decompression path is
exercised for full-file-compressed packages, not for sub-slice GPKs read at a known
offset within a larger container. Whether the engine *can* still decompress a
sub-slice GPK with `CompressionFlags=2` is unclear — and not a path the tooling
ecosystem documents or supports.

Other failure surfaces:

- The candidate's uncompressed body is 8,339,520 bytes — only +3,322 bytes vs vanilla.
  But foglio's `mod.gfx` is 493,899 bytes vs the vanilla SWF inside the x64 PaperDoll
  package (~7.8 MB by subtraction). A real splice should produce a body ~7.3 MB
  *smaller* than vanilla, not larger by 3 KB. **Indicates the splicer kept the vanilla
  SWF instead of swapping it.** The most likely cause: the splicer used the foglio x32
  `S1UI_PaperDoll.gpk`'s GFxMovieInfo as the SWF source and then byte-swapped *that*
  (which contains the same modded SWF wrapped in x32 property bytes), but if the GFX
  magic offset in the x32 file's property block differed from what the splicer expected
  (because x32 BoolProperty and ByteProperty have different sizes than x64), the splice
  may have aimed at the wrong byte range.
- Even if the splice *had* worked correctly, the resulting compressed candidate would
  not match TMM's documented install-path expectation (uncompressed in CookedPC).

The new pipeline avoids both surfaces by (a) injecting the loose `mod.gfx` directly into
the *x64* vanilla GfxMovieInfo body (skipping x32 property parsing entirely), and (b)
emitting an uncompressed GPK.

---

## 8. Verification gates before in-game smoke test

Each gate must pass with explicit evidence (numbers, hashes, tool output).

**G1: Asset integrity.**
- `mod.gfx` SHA-256 known and matches foglio source.
- Vanilla x64 PaperDoll_dup parses cleanly, has exactly one
  `Core.GFxUI.GFxMovieInfo` export named `PaperDoll_dup`, contains exactly one GFX
  magic in its body, GFX section length matches `count` field at `gfx_offset - 4`.

**G2: Splice arithmetic.**
- New body length = old prefix + len(mod.gfx) + old suffix.
- Old SerialSize - New SerialSize = vanilla SWF byte count - foglio SWF byte count.

**G3: Re-parse cleanliness.**
- Emitted file parses with `gpk_package::parse_package`.
- Names count, imports count, exports count unchanged vs vanilla.
- All export `SerialOffset + SerialSize` <= file size.
- FolderName preserved as `MOD:c7a706fb_268926b3_1ddcb.PaperDoll_dup`.
- FileVersion = 897, LicenseVersion = 17.

**G4: Mapper round-trip.**
- After installing, decrypt mapper.dat, find entry for object_path target → matches
  expected (filename, offset=0, size=<file length>).
- Encrypt mapper, decrypt again, verify identity.

**G5: D:\Elinu state safety.**
- Before install: `ff54e3e4_04.gpk` SHA == `ff54e3e4_04.gpk.vanilla-bak` SHA.
- After install: `ff54e3e4_04.gpk` SHA UNCHANGED (we only added a new file and rewrote
  one mapper row).
- After uninstall: `CompositePackageMapper.dat` SHA == `.clean` SHA. Modded GPK file
  removed.

Only after G1–G5 pass do we hand off to the user for in-game smoke test.

---

## 9. Out of scope (future work)

- **`S1UIRES_Skin` rebuild for v100.02.** Requires an x64-native Texture2D
  read/write path. Foglio's mod cannot render race silhouettes correctly without it.
  Decoupled from this port; UI layout port can ship with vanilla resource fallback.
- **Bulk import / TFC packaging.** RealEditor "Bulk import..." finds duplicates
  across composite packages. Paperdoll's main UI surface is a single object
  (`PaperDoll_dup`), so bulk import is unnecessary for this port. Will be needed
  later for textures/sounds that have many duplicates.
- **TMM v3 FILEMOD output.** The launcher's install path doesn't need it; the file
  goes straight from authoring tool into CookedPC + mapper rewrite. If we want
  external TMM tool compatibility, add a v2 footer to the emitted file (one composite
  package, no TFCs) — straightforward; ~50 lines of Rust.

---

## 10. References (URLs, reachable as of 2026-05-01)

- VenoMKO/TMM — https://github.com/VenoMKO/TMM
- VenoMKO/RealEditor — https://github.com/VenoMKO/RealEditor
- VenoMKO/RealEditor wiki — https://github.com/VenoMKO/RealEditor/wiki
- VenoMKO/TeraCoreLib — https://github.com/VenoMKO/TeraCoreLib
- GoneUp/GPK_RePack — https://github.com/GoneUp/GPK_RePack
- GoneUp/GPK_RePack wiki — https://github.com/GoneUp/GPK_RePack/wiki
- foglio1024/tera-restyle — https://github.com/foglio1024/tera-restyle
- foglio1024/tera-modern-ui — https://github.com/foglio1024/tera-modern-ui
- foglio1024/tera-custom-cooldowns — https://github.com/foglio1024/tera-custom-cooldowns
- Gildor's UModel — http://www.gildor.org/en/projects/umodel

Local reference clones (read-only):

- `C:/Users/Lukas/Documents/GitHub/TERA EU Classic/TMM` — TMM source
- `C:/Users/Lukas/Documents/GitHub/TERA EU Classic/RealEditor` — RealEditor source
- `C:/Users/Lukas/AppData/Local/Temp/RealEditor-wiki` — wiki (.md files)
- `C:/Users/Lukas/Documents/GitHub/GPK_RePack` — GPK_RePack source
- `C:/Users/Lukas/AppData/Local/Temp/tera-restyle-clone` — foglio source

Skill: `~/.claude/skills/gpk-tera-format/SKILL.md` — internal reference compiled from
TeraCoreLib + GPK_RePack.
