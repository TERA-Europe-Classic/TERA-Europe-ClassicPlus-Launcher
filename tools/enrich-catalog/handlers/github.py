"""Fetch + parse a GitHub README for catalog enrichment."""
from __future__ import annotations

import json
import re
import subprocess
from dataclasses import dataclass, field
from typing import Optional
from urllib.parse import urlparse


@dataclass
class GithubMeta:
    owner: str
    repo: str
    description: str = ""
    last_commit_date: str = ""
    readme_markdown: str = ""
    image_urls: list[str] = field(default_factory=list)
    repo_topics: list[str] = field(default_factory=list)


def parse_repo(url: str) -> Optional[tuple[str, str]]:
    p = urlparse(url)
    if "github.com" not in p.netloc:
        return None
    parts = [s for s in p.path.split("/") if s]
    if len(parts) < 2:
        return None
    return parts[0], parts[1]


def fetch(url: str) -> Optional[GithubMeta]:
    parsed = parse_repo(url)
    if not parsed:
        return None
    owner, repo = parsed

    # Repo description + topics + last commit date
    api = subprocess.run(
        ["gh", "api", f"repos/{owner}/{repo}"],
        capture_output=True, text=True, encoding="utf-8", errors="replace"
    )
    description, topics = "", []
    if api.returncode == 0 and api.stdout:
        body = json.loads(api.stdout)
        description = body.get("description") or ""
        topics = body.get("topics") or []

    last_commit = subprocess.run(
        ["gh", "api", f"repos/{owner}/{repo}/commits", "--jq", ".[0].commit.author.date"],
        capture_output=True, text=True, encoding="utf-8", errors="replace"
    )
    last_commit_date = last_commit.stdout.strip() if last_commit.returncode == 0 and last_commit.stdout else ""

    # GH's /readme endpoint resolves the README regardless of branch or
    # filename casing (Readme.md, readme.MD, README.rst all work). Returns
    # base64-encoded content; jq decodes it.
    readme = ""
    api_readme = subprocess.run(
        ["gh", "api", f"repos/{owner}/{repo}/readme",
         "--jq", ".content | @base64d"],
        capture_output=True, text=True, encoding="utf-8", errors="replace"
    )
    if api_readme.returncode == 0 and api_readme.stdout:
        readme = api_readme.stdout

    images = extract_image_urls(readme, owner, repo)

    return GithubMeta(
        owner=owner, repo=repo,
        description=description,
        last_commit_date=last_commit_date,
        readme_markdown=readme,
        image_urls=images,
        repo_topics=topics,
    )


def extract_image_urls(readme: str, owner: str, repo: str) -> list[str]:
    """Find ![](url) and <img src=...> URLs, normalising relative paths."""
    if not readme:
        return []
    out: list[str] = []
    for m in re.finditer(r"!\[[^\]]*\]\(([^)]+)\)", readme):
        out.append(m.group(1).strip())
    for m in re.finditer(r'<img[^>]+src="([^"]+)"', readme, re.I):
        out.append(m.group(1).strip())
    # Normalise relative
    fixed: list[str] = []
    for u in out:
        if u.startswith(("http://", "https://")):
            fixed.append(u)
        elif u.startswith("/"):
            fixed.append(f"https://raw.githubusercontent.com/{owner}/{repo}/HEAD{u}")
        else:
            fixed.append(f"https://raw.githubusercontent.com/{owner}/{repo}/HEAD/{u.lstrip('./')}")
    # Dedupe preserving order
    seen, deduped = set(), []
    for u in fixed:
        if u not in seen:
            seen.add(u)
            deduped.append(u)
    return deduped
