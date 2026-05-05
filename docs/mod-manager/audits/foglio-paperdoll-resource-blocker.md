# Foglio PaperDoll shared-resource blocker

## Status

`foglio1024.restyle-paperdoll` is stable only as a GFx-only x64 rebuild. Do not publish or deploy `S1UIRES_Skin.gpk` or `S1UIRES_Component.gpk` candidates until the launcher has an x64-native `Texture2D` round-trip serializer and validator.

## Why `_restyle` is not enough

Foglio's original guide says to create an underscore-prefixed folder such as `_restyle` or `_S1UI` under `TERA/Client/S1Game/CookedPC` and place the GPKs there. That is an install-location rule for the old client layout; it does not adapt x32 package data to current x64 package and texture layouts.

The later `foglio1024/tera-modern-ui` `Modern_Resources/index.js` explicitly returns on x64 before installing resources. For pre-x64 patch 93+, it would install:

- `85/S1UIRES_Skin.gpk`
- `93/S1UIRES_Component.gpk`
- `91/S1UIRES_Atlas.gpk`

Those files are useful as intent/source assets, not as directly deployable current x64 packages.

## PaperDoll script findings

`https://github.com/foglio1024/tera-restyle/tree/master/PaperDoll/scripts` contains ActionScript/GFx behavior references, not resource-package tooling.

Useful references:

- PaperDoll UI/game command contract: `ToGame_PaperDoll_Init`, `ToGameShowPaperDoll`, `ToGameRotatePaperDoll`, `ToGame_PaperDoll_RequestOpenEnchantUI`, `ToGame_PaperDoll_CloseUI`, `ToGame_PaperDoll_RightClick`, `ToGame_PaperDoll_LeftClickSlot`.
- Item and crystal slot parsing: game-provided icon URI strings are drawn with `lib.display.DrawBitmapData.draw(...)` / `lib.util.DrawBitmap.draw(...)`.
- Shared skin silhouette lookup: `img://__S1UIRES_Skin.PaperDoll_` + `race` + `_` + `sex`.
- Jewel icon remap references in `jewels.js`, e.g. `img://__Icon_Items.BlueCustomize..._Tex`.

Not present:

- GPK package writer
- x64 header/export/import serializer beyond existing package-level code
- `Texture2D` serializer
- mipmap, DDS, TFC, or texture-cache writer
- x64 conversion logic

## Local baseline facts

Current x64 vanilla resource files are standalone S1UI package files:

| Package | Path | Size | SHA-256 |
|---|---|---:|---|
| `S1UIRES_Component.gpk` | `D:\Elinu\S1Game\CookedPC\Art_Data\Packages\S1UI\S1UIRES_Component.gpk` | `512411` | `65c8e3d9b01a7a363b86eba867bb4a9bf9edaee2640ada381ad7afffe4406579` |
| `S1UIRES_Skin.gpk` | `D:\Elinu\S1Game\CookedPC\Art_Data\Packages\S1UI\S1UIRES_Skin.gpk` | `7037` | `8f08c23ba52eaa3a8612d4047795bd1b550ea6b0fc96fe37876fc1a2ef78380a` |

Foglio pre-x64 resource sources are much larger:

| Source | Package | Size |
|---|---|---:|
| `foglio1024/tera-modern-ui/Modern_Resources/85` | `S1UIRES_Skin.gpk` | `9754339` |
| `foglio1024/tera-modern-ui/Modern_Resources/83` | `S1UIRES_Skin.gpk` | `9754339` |
| `foglio1024/tera-modern-ui/Modern_Resources/93` | `S1UIRES_Component.gpk` | `1744763` |
| `foglio1024/tera-modern-ui/Modern_Resources/90` | `S1UIRES_Component.gpk` | `1745272` |
| `foglio1024/tera-modern-ui/Modern_Resources/86` | `S1UIRES_Component.gpk` | `1740398` |
| `foglio1024/tera-modern-ui/Modern_Resources/85` | `S1UIRES_Component.gpk` | `1712084` |
| `foglio1024/tera-modern-ui/Modern_Resources/83` | `S1UIRES_Component.gpk` | `1601775` |

The size difference, the x64-disable guard, and prior crash results make direct file/drop-in use unsafe.

## 2026-04-30 research synthesis

External references reviewed:

- VenoMKO/TMM source and wiki.
- VenoMKO/RealEditor wiki and source references.
- GoneUp/GPK_RePack 64-bit/ObjectMapper wiki and source references.
- Foglio `tera-custom-cooldowns` source tree and install script.

