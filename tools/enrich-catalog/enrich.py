#!/usr/bin/env python3
"""Bulk enrichment driver."""
from __future__ import annotations

import argparse
import json
import re
import sys
from datetime import date, datetime
from pathlib import Path
from urllib.parse import urlparse

sys.path.insert(0, str(Path(__file__).parent))
from handlers import github, tumblr  # noqa: E402


def patch_for_date(iso_date: str, ranges: list[dict]) -> str:
    if not iso_date:
        return ""
    try:
        d = datetime.fromisoformat(iso_date.replace("Z", "+00:00")).date()
    except ValueError:
        return ""
    cur = ""
    for r in ranges:
        if d >= date.fromisoformat(r["from"]):
            cur = r["patch"]
    return cur


def is_landscape_image_url(url: str) -> bool:
    u = url.lower()
    if any(t in u for t in ("banner", "hero", "preview", "screenshot", "after")):
        return True
    if any(t in u for t in ("icon", "logo", "thumb", "avatar")):
        return False
    return True


def likely_before_image(url: str) -> bool:
    return any(t in url.lower() for t in ("before", "vanilla", "original"))


def gpk_files_from_text(text: str) -> list[str]:
    if not text:
        return []
    out = set()
    for m in re.finditer(r"\bS1[A-Za-z0-9_]+\.gpk\b", text):
        out.add(m.group(0))
    for m in re.finditer(r"\bPC_Event_\d+\b", text):
        out.add(m.group(0) + ".gpk")
    return sorted(out)


def first_sentence(text: str, max_len: int = 90) -> str:
    s = re.split(r"(?<=[.!?])\s+", (text or "").strip(), maxsplit=1)[0]
    if len(s) > max_len:
        s = s[: max_len - 1].rstrip() + "…"
    return s


def trim_description(text: str, max_paragraphs: int = 3) -> str:
    if not text:
        return ""
    paragraphs = [p.strip() for p in re.split(r"\n\s*\n", text) if p.strip()]
    keep = paragraphs[:max_paragraphs]
    return "\n\n".join(keep)


def enrich_entry(entry: dict, patch_ranges: list[dict]) -> dict:
    src = entry.get("source_url") or ""
    if not src:
        return entry

    host = urlparse(src).netloc.lower()
    meta = None
    if "github.com" in host or "raw.githubusercontent.com" in host:
        meta = github.fetch(src)
    elif "tumblr.com" in host:
        meta = tumblr.fetch(src)

    if not meta:
        return entry

    if not entry.get("featured_image") and meta.image_urls:
        candidates = [u for u in meta.image_urls if is_landscape_image_url(u) and not likely_before_image(u)]
        if candidates:
            entry["featured_image"] = candidates[0]

    if not entry.get("before_image"):
        for u in meta.image_urls:
            if likely_before_image(u):
                entry["before_image"] = u
                break

    if not entry.get("screenshots"):
        used = {entry.get("featured_image"), entry.get("before_image")}
        rest = [u for u in meta.image_urls if u not in used][:8]
        entry["screenshots"] = rest

    if not entry.get("long_description"):
        body = getattr(meta, "readme_markdown", "") or getattr(meta, "body_text", "")
        entry["long_description"] = trim_description(body)

    if not entry.get("tagline"):
        if hasattr(meta, "description") and meta.description and len(meta.description) <= 90:
            entry["tagline"] = meta.description
        else:
            body = getattr(meta, "readme_markdown", "") or getattr(meta, "body_text", "")
            entry["tagline"] = first_sentence(body)

    if not entry.get("last_verified_patch") and getattr(meta, "last_commit_date", ""):
        entry["last_verified_patch"] = patch_for_date(meta.last_commit_date, patch_ranges)

    if not entry.get("gpk_files"):
        body = getattr(meta, "readme_markdown", "") or getattr(meta, "body_text", "")
        entry["gpk_files"] = gpk_files_from_text(body)

    if not entry.get("tags") and getattr(meta, "repo_topics", []):
        entry["tags"] = list(meta.repo_topics)[:5]

    return entry


def main() -> int:
    p = argparse.ArgumentParser()
    p.add_argument("--in", dest="src", required=True, type=Path)
    p.add_argument("--out", dest="dst", required=True, type=Path)
    p.add_argument("--only", default="", help="Comma-separated mod ids to enrich; default = all")
    args = p.parse_args()

    catalog = json.loads(args.src.read_text(encoding="utf-8"))
    patch_ranges = json.loads((Path(__file__).parent / "patch-date-map.json").read_text())["ranges"]

    only = set(filter(None, args.only.split(",")))
    enriched = []
    for entry in catalog["mods"]:
        if only and entry["id"] not in only:
            enriched.append(entry)
            continue
        print(f"-> {entry['id']}", file=sys.stderr)
        enriched.append(enrich_entry(entry, patch_ranges))

    catalog["mods"] = enriched
    args.dst.write_text(json.dumps(catalog, indent=2, ensure_ascii=False), encoding="utf-8")
    print(f"Wrote {args.dst} ({len(enriched)} entries)", file=sys.stderr)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
