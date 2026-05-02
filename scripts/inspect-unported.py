#!/usr/bin/env python3
"""For each currently-unported / x32-marked GPK in the catalog, download the
modded GPK, inspect its export structure, query v100 PkgMapper for the
target package's primary objects, and emit a per-mod diagnostic so we can
add precise overrides (target_object_path + rename) to the orchestrator
instead of marking them x32."""
import json
import re
import subprocess
import sys
import urllib.request
from pathlib import Path

ROOT = Path("C:/Users/Lukas/Documents/GitHub/TERA EU Classic")
LAUNCHER = ROOT / "TERA-Europe-ClassicPlus-Launcher"
CATALOG = Path("C:/Users/Lukas/AppData/Local/Temp/external-mod-catalog/catalog.json")
WORKDIR = Path("C:/Users/Lukas/AppData/Local/Temp/foglio-batch")
GAME = Path("D:/Elinu")
INSPECT = LAUNCHER / "teralaunch/src-tauri/target/release/inspect-gpk-resources.exe"
FIND = LAUNCHER / "teralaunch/src-tauri/target/release/find-current-gpk-mapper.exe"


def http_get(url: str, dest: Path) -> int:
    with urllib.request.urlopen(url, timeout=60) as r:
        d = r.read()
    dest.write_bytes(d)
    return len(d)


def run(cmd: list[str]) -> tuple[str, str]:
    r = subprocess.run(cmd, capture_output=True, text=False)
    return (r.stdout.decode("utf-8", errors="replace") if r.stdout else "",
            r.stderr.decode("utf-8", errors="replace") if r.stderr else "")


def inspect_exports(path: Path) -> tuple[list[str], list[str]]:
    """Return (gfx_movie_info_exports, all_exports) for a GPK file."""
    out, _ = run([str(INSPECT), str(path)])
    gfx, all_exports = [], []
    for line in out.splitlines():
        m = re.match(r"^texture=(\S+)\s", line)
        if m:
            all_exports.append(m.group(1))
            continue
        m = re.match(r"^redirector=(\S+)\s", line)
        if m:
            all_exports.append(m.group(1))
    return gfx, all_exports


def vanilla_pkg_paths(package: str) -> list[str]:
    """Return all S<package>.<X> logical paths in v100 PkgMapper.clean."""
    out, _ = run([str(FIND), str(GAME), package.lower()])
    paths = []
    for line in out.splitlines():
        m = re.match(rf"^PkgMapper\.dat: ({re.escape(package)}\.[^,]+),", line, re.IGNORECASE)
        if m:
            paths.append(m.group(1))
    return paths


def main() -> int:
    c = json.loads(CATALOG.read_text(encoding="utf-8"))
    bad = [m for m in c["mods"]
           if m.get("kind") == "gpk"
           and (m.get("compatible_arch") == "x32"
                or "TERA-Europe-Classic" not in m["download_url"])]
    print(f"Investigating {len(bad)} unported / x32-flagged entries")
    print()

    WORKDIR.mkdir(parents=True, exist_ok=True)
    for m in bad:
        mid = m["id"]
        url = m["download_url"]
        if not url or "TERA-Europe-Classic" in url:
            continue  # already ported, skip
        fname = url.rsplit("/", 1)[-1]
        pkg = fname.replace(".gpk", "")
        target_path = WORKDIR / f"{mid}.x32.gpk"
        try:
            if not target_path.exists() or target_path.stat().st_size < 100:
                http_get(url, target_path)
        except Exception as e:
            print(f"--- {mid} ---")
            print(f"  download FAIL: {e}")
            continue
        # Inspect modded
        out, _ = run([str(INSPECT), str(target_path)])
        modded_class = re.findall(r"^class=(\S+) count=(\d+)", out, re.MULTILINE)
        modded_textures = [m.group(1) for m in re.finditer(r"^texture=([^\s]+)", out, re.MULTILINE)]
        # Inspect vanilla pkg
        vanilla_paths = vanilla_pkg_paths(pkg)
        print(f"--- {mid} ---")
        print(f"  url: {url}")
        print(f"  pkg from filename: {pkg}")
        print(f"  v100 PkgMapper.clean has {len(vanilla_paths)} entries under {pkg}: {vanilla_paths[:5]}{'...' if len(vanilla_paths) > 5 else ''}")
        print(f"  modded classes: {modded_class}")
        if modded_textures:
            print(f"  modded textures (first 5): {modded_textures[:5]}")
        print()
    return 0


if __name__ == "__main__":
    sys.exit(main())