Conclusions:

- 64-bit TERA looks up most UI resources through `PkgMapper.dat` and `CompositePackageMapper.dat` before ordinary package files. A logical object such as `S1UI_PaperDoll.PaperDoll_I147` resolves to a composite UID, then to `(filename, offset, size)` inside a composite container.
- TMM composite mods are metadata containers. RealEditor writes embedded package folder names as `MOD:<object-path>` so TMM can recover the mapper target, then TMM copies the container to `CookedPC` and rewrites `CompositePackageMapper.dat` entries to point at package offsets inside that container.
- RealEditor warns final TMM mod filenames should contain only Latin letters/numbers. The dotted/hyphenated launcher-generated container name `foglio1024.restyle-paperdoll.resources-x64.gpk` is not aligned with that guidance.
- GPK_RePack's 64-bit Toolbox/ObjectMapper workflow is not "wrap old bytes in a new GPK." It starts from the original x64 package, loads the corresponding composite export, copies real x64 export data over the redirector/base export, minimizes unchanged redirectors, and lets Toolbox/ObjectMapper patch lookup entries.
- Foglio's own `tera-custom-cooldowns` ships separate `x86` and `x64` GPKs and chooses by client architecture in `tcc-launcher.js`. Foglio does not ship a `S1UI_PaperDoll.gpk` precedent in that repo. The x64 files have file version `0x381`; the x86 files have file version `0x262`.
- Therefore Foglio's old 32-bit resource packages are source/intent, not byte-port templates for x64 PaperDoll resources.

Local crash root cause found during research:

- The no-op resource-routing canary was confounded by a stale patched `D:\Elinu\S1Game\CookedPC\ff54e3e4_04.gpk`.
- Clean mapper entries pointed `S1UI_PaperDoll.PaperDoll_I147` to `ff54e3e4_04` at offset `8336198`, size `527270`.
- The live `ff54e3e4_04.gpk` was only `2008864` bytes while `ff54e3e4_04.gpk.vanilla-bak` was `8863468` bytes. The game log then fell back to `..\S1Game\CookedPC\ffe86d35_e90341cb_1ddaf.gpk` and reported "unrecognizable data."
- Restoring only mapper files is unsafe when composite containers have been patched and shrunk. A clean restore must also copy every `*.gpk.vanilla-bak` back over its live `.gpk` before trusting `.clean` mapper offsets.

Immediate engineering fix:

- `gpk::restore_clean_gpk_state` restores backed-up `.gpk` containers and both mapper files together.
- `restore-clean-gpk-mappers` now uses that full mapper/container restore instead of mapper-only writes.
- `rebuild_gpk_state` starts from the full clean GPK state, preventing stale container/clean mapper mismatches on rebuild.

## Failed unsafe approaches

Do not restore these approaches:

- Whole x32 `Texture2D` payload replacement into the x64 PaperDoll package.
- x64 wrapper plus x32 post-`None` native tail replacement.
- Raw shared-resource GPK deployment through `_restyle` / `_S1UI` without x64 validation.
- Mapper-only restore after shrinking a composite container. This leaves valid clean offsets pointing beyond live container EOF and can crash later unrelated package loads.

Both PaperDoll texture payload approaches crashed the client. The current `splice-x32-payloads` safe path must remain limited to `Core.GFxUI.GFxMovieInfo --gfx-swap` and class-filtered replacements.

## Required gates before resource generation

1. Parse current x64 `S1UIRES_Component.gpk` and `S1UIRES_Skin.gpk`; record `FileVersion`, export classes, names/imports/exports, compression, and whether texture data is inline or TFC-backed.
2. Implement a minimal x64 `Texture2D` read/write path against the exact PaperDoll resource shapes.
3. Prove vanilla round-trip: parse -> serialize -> parse current x64 resource packages with unchanged object graph and valid offsets.
4. Extract Foglio resources as image/intent only; re-emit fresh x64 `Texture2D` exports. Do not copy x32 texture payloads or tails.
5. Reparse generated candidates and validate every generated texture dimension, format, mip count, compression block, and TFC reference.
6. Only after all gates pass, publish candidate resource packages and run install/disable/re-enable plus PaperDoll smoke tests across multiple race/sex silhouettes.

## Current release decision

Keep the stable GFx-only PaperDoll artifact published. The remaining old backplate/graphics are a known resource dependency blocker, not a reason to deploy unvalidated x32 resource bytes.
