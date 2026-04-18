# External mod catalog — seed entries

Source of truth for the initial `external-mod-catalog` repo contents.
Each entry below must, before shipping, resolve to a `sha256`-verified
download URL and a valid `source_url`/`author`/`license`/`credits`
triple. See DESIGN.md § "Credit / attribution requirements" — no entry
merges without those four fields populated.

## Phase 1 — External app mods (our forks)

| id | name | source_url | author | license | status |
|----|------|-----------|--------|---------|--------|
| `classicplus.shinra` | Shinra Meter (Classic+) | https://github.com/TERA-Europe-Classic/ShinraMeter | neowutran (upstream), LukasTD fork, TERA-Europe-Classic fork | MIT | CI green; needs first tagged release |
| `classicplus.tcc` | TCC (Classic+) | https://github.com/TERA-Europe-Classic/TCC | Foglio1024 (upstream), TERA-Europe-Classic fork | GPL-3.0 | CI green; needs first tagged release |

`credits` field for `classicplus.shinra`:
> Originally by neowutran; Classic support forked through EU-Classic and LukasTD. This Classic+ variant strips telemetry and hard-codes the mirror sniffer on 127.0.0.1:7803.

`credits` field for `classicplus.tcc`:
> Originally by Foglio1024 (https://github.com/Foglio1024/Tera-custom-cooldowns). This Classic+ variant strips every outbound RPC (LFG, Moongourd, Firebase, Discord webhooks) and replaces the Toolbox sniffer with a read-only mirror reader on 127.0.0.1:7803.

## Phase 2 — GPK community mods

Research pool extracted from the Discord export
`f180f3cf-e445-4f69-8429-c09bce84cd34.htm` (channel "Community UI mods :3"):

- `S1UI_PartyWindow.gpk` (variants: `_1`, `_2`) — custom party window layout
- `S1UI_PartyWindow_1.gpk` / `S1UI_PartyWindow_2.gpk` — alternate party window skins
- `S1UI_PaperDoll.gpk` — character equipment/paperdoll window redesign
- `S1UI_ShortCut.gpk` / `S1UI_ExtShortCut.gpk` — skill hotbar / extended shortcut bar
- `S1UI_GageMonsterHp.gpk` — monster HP gauge style
- `PostProcess.gpk` — screen post-processing tweaks (colorgrade, sharpness)
- `Icon_Skills.gpk` — custom skill icon pack
- `TexturedFonts.gpk` — font substitution
- `NPC_DisPenser.gpk` — NPC dispenser UI customization
- `FX_Awaken_Engineer.gpk` / `FX_E_HotFix_140925.gpk` — effect tweaks

Attribution for every GPK above must be retrieved from the Discord
export by cross-referencing the uploader handle for each attachment.
**Do not add any of these to the catalog without a credit line naming
the original uploader.** If the uploader cannot be identified, leave
the entry out.

Known tooling credit (applies to every GPK entry):
- GPK unpacker/repacker: `lunchduck/GPK_RePack`
  (https://github.com/lunchduck/GPK_RePack) — required to build
  redistributable GPK mods. Install instructions linked from the
  mod detail panel via this tool's `source_url`.
- Parser/reader for UPK/GPK: `vezel-dev/novadrop`
  (https://github.com/vezel-dev/novadrop) — referenced in the Discord
  export as the current de-facto parser.

## Attribution workflow

1. Find the uploader in the Discord export.
2. Confirm the GPK's current hosting (re-uploads frequently shift to
   Discord CDN — prefer a stable GitHub release if the author has one).
3. Fill in `author`, `source_url`, `license` (default to "Unknown" if
   the original post doesn't state one), and `credits`.
4. Open a PR against `external-mod-catalog`.

## License unknowns

Most community GPK mods predate formal licensing. When in doubt,
contact the original uploader and obtain written permission to
redistribute through the launcher. Record the permission in the
`credits` field with a dated quote — e.g.
`"Redistribution with credit permitted by @Username on 2026-04-18."`
