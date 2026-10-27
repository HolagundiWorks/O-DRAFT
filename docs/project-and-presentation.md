# Project parameters, company details and presentation import

What exists today and what is not built. Status vocabulary: [documentation-policy.md](documentation-policy.md).

## Project parameters (Project panel, `project`)

Implemented and tested (`crates/engine/tests/project_details.rs`): the panel shows (read only) the
twelve project details (project name, client, location, site address, property ID, site area, plot
use, approving authority, architect, engineer, date, summary) and the slab and beam defaults; each
is edited in the Project Wizard ("Edit project..." or File > Project Wizard). `project` also returns `company`, the company details
the drawing holds. Checked by hand in the running shell; there is no automated UI test.

## Company details (`companyfile`)

The OSAS company file (`document_context`, version 1) gives: company name, address, phone, email,
PAN, GSTIN, COA registration, one architect name, the preparation date, and the project's name, code,
site address, summary and client. The panel shows these read only and has a "Read company file"
button (`companyfile {refresh: true}`), which needs `OSAS_COMPANY` and a ticket. It was not tried
against a real company file.

Not available from the company file: **a logo** and **more than one architect**. `CompanyProfile`
(`projectplan/company.rs`) has a logo path and one architect; neither reaches the panel or the title
block. Both need either an extension of `document_context` (an OSAS/O-Studio contract, not in this
repository) or a new profile loader.

## Import for presentation: not built

| Wanted | State |
|---|---|
| PNG, JPEG and other raster images | `EntityKind::Image` is read and written in DXF but drawn only as a frame with a cross (`render/lib.rs`). There is no decoder, no texture path (GPU, raster, PDF, SVG) and no attach command. Needs an image pipeline first |
| Other vector files | DXF and DWG open, and `underlay.attach` reads a PDF page as paths. SVG import does not exist |
| Mood boards | Depends on raster images in layouts |
| Tables from CSV or spreadsheets | `EntityKind::Table` exists and is drawn, but nothing creates one from a file |
| Tables linked to ShilpiDB so a parameter change updates them | Not designed. `vdb.*` exchanges drawing entities, not table values. Needs a decision on what a table cell links to |

Open questions for the owner: which image formats and decoder crate (licence and wasm check), whether
images are embedded in the file or referenced by path, and what a ShilpiDB-linked cell refers to.
