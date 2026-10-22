# blackcad-native: BlackCAD's engine under CADCraft's interface

Status: started 2026-10-22. Branch `blackcad-native`, from `main`. Experimental.

## The idea

CADCraft's interface (command line, prompts, menus, tool sets, the AutoCAD workflow) on top of
BlackCAD's Rust engine instead of CADCraft's own. In BlackCAD's engine the drawing *is* a DWG
database: every object is kept by its DWG handle, DWG, DXF and `.cad` are read and written
directly with no conversion between them, and objects the engine doesn't decode are kept as their
own DWG bytes and written back.

The other two projects on this fork keep CADCraft's engine:

```
main
 ├─ dwg-native          full DWG-native file layer; CADCraft's model untouched
 │    └─ cad-native     CADCraft's own high-fidelity native format
 └─ blackcad-native     this branch: BlackCAD's engine, CADCraft's interface
```

This branch shares no commits with the other two after `main`. It pulls from `main` for CADCraft's
interface; BlackCAD's own progress arrives by moving the pin in `Cargo.toml` forward.

`main` is a mirror of storytold/cadcraft and is never committed to, so syncing it is always a
fast-forward. After each sync, `main` is merged into this branch (and into `dwg-native`, then
`dwg-native` into `cad-native`); conflicts are resolved here, never on `main`.

## What's imported from withshalom/CAD

All by git dependency, pinned to one BlackCAD commit (`f5dc64e`, BlackCAD 0.15.0), never copied
into this repository:

| Crate | Licence | What it is |
|---|---|---|
| `cad-core` | proprietary | the engine: drafting operations (modify, snap, grips, hatch boundaries, blocks, layouts, dimensions), rendering, SVG/PDF/PNG export, units, standards and templates |
| `cad-model` | MIT OR Apache-2.0 | the drawing database (objects by handle, tables, undo journal) |
| `cad-dwg` | MIT OR Apache-2.0 | DWG reader (R13 to 2018; 2007 pending) and writer (2018) |
| `cad-dxf` | MIT OR Apache-2.0 | DXF reader and writer, ASCII and binary |
| `cad-native` | MIT OR Apache-2.0 | the `.cad` format, `.cdl` locks, compression and checksums |

The open crates were already dependencies of `main` (`cad-dwg`, `cad-dxf`, `cad-model`); this
branch moves them from BlackCAD 0.8.0 to 0.15.0 so all five come from one commit. Since 0.13.0
the model also holds tolerances, raster images and wipeouts, and multilines (`.cad` 2.9).

Not imported: `cad-ffi` (BlackCAD's bridge to the Android and web apps, with its `Session` and
command line). It contains BlackCAD's only `unsafe` code and exports platform entry points, which
CADCraft doesn't allow (`unsafe_code = "forbid"`). The command layer CADCraft's interface needs is
the next thing to build, here or by moving `Session` into `cad-core` in withshalom/CAD.

`cad-core` uses no third-party crates and no `unsafe`, so it builds for the web like CADCraft.

## Licence

`cad-core` is proprietary. The code in `crates/blackcad` is MIT OR Apache-2.0 like the rest of
CADCraft, but anything built from this branch includes `cad-core` and is not open source (see
`NOTICE`). Nothing from this branch should be offered upstream to storytold/cadcraft.

## The seam: `crates/blackcad`

`cadcraft-blackcad` is the only crate that touches the engine. It starts with files and display:

- `open(bytes)`: `.cad`, DWG or DXF, detected by content (1 GiB cap), with the reader's notes.
- `save(drawing, format)`: `.cad`, DWG 2018 or DXF, as BlackCAD writes them, with what the
  writer left out.
- `model_space_svg(drawing)`: model space drawn by the engine.
- `guard(what, f)`: every call runs under `catch_unwind`, so a panic in the engine becomes an
  error (CADCraft's never-crash rule).

Tests: every format round trip of the engine's sample drawing, `.cad` keeping every object by
handle, hostile files (truncated, corrupted, empty, NaN) returning instead of crashing, and the
MI-201 corpus (below).

## First measurement: the MI-201 corpus through the engine

The same part saved by AutoCAD from R12 to 2018 (BlackCAD's `corpus/mosaiciron-mi201`), opened by
the engine, saved as `.cad` and as DWG 2018, and opened again. CADCraft's model isn't involved.

| File | Objects | Kept as DWG bytes | `.cad` round trip | DWG 2018 round trip |
|---|---|---|---|---|
| R12 DXF | 99 | 0 | all kept | 34 → 34 entities |
| R14 DWG | 341 | 100 | all kept | 34 → 34; 93 objects not written |
| 2000 DWG | 268 | 75 | all kept | 34 → 34; 70 not written |
| 2004 DWG | 258 | 72 | all kept | 34 → 34; 67 not written |
| 2007 DWG | — | — | doesn't open (2007 not read yet) | — |
| 2010 DWG | 243 | 37 | all kept | 34 → 34; 33 not written |
| 2013 DWG | 195 | 6 | all kept | 34 → 34; 2 not written |
| 2018 DWG | 191 | 4 | all kept | 191 → 191 objects, nothing left out |
| 2000–2018 DXF | 194–265 | 0 | all kept | 34 → 34; 0–27 not written |

So `.cad` keeps everything the engine reads, from every version, already. The DWG writer writes
only 2018, so objects older versions store differently (materials, visual styles, section and
detail view styles) are left out when saving those files as DWG; they're named in the writer's
notes. Saving a 2018 DWG is complete.

Also found: the DXF reader marks model space's own BLOCK and ENDBLK records as being in model
space, where the DWG reader doesn't. Harmless, but it's an inconsistency to fix in BlackCAD.

## Next

1. **Commands.** CADCraft's interface talks to CADCraft's engine through commands
   (`crates/engine`, `CommandSpec`). Decide how the interface reaches BlackCAD's: a command layer
   in this crate over `cad-core`'s operations, or BlackCAD's `Session` moved into `cad-core`.
2. **Display.** Feed CADCraft's canvas from `cad_core::render` scenes instead of its own renderer.
3. **One command end to end** (LINE, then MOVE): typed at CADCraft's command line, done by
   BlackCAD's engine, saved as DWG.
4. **DWG writer for older versions' objects** (in withshalom/CAD), so saving an R14–2013 file as
   DWG is complete too.
