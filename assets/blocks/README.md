# Discipline block libraries

Plan symbols and construction details for plumbing, HVAC, electrical, structural / RCC and interior
work. Geometry is **1:1 millimetres**. These are ordinary (static) DXF blocks; for blocks whose size
and options change with parameters, see the smart blocks in `../smartblocks/` and
`docs/smart-blocks.md`.

| Folder | Contents |
|---|---|
| `plumbing/` | WCs, urinal, basins, sinks, bath, shower tray, valves, pipe fittings, symbols sheet |
| `hvac/` | AHU, condensing unit, FCU, fans, diffusers, grilles, damper, thermostat, symbols sheet |
| `electrical/` | Switches, switched receptacle, meter, lighting panel, downlights, spotlights, exterior lights, symbol sheets |
| `structural/` | Pad / strip footings, pile cap, raft edge, slab on grade, retaining wall, RC stairs and landing, formwork, steel base plate, end plate, splice and anchor plate details |
| `interior/` | Accessible toilet room, door and window symbol sheets and tags, wardrobe elevation, grab bars, kitchen appliances, door clearances |

Each folder has a `catalog.json` in the same shape as `../furniture/standard/catalog.json` (id,
title, file, category, source). Insert a file with `insert {name, file, at}` or by typing
`INSERT` and giving the path. The Blocks panel lists only the furniture set for now.

## Licence

Every DXF here is an original Innerscene model released under **CC0 1.0** (public domain) in the
[Innerscene Drawing Library](https://www.innerscene.com/tools/library/drawings); the licence of each
item was read from its own page on 2026-10-27. Some sheets carry the text "INNERSCENE / FREE PARTS
LIBRARY / ORIGINAL CC0" as a credit. See `catalog.json` and `ATTRIBUTION.md`.

Drafting symbols only: verify sizes, reinforcement and clearances against the design, the product
and local codes before construction.
