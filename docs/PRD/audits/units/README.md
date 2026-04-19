# Per-Unit Audit Docs — PRD §3.8.7

Every discrete unit (catalog GPK mod, external app, launcher module,
TCC class layout) has a one-page audit doc here. The doc answers:

- **What is this?** Source provenance, license, version.
- **What does it do?** Public surface, settings written, files touched.
- **What could go wrong?** Risks (static-analysis flags, unverified
  upstream, obfuscated binaries, known CVEs).
- **How do we know it's safe?** Tests run, verification commands,
  sign-off status.

The rollout target is in PRD §3.8.7 — **count ≥ 110**:

| Category | Count | Location |
|---|---|---|
| GPK catalog entries | 99 | `gpk/<id>.md` |
| External apps | 2 | `external/<id>.md` |
| Launcher modules | 7 | `launcher/<module>.md` |
| TCC class layouts | 13 | `tcc/<class>.md` |
| **Total floor** | **121** | — |

## Authoring a new audit doc

1. Copy `TEMPLATE.md` into the appropriate subdirectory with the unit
   slug as filename.
2. Fill every section. If a section doesn't apply, write `N/A` with
   a one-line reason — don't delete the header.
3. Sign off with date + verifier in the footer.

The guard `tests/audits_units_coverage.rs` counts files under this
tree and fails if the floor is breached. Current threshold is
relaxed as the rollout progresses; it tightens as each category
completes.

## Rollout status

- [x] Directory structure + README + TEMPLATE (iter 229)
- [x] External: Shinra, TCC (2/2)
- [ ] GPK: 1/99 (psina.postprocess shipped as exemplar)
- [ ] Launcher modules: 0/7
- [ ] TCC class layouts: 0/13
