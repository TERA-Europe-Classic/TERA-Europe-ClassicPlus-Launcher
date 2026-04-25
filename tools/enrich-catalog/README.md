# Enrich Catalog

Auto-fills the new schema fields (tagline, featured_image, before_image,
screenshots, long_description, last_verified_patch, gpk_files, tags) from
each mod's `source_url`. Run locally; not shipped to users.

## Requirements
- Python 3.10+
- `gh` CLI authenticated (`gh auth status` should be green)
- `curl`

## Usage
```bash
# Single mod (smoke test)
python tools/enrich-catalog/enrich.py \
    --in /path/to/external-mod-catalog/catalog.json \
    --out /tmp/catalog.proposed.json \
    --only classicplus.shinra

# Full sweep
python tools/enrich-catalog/enrich.py \
    --in /path/to/external-mod-catalog/catalog.json \
    --out /tmp/catalog.proposed.json
```

## Workflow
1. Run the script against `external-mod-catalog/catalog.json`.
2. Diff `catalog.proposed.json` against the input — eyeball every entry.
3. Hand-edit obvious extraction mistakes.
4. Copy `catalog.proposed.json` over `external-mod-catalog/catalog.json`.
5. Bump `version` in the catalog header.
6. Commit + PR to `external-mod-catalog/main`.
