"""Fetch + parse a Tumblr post for catalog enrichment."""
from __future__ import annotations

import re
import subprocess
from dataclasses import dataclass, field
from typing import Optional
from urllib.parse import urlparse


@dataclass
class TumblrMeta:
    url: str
    body_text: str = ""
    image_urls: list[str] = field(default_factory=list)


def fetch(url: str) -> Optional[TumblrMeta]:
    p = urlparse(url)
    if "tumblr.com" not in p.netloc:
        return None
    r = subprocess.run(
        ["curl", "-fsSL", "-A", "Mozilla/5.0 (compatible; mod-catalog-enricher)", url],
        capture_output=True, text=True, encoding="utf-8", errors="replace"
    )
    if r.returncode != 0 or not r.stdout:
        return None
    html = r.stdout

    images = re.findall(r'<img[^>]+src="(https://[^"]+\.(?:png|jpg|jpeg|webp|gif))"', html, re.I)
    seen, deduped = set(), []
    for u in images:
        if u in seen:
            continue
        seen.add(u)
        deduped.append(u)

    body_match = re.search(r'<article[^>]*>(.*?)</article>', html, re.S)
    body_html = body_match.group(1) if body_match else ""
    body_text = re.sub(r"<[^>]+>", "", body_html or "")
    body_text = re.sub(r"\s+", " ", body_text).strip()[:1200]

    return TumblrMeta(url=url, body_text=body_text, image_urls=deduped)
