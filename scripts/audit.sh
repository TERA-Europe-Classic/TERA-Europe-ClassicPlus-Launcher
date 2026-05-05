#!/usr/bin/env bash
# Run `cargo audit -D warnings` on every Rust crate in the workspace,
# parsing each crate's `audit.toml` for documented advisory ignores.
#
# Pass criterion: every invocation exits 0 with no findings.
#
# Usage:
#   bash scripts/audit.sh
#
# Exit codes:
#   0  — all crates clean under -D warnings
#   1  — a crate surfaced an advisory not present in its audit.toml ignore
#        list, OR a brand-new vulnerability was published since the last
#        review of audit.toml. Either way: investigate before silencing.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CRATES=(
  "teralib"
  "teralaunch/src-tauri"
)

extract_ignores() {
  # Pull RUSTSEC-YYYY-NNNN identifiers out of an audit.toml's ignore = [...]
  # block. Tolerates trailing commas, comments, and varied whitespace.
  local toml="$1"
  if [[ ! -f "$toml" ]]; then
    return
  fi
  grep -oE '"RUSTSEC-[0-9]{4}-[0-9]+"' "$toml" | tr -d '"' | sort -u
}

run_one() {
  local crate_dir="$1"
  local toml="$REPO_ROOT/$crate_dir/audit.toml"
  local ignore_args=()

  local ignore_count=0
  while IFS= read -r id; do
    if [[ -n "$id" ]]; then
      ignore_args+=("--ignore" "$id")
      ignore_count=$((ignore_count + 1))
    fi
  done < <(extract_ignores "$toml")

  echo "==> cargo audit -D warnings (crate: $crate_dir, ignored: $ignore_count advisories)"
  (cd "$REPO_ROOT/$crate_dir" && cargo audit -D warnings "${ignore_args[@]}")
}

for crate in "${CRATES[@]}"; do
  run_one "$crate"
done

echo "==> all crates clean."
