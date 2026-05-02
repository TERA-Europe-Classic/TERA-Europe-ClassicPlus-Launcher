#!/usr/bin/env python3
"""
foglio-batch-port.py — port every catalog foglio.* mod that's still pointing at
foglio's raw x32 GitHub URL to a v100 (x64) artifact published as a release
asset on TERA-Europe-Classic/external-mod-catalog.

Pipeline per mod:
  1. Download foglio's x32 source GPK from the catalog's current download_url.
  2. Extract the v100 vanilla composite slice for the mod's target_object_path
     (using extract-vanilla-slice-raw, which reads from D:/Elinu).
  3. Run splice-x32-payloads --gfx-swap to splice foglio's mod.gfx into the
     vanilla wrapper, output a roundtrip x64 GPK.
  4. SHA256 + size the output.
  5. Upload to the foglio-x64-port-batch-2026-05-01 release.
  6. Update the catalog entry with download_url, sha256, size_bytes,
     compatible_arch=x64, target_object_path, version.

After all mods are processed, push catalog.json once.

Skips mods whose target_object_path can't be auto-derived (Type D / no
v100 vanilla baseline / multi-package).
"""
import hashlib
import json
import shutil
import subprocess
import sys
import urllib.request
from pathlib import Path
from typing import Optional

ROOT = Path("C:/Users/Lukas/Documents/GitHub/TERA EU Classic")
LAUNCHER = ROOT / "TERA-Europe-ClassicPlus-Launcher"
CATALOG_REPO = Path("C:/Users/Lukas/AppData/Local/Temp/external-mod-catalog")
GAME_ROOT = Path("D:/Elinu")
WORKDIR = Path("C:/Users/Lukas/AppData/Local/Temp/foglio-batch")
RELEASE_TAG = "foglio-x64-port-batch-2026-05-01"

SLICE_BIN = LAUNCHER / "teralaunch/src-tauri/target/release/extract-vanilla-slice-raw.exe"
SPLICE_BIN = LAUNCHER / "teralaunch/src-tauri/target/release/splice-x32-payloads.exe"
SPLICE_SHIFT_BIN = LAUNCHER / "teralaunch/src-tauri/target/release/splice-shift-aware.exe"


# Cache: package_name (lowercase) -> first matching logical path in v100 PkgMapper.clean
_PKG_FIRST_OBJECT: Optional[dict[str, str]] = None


