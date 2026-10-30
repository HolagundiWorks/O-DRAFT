# OSAS suite: names, roles, files, links and how it works live

**Canonical copy:** the O-Studio repository, `docs/SUITE.md`. Each app repository carries an identical copy at `docs/SUITE.md` so a developer in any one repo knows what the others are, what they are called, what their toolbars hold and how they connect. If you change one, change all four. Updated 2026-10-30.

## 1. Names

The suite is **OSAS, the Open Source Architecture Suit**. A name below is the name to use in text, menus and installers. Code identifiers keep older spellings because files already use them; they are identifiers, not names.

| Name to use | What it is | Colour | Repository / folder | Identifiers that keep old spellings |
|---|---|---|---|---|
| **OSAS** | The launcher and the suite's single company sign-in | none | `OSAS-Open_Source_Architecture_Suit` | `Osas.Core`, `Osas.Launcher`, `osas-suite-1.0.x` |
| **O-Studio** | Office and project management: clients, projects, letters, invoices, accounts, staff, approvals. Owns the company file. Will own the cloud sync | Orange | `O-STUDIO` | `aos`, `AOSENC1`, `office-core`, `office.dll` |
| **O-CoreOp** | Quantities, estimates, rate books, bar bending, BOQ, purchase orders, running bills, scheduling | Blue | `O-COREOP` | `AQCCore.exe` (old), `bbs_engine.dll`, `.bbsproj` (old project files), `aqc` |
| **O-Draft** | 2D CAD: drafting, DXF/DWG, layouts, plotting, architecture tools and the take-off tools | Black | `O-DRAFT` | `aad-*` crates, `aad-host.dll`, `aad-winui` |
| **O-UI** | Shared WinUI themes, marks and chrome (`Hcw.Ui`) | | `o-ui` | `Hcw.Ui` |
| **O-Form**, **O-Dynamo** | Planned, not built | | `O-Form`, `O-Dynamo` | |

Other names:

| Term | Meaning |
|---|---|
| **Company file** | One file per practice, `.osas` (earlier `.adms` opens too). SQLite, optionally encrypted (`AOSENC1`). Chosen and created only in OSAS; opened read-write by O-Studio, read-only for details by O-CoreOp and O-Draft |
| **Project file** | `.osp`. One SQLite file per project holding the drawing, the take-off and the project record. Format name `osas-project`, version 1. Replaces ShilpiDB for projects. (`.odp` was the working name and clashed with LibreOffice Impress; it is not used) |
| **Take-off** | Quantities measured from a drawing. Recorded by O-Draft into the project file; read by O-CoreOp |
| **Revision** | The counter inside a project file (`meta.revision`). Every save by any program is one revision. Apps compare revisions to know someone else wrote |
| **Active-project pointer** | `%LOCALAPPDATA%\OSAS\active-project.json`, written by O-Draft: which project file is open there, so O-CoreOp can open the same one |
| **Ticket** | One-time sign-in token OSAS gives an app it starts: 32 random bytes, two minutes, one use. The password is never passed on |
| **Persona** | In O-CoreOp, Project Manager/PMC or Contractor: each has its own letterhead and numbering |
| **ShilpiDB** | The old spatial store (`.vdb`, `shilpid`). **Retired and removed** from O-Draft, O-CoreOp, OSAS and the installers. No conversion exists or is needed: no production project was ever created in it. Its record layout survives as the take-off tables of the `.osp` |

## 2. Hierarchy

```
OSAS (launcher, company sign-in)                          <- which company, who is signed in
 |
 +-- Company file (.osas)                                 <- the practice: clients, projects, users, accounts
 |     |
 |     +-- Project (ProjectRef: company_uuid + project_id)  <- identity shared by all apps
 |           |
 |           +-- Project file (.osp)                        <- the working data of one project
 |                 |-- drawing(s)  (DWG/DXF bytes, unchanged)
 |                 |-- take-off    (headers, rows, annotated PDFs)
 |                 |-- project record (estimate, bills, tasks, quantities, POs)
 |                 |-- markups, history, change log
 |
 +-- O-Studio   edits the company file; later syncs .osp files to the cloud
 +-- O-CoreOp   reads company details; reads/writes the project record and take-off view in the .osp
 +-- O-Draft    writes the drawing and take-off in the .osp
 +-- Viewer (planned: tablet/phone)  views and marks up the synced project
```

Rule: **a project's data lives in its `.osp`; the practice's data lives in the company file; nothing else is a source of truth.** Cloud (Supabase) is a master copy that O-Studio syncs; the apps never hold cloud keys.

## 3. Roles

Users belong to the company, so they are the same in every app. Roles are in `office-core` (`auth.rs`):

| Role | Can |
|---|---|
| **Owner** | Everything: settings, users, backup, post, close periods, issue, view audit |
| **Principal** | View, issue |
| **Accounts** | View, issue, post |
| **Document editor** | View and edit documents |
| **Auditor** | View, and view the audit trail (read only) |

Users are added, re-roled and re-passworded in OSAS (Company > Users), each change asking for the owner's password again; the last active owner cannot be removed. In O-CoreOp the company file is read-only details (name, address, GSTIN, letterhead); the project's persona (PM/PMC or Contractor) is separate from the user's role.

## 4. What each app has on screen

