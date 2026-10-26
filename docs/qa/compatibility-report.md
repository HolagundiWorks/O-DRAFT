# Compatibility report (DXF / DWG)

## Campaign status

**Incomplete.** Automated IO unit tests re-run on this machine; no new large-drawing corpus
walk or reference-application visual comparison was completed in this campaign.

## Automated (measured 2026-10-26)

| Check | Result |
|---|---|
| `cargo test -p aad-io --lib` | 56 passed after QA-2026-002 |
| AC1024 DWG many entities (in suite) | listed as passing in suite output |
| Hostile / oversized rejection tests | passing in suite output |

## Classification (from product docs — not re-validated here)

See [status.md](../status.md) and [KNOWN_ISSUES.md](../KNOWN_ISSUES.md):

| Feature | Class |
|---|---|
| DXF open/save | Fully supported (engine) |
| DWG via isolated worker | Partially supported; ~1.2–1.5 s large open; Job memory cap |
| Dynamic blocks | Not claimed fully editable |
| 3D solids / meshes | Preserved/reported, not modelled |
| SHX fonts | Unsupported |

## Not run this campaign

- Round-trip of `examples/apartment.dxf` with entity-count diff report
- Visual compare against a reference CAD application
- Mutation robustness extended run (`AAD_MUTATION_ITERS`)
- DWG isolation suite (`cargo test -p aad-cli --test dwg_isolation`)