def build_package_first_object_map() -> dict[str, str]:
    """Decrypt PkgMapper.clean, return {package_name_lower: first_full_logical_path}.
    Used as a fallback when our naming convention can't guess the primary object."""
    pkg_clean = GAME_ROOT / "S1Game/CookedPC/PkgMapper.clean"
    enc = pkg_clean.read_bytes()
    KEY1 = [12, 6, 9, 4, 3, 14, 1, 10, 13, 2, 7, 15, 0, 8, 5, 11]
    KEY2 = b"GeneratePackageMapper"
    out = bytearray(enc)
    n = len(out)
    blocks = n // 16
    for b in range(blocks):
        off = b * 16
        original = bytes(out[off:off + 16])
        for i in range(16):
            out[off + i] = original[KEY1[i]]
    a, b = 1, n - 1
    iters = (n // 2 + 1) // 2
    for _ in range(iters):
        if a < n and b < n:
            out[a], out[b] = out[b], out[a]
            a += 2
            b -= 2
    for i in range(n):
        out[i] ^= KEY2[i % 21]
    if len(out) >= 2 and out[0] == 0xFF and out[1] == 0xFE:
        out = out[2:]
    plain = bytes(out).decode("utf-8", errors="replace")
    result: dict[str, str] = {}
    for cell in plain.split("|"):
        cell = cell.strip()
        if not cell or "," not in cell:
            continue
        uid = cell.split(",", 1)[0]
        if "." not in uid:
            continue
        pkg = uid.split(".", 1)[0].lower()
        if pkg not in result:
            result[pkg] = uid
    return result


def derive_target_object_path(mod_id: str, gpk_filename: str) -> Optional[str]:
    """
    Return the full Package.Object logical path for the mod's primary widget.

    Convention: foglio's restyle-* and modern-ui-* mods replace the main
    widget object inside an S1UI_<Widget>.gpk package. The object name is
    USUALLY <Widget> (the package name without the S1UI_ prefix), but a few
    v100 packages name the widget differently (e.g. S1UI_InventoryWindow's
    widget is `Inventory`, S1UI_PartyWindowRaidInfo's is
    `PartyWindowRaidInfo`).

    Returns None for mods whose target can't be derived (forces a skip).
    """
    # Hand-curated overrides for v100 widget naming exceptions (verified
    # against PkgMapper.clean).
    OVERRIDES = {
        # mod_id → target_object_path
        "foglio1024.restyle-inventory": "S1UI_InventoryWindow.Inventory",
        "foglio1024.modern-ui-jewels-fix-inventory": "S1UI_InventoryWindow.Inventory",
        # toolbox-thinkblob targets a compound 3-part logical path
        # (the v100 PkgMapper for Awaken_SpiritKing uses
        # `Package.Skel.Object` not the simple `Package.Object` form).
        "foglio1024.toolbox-thinkblob": "Awaken_SpiritKing.Skel.Awaken_SpiritKing_Skel",
        # taorelia: foglio source has 'Inventory' (no _dup); v100 routes
        # InventoryWindow widget via S1UI_InventoryWindow.Inventory.
        "taorelia.restyle-inventory": "S1UI_InventoryWindow.Inventory",
        # teralove.targetinfo: target the TargetInfo widget directly so the
        # rename TargetInfo=TargetInfo_dup picks up the GFxMovieInfo.
        "teralove.targetinfo": "S1UI_TargetInfo.TargetInfo",
        # deathdefying catalog data quality: URL files are actually ProgressBar
        # mods (FlightGauge-style removers). Re-target to the real widget.
        "deathdefying.ui-remover-quest-tracker": "S1UI_ProgressBar.ProgressBar",
        "deathdefying.ui-remover-tera-rewards": "S1UI_ProgressBar.ProgressBar",
    }
    if mod_id in OVERRIDES:
        return OVERRIDES[mod_id]

    pkg_name = gpk_filename.replace(".gpk", "")
    if pkg_name == "Icon_Items":
        return None  # multi-thousand-object package; out of scope
    if pkg_name == "TexturedFonts":
        return None  # not in v100 vanilla

    global _PKG_FIRST_OBJECT
    if _PKG_FIRST_OBJECT is None:
        _PKG_FIRST_OBJECT = build_package_first_object_map()

    # Helper: check whether a logical path actually exists in PkgMapper.clean.
    def has_path(logical: str) -> bool:
        pkg, _, _ = logical.partition(".")
        first = _PKG_FIRST_OBJECT.get(pkg.lower())
        if first is None:
            return False
        # Either it's a hit (path found verbatim) or any path in this package
        # would do — but we only stored ONE per package, so check the package
        # has SOME path. The actual case-sensitive lookup happens later in the
        # extract step; this just gates "is the package mapped at all".
        return True

    # Try Package.WidgetName conventions (strip common prefixes).
    convention_candidates: list[str] = []
    if pkg_name.startswith("S1UI_"):
        convention_candidates.append(f"{pkg_name}.{pkg_name[5:]}")
    if pkg_name.startswith("S1Data_"):
        convention_candidates.append(f"{pkg_name}.{pkg_name[7:]}")
    if pkg_name.startswith("Modern_"):
        convention_candidates.append(f"{pkg_name}.{pkg_name[7:]}")
    convention_candidates.append(f"{pkg_name}.{pkg_name}")  # Package.Package

    # If any convention is in the smart map (i.e. the package is in
    # PkgMapper at all), prefer the package's actual first-object instead
    # of the convention guess (which might not exist).
    pkg_lower = pkg_name.lower()
    if pkg_lower in _PKG_FIRST_OBJECT:
        return _PKG_FIRST_OBJECT[pkg_lower]

    # Package not in PkgMapper at all — return the convention so the
    # caller logs a clear error.
    return convention_candidates[0]


def derive_gpk_filename(catalog_entry: dict) -> Optional[str]:
    """Get the main GPK filename for the mod, used to determine target widget."""
    if catalog_entry.get("gpk_files"):
        return catalog_entry["gpk_files"][0]
    # Fallback: parse from URL
    url = catalog_entry.get("download_url", "")
    return url.rsplit("/", 1)[-1] if url else None


def http_get(url: str, dest: Path) -> int:
    with urllib.request.urlopen(url, timeout=60) as resp:
        data = resp.read()
    dest.write_bytes(data)
    return len(data)


def run(cmd: list[str], cwd: Optional[Path] = None) -> tuple[int, str, str]:
    r = subprocess.run(cmd, cwd=cwd, capture_output=True, text=True)
    return r.returncode, r.stdout, r.stderr


def upload_release_asset(local_path: Path) -> bool:
    code, out, err = run([
        "gh", "release", "upload", RELEASE_TAG, str(local_path),
        "--repo", "TERA-Europe-Classic/external-mod-catalog",
        "--clobber",
    ])
    return code == 0


def main() -> int:
    WORKDIR.mkdir(parents=True, exist_ok=True)

    catalog_path = CATALOG_REPO / "catalog.json"
    catalog = json.loads(catalog_path.read_text(encoding="utf-8"))

    # ALL unported GPK mods (not just foglio). Anything where:
    #   kind == 'gpk' AND download_url doesn't already point to our release CDN
    #   AND not already explicitly marked compatible_arch=x32
    foglio = [
        m for m in catalog["mods"]
        if m.get("kind") == "gpk"
        and "TERA-Europe-Classic" not in m["download_url"]
        and m.get("compatible_arch") != "x32"
    ]
    print(f"Catalog has {len(foglio)} unported GPK entries (any author)")

    successes: list[tuple[str, dict]] = []
    skips: list[tuple[str, str]] = []

    for entry in foglio:
        mod_id = entry["id"]
        gpk_filename = derive_gpk_filename(entry)
        if not gpk_filename:
            skips.append((mod_id, "no gpk_files / download_url"))
            continue
        target_object_path = derive_target_object_path(mod_id, gpk_filename)
        if not target_object_path:
            skips.append((mod_id, f"target_object_path undeterminable for {gpk_filename}"))
            continue
        package_name = gpk_filename.replace(".gpk", "")

        print(f"\n=== {mod_id} → {target_object_path} ===")
        # 1. Download foglio source
        x32_path = WORKDIR / f"{mod_id}.x32.gpk"
        try:
            n = http_get(entry["download_url"], x32_path)
            print(f"  downloaded {n} bytes from {entry['download_url']}")
        except Exception as e:
            skips.append((mod_id, f"download failed: {e}"))
            continue

        # 2. Extract vanilla x64 slice
        vanilla_path = WORKDIR / f"{mod_id}.vanilla-slice.gpk"
        code, out, err = run([
            str(SLICE_BIN),
            "--game-root", str(GAME_ROOT),
            "--logical", target_object_path,
            "--out", str(vanilla_path),
        ])
        if code != 0:
            skips.append((mod_id, f"slice extract failed: {err.strip()[:200]}"))
            continue
        print(f"  extracted vanilla slice: {vanilla_path.stat().st_size} bytes")

        # 3. Splice (with two passes: first try plain, then retry with
        # `<Widget>=<Widget>_dup` rename — foglio's UI-Remover and some
        # other repos name the SWF widget without the composite `_dup`
        # suffix that the vanilla slice uses).
        out_path = WORKDIR / f"{mod_id}.{package_name}.roundtrip.gpk"
        widget = target_object_path.split(".", 1)[1] if "." in target_object_path else ""
        # Some v100 vanilla composite slices store the widget GFxMovieInfo
        # export with the widget name lowercased (e.g. partywindow_dup
        # rather than PartyWindow_dup). Also try the all-lowercase rename.
        wl = widget.lower()
        attempts = [
            ([], "no-rename"),
            (["--rename", f"{widget}={widget}_dup"], f"rename {widget}={widget}_dup"),
            (["--rename", f"{widget}={wl}_dup"], f"rename {widget}={wl}_dup"),
            # Some foglio sources have the modded export with widget name
            # already lowercased (e.g. UI-Remover party-window has
            # `partywindow` not `PartyWindow`). Match those too.
            (["--rename", f"{wl}={wl}_dup"], f"rename {wl}={wl}_dup"),
        ]
        spliced = False
        last_err = ""
        for extra_args, label in attempts:
            code, out, err = run([
                str(SPLICE_BIN),
                "--vanilla-x64", str(vanilla_path),
                "--modded-x32", str(x32_path),
                "--output", str(out_path),
                "--mod-id", mod_id,
                "--gfx-swap",
                *extra_args,
            ])
            if code == 0:
                if extra_args:
                    print(f"  splice succeeded with {label}")
                spliced = True
                break
            last_err = (err or out).strip().split("\n")[-3:]
            last_err = " | ".join(last_err)[:300]
        if not spliced:
            # Fallback: shift-aware splice for tables-in-middle layouts
            # (e.g. servant-storage where some bodies live after the export
            # table). Tries the same rename combinations.
            shift_attempts = [
                ([f"--rename={widget}={widget}_dup"], f"shift {widget}={widget}_dup"),
                ([f"--rename={widget}={wl}_dup"], f"shift {widget}={wl}_dup"),
                ([f"--rename={wl}={wl}_dup"], f"shift {wl}={wl}_dup"),
            ]
            # Try multiple target_export forms with each rename combination,
            # preferring widget casing variants.
            target_variants = [
                f"{widget}_dup",
                f"{wl}_dup",
                widget,
                wl,
            ]
            for target_form in target_variants:
                if spliced: break
                for extra_args, label in shift_attempts:
                    rename_pair = extra_args[0].replace("--rename=", "")
                    code, out, err = run([
                        str(SPLICE_SHIFT_BIN),
                        "--vanilla-x64", str(vanilla_path),
                        "--modded-x32", str(x32_path),
                        "--output", str(out_path),
                        "--target-export", target_form,
                        "--rename", rename_pair,
                    ])
                    if code == 0:
                        print(f"  shift-aware splice succeeded: target='{target_form}', {label}")
                        spliced = True
                        break
                    last_err = (err or out).strip().split("\n")[-3:]
                    last_err = " | ".join(last_err)[:300]
        if not spliced:
            skips.append((mod_id, f"splice failed: {last_err}"))
            continue

        # 4. Hash + record
        data = out_path.read_bytes()
        sha = hashlib.sha256(data).hexdigest()
        size = len(data)
        print(f"  produced {out_path.name}: {size} bytes, sha={sha[:12]}")

        # 5. Upload
        ok = upload_release_asset(out_path)
        if not ok:
            skips.append((mod_id, "upload failed"))
            continue

        url = f"https://github.com/TERA-Europe-Classic/external-mod-catalog/releases/download/{RELEASE_TAG}/{out_path.name}"
        successes.append((mod_id, {
            "download_url": url,
            "sha256": sha,
            "size_bytes": size,
            "target_object_path": target_object_path,
        }))

    print(f"\n=== SUMMARY ===")
    print(f"successes: {len(successes)}")
    print(f"skips: {len(skips)}")
    for mid, reason in skips:
        print(f"  SKIP {mid}: {reason}")

    # 6. Update catalog entries
    by_id = dict(successes)
    now = "2026-05-01T23:00:00Z"
    updated = 0
    for entry in catalog["mods"]:
        if entry["id"] not in by_id:
            continue
        upd = by_id[entry["id"]]
        entry["download_url"] = upd["download_url"]
        entry["sha256"] = upd["sha256"]
        entry["size_bytes"] = upd["size_bytes"]
        entry["target_object_path"] = upd["target_object_path"]
        entry["compatible_arch"] = "x64"
        entry["version"] = "2026-05-01-x64-port"
        entry["updated_at"] = now
        notes = entry.get("compatibility_notes") or ""
        if "Adapted for x64" not in notes:
            entry["compatibility_notes"] = (notes + " Adapted from foglio's x32 mod by TERA-Europe-Classic for v100.02 (x64). May not look exactly as foglio intended.").strip()
        credits = entry.get("credits") or ""
        if "TERA-Europe-Classic" not in credits:
            entry["credits"] = (credits + " Adapted to v100.02 (x64) by TERA-Europe-Classic.").strip()
        updated += 1
    catalog["updated_at"] = now
    catalog_path.write_text(json.dumps(catalog, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
    print(f"\nUpdated {updated} catalog entries (paperdoll + 4 prebuilts already done in prior commits)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