### O-Draft (WinUI shell over a Rust engine)

| Region | Contents |
|---|---|
| Menu bar | File, Edit, View, Insert, Format, Tools, Draw, Dimension, Modify, Architecture, Window, Help |
| Left tool stack | Draw, Modify, Annotate toolbars (vertical) |
| Top strip | Layer box, architecture tools, gear (View > Customize Top Bar). **Take-off tools (Ext wall, Int wall, Beam, Floor, Ceiling, Column, Deduct, Paint, Openings, Record, Map layers) show only while Take-off is on** |
| Right | Layers / Properties / Blocks tabbed stack |
| Bottom bar | Snap/ortho/polar toggles, coordinates, units, **Take-off toggle** (next to Nameplate) |
| File | Open/Save as `.dwg`, `.dxf` or **`.osp`** (Project: drawing and take-off) |
| Command line | Every tool is also a typed command, e.g. `takeoff.record` |

Details: `docs/UI_UX.md`.

### O-CoreOp (WinUI shell, C++ engine)

| Region | Contents |
|---|---|
| Main ribbon | One text tab per section: Project, RCC steel, Civil BOQ, Schedule, Contracts, Stores, Outputs (Ctrl+1 to Ctrl+7). Office, Accounts and HR live in O-Studio |
| Sub ribbon | One text tab per page of the open section |
| Take-off page | Reads the take-off from the open `.osp`; shows "O-Draft updated the take-off" when O-Draft writes; Import brings it into the estimate |
| File | Open/Save as `.osp` (older `.bbsproj` and `.json` still open; Save as puts them in `.osp`). No company menu: the company comes from OSAS/O-Studio |
| Floating AI bar | Local Olmo 3 7B (shared with O-Draft), opt-in |

Details: `docs/WINUI-SHELL.md`.

### O-Studio (WinUI shell over a Rust engine)

| Region | Contents |
|---|---|
| Left pane | Practice (Pulse, Clients, Projects, enquiries, site records, permits, proposals, meetings); Documents (letters, templates); Billing (invoices, receipts, reconciliation); Books (expenses, income, salaries, assets, journals); Reports and tax; Office (firm profile, staff, leave, attendance, library, security, backup) |
| Shortcuts | Ctrl+N new record, Ctrl+S save, Ctrl+P print, Ctrl+F search, Ctrl+K go to page |
| Planned | Cloud connector: sign in, push/pull projects by revision, conflicts, realtime |

### OSAS (launcher)

Cards for each installed app with version and Open; Company section (choose/create company, sign in once, company details, users); company-file monitor.

## 5. How they are linked, and how it works live

| Link | Mechanism | Latency |
|---|---|---|
| OSAS -> apps | Starts the app on the company file with a one-time ticket | on launch |
| Company -> apps | `office.dll` (C ABI from `office-ffi`), read by the app | on open |
| O-Draft -> O-CoreOp | Both open the **same `.osp`**. O-Draft's `takeoff.record` writes headers/rows/annotated PDF in one revision; O-CoreOp polls `meta.revision` every 2 s and notifies | about 2 s, no server |
| O-Draft -> O-CoreOp (which file) | Active-project pointer: `{format:"osas-active-project", app, drawing, project, updated_utc}` | on drawing change |
| Two programs, one file | SQLite WAL, immediate write transactions, 5 s busy timeout: writers take turns, every commit is exactly one revision, readers never block (tested with 4 writers x 40 commits) | immediate |
| Machine -> machine | O-Studio syncs `.osp` with Supabase by revision (**planned**); a viewer on tablet/phone reads what Studio pushed and sends markups back | realtime via Supabase |

Both apps share one format library: Rust `aad-odp` (O-Draft) and .NET `Office.Shared.OdpFile` (O-CoreOp), kept compatible by tests that read each other's files.

Saving: O-Draft writes the drawing back into the `.osp` only when its bytes changed (so a take-off-only change does not touch the drawing), and records take-off changes as differences only.

## 6. Where to read next

| Topic | Document |
|---|---|
| Project file format and sync plan | O-Studio `docs/ODP-PROJECT-FILE-AND-SYNC.md` (filename keeps the working name) |
| Suite architecture and integration map | O-Studio `docs/suite-architecture.md`, `docs/suite-hub.md` |
| Company file ABI | O-Studio `docs/office-abi.md` |
| O-Draft UI, commands, performance | O-Draft `docs/UI_UX.md`, `docs/PERFORMANCE.md`, `docs/PERFORMANCE-AUDIT-2026-10-30.md` |
| O-CoreOp shell, model, take-off | O-CoreOp `docs/WINUI-SHELL.md`, `docs/LOCAL-MODEL.md`, `docs/PROJECT-FILE.md` |
| Installers | O-Studio `installer/suite/README.md`; O-CoreOp `docs/PACKAGING.md`, `docs/SUITE-PACKAGING.md` |

## 7. Conventions

- Write the app names as O-Studio, O-CoreOp, O-Draft, OSAS. Never AQC, AOS, AAD in user text.
- The project file is `.osp`. The company file is `.osas`.
- A new feature that stores project data adds a table to the `.osp`, with a change-log row, never a new file next to it.
- Apps do not call each other; they meet in the project file, the company file or the pointer.
- Cloud keys and sign-in live only in O-Studio.
