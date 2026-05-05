"""One-shot helper: removes redundant `#[allow(dead_code)]`,
`#[allow(dead_code, unused_imports)]`, and similar outer attributes
that immediately precede a `#[path = "../...../X.rs"] mod X;` line in
bin and integration-test files. The included files now carry their
own inner `#![allow(dead_code)]`, so duplicating it on the include
site triggers `clippy::duplicated_attributes`.

Run via: `python scripts/strip-redundant-allows.py`. Idempotent.
"""

import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
TARGETS = [
    ROOT / "teralaunch" / "src-tauri" / "tests",
    ROOT / "teralaunch" / "src-tauri" / "src" / "bin",
]

# Match `#[allow(dead_code)]` or `#[allow(dead_code, unused_imports)]`
ALLOW_PATTERNS = [
    re.compile(r"^#\[allow\(dead_code(?:, unused_imports)?\)\]\n(?=#\[path = \"\.\.[/\\])", re.MULTILINE),
    re.compile(r"#\[allow\(dead_code(?:, unused_imports)?\)\]\s+(?=#\[path = \"\.\.[/\\])"),
    re.compile(r"(^#\[cfg\(test\)\]\n)#\[allow\(dead_code(?:, unused_imports)?\)\]\n(?=#\[path = \"\.\.[/\\])", re.MULTILINE),
]


def strip_file(path: pathlib.Path) -> bool:
    src = path.read_bytes().decode("utf-8", errors="replace")
    new = src
    new = ALLOW_PATTERNS[0].sub("", new)
    new = ALLOW_PATTERNS[1].sub("", new)
    new = ALLOW_PATTERNS[2].sub(r"\1", new)
    if new != src:
        path.write_bytes(new.encode("utf-8"))
        return True
    return False


def main() -> int:
    edited = 0
    for target in TARGETS:
        if not target.is_dir():
            continue
        for path in sorted(target.glob("*.rs")):
            if strip_file(path):
                edited += 1
                print(f"edited {path.relative_to(ROOT)}")
    print(f"-- {edited} file(s) updated")
    return 0


if __name__ == "__main__":
    sys.exit(main())
