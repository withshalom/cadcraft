# CADCraft's drawing model against DWG — the map

> **Status:** S1–S4 and the field gaps of §4 done, 2026-10-23 (branch `cad-native`) · **Measurements:** `crates/io/tests/model_map.rs`
> · **Format:** [`cad-format.md`](cad-format.md)

This is the plan for making CADCraft's own drawing model (`cadcraft-model`) the one model that DWG,
DXF and `.cad` all read into and write from. Everything below is measured or read from the code.

## 1. Decisions (owner, 2026-10-22)

1. **CADCraft's model is the model.** DWG maps straight onto `cadcraft-model`: no second drawing model
   in between, and no DXF text in between.
2. **CADCraft gets its own DWG reader and writer** in `crates/dwg`. The low-level parts (bit
   streams, compression, checksums, the containers, the object map) are ported from WithShalom's open
   `cad-dwg` crate (MIT OR Apache-2.0, same owner) and credited in `NOTICE`. The object decoders
   and encoders are rewritten to read and write CADCraft's types.
3. **`.cad` keeps `cad-native`'s container** (`cad-native`). Its records stay CADCraft's own.
4. **The work is done here first, on `cad-native`.** The model additions and the DWG reader and
   writer then go to `dwg-fidelity` and the upstream PR. The `.cad` format stays on this branch
   until it is shared later. By then it already sits on the same model and the same DWG mapping.

## 2. How it is measured

`model_map.rs` takes AutoCAD-saved files (the MI-201 corpus, never committed) and does this:

- Reads each DXF into the model and writes it back as DXF.
- Matches the two files record by record: by handle, then by name, then by position.
- Compares each record's values. References to other objects are left out of that comparison;
  whether a record kept its handle is counted separately.
- Writes the model to `.cad` and back, and fails if that changes anything.

For DWG it lists the object types each file holds.

**What `.cad` holds:** everything the model holds, in all eight versions (the test passes). So
`.cad` is as good as the model, and every gap below is a gap in the model or in the DXF writer.

### 2.1 Results (MI-201, one part saved in every version)

Now, saved as CADCraft saves a drawing (in its own version):

| File | Records with the same values | Records with a new handle | Header variables kept |
|---|---|---|---|
| R12 DXF (AC1009) | 0 of 63* | – | 133 of 133 |
| 2000 DXF (AC1015) | 249 of 265 | 0 | 199 of 199 |
| 2004 DXF (AC1018) | 240 of 256 | 0 | 211 of 211 |
| 2007 DXF (AC1021) | 235 of 248 | 0 | 251 of 251 |
| 2010 DXF (AC1024) | 235 of 246 | 0 | 252 of 252 |
| 2013 DXF (AC1027) | 187 of 198 | 0 | 253 of 253 |
| 2018 DXF (AC1032) | 183 of 194 | 0 | 253 of 253 |

The first map (2026-10-23) measured the R2000 writer instead: 124 of 194 for the 2018 file, every
table record and object with a new handle, 107 of 253 header variables.

What still differs, in the 2018 file (11 of 194 records), is by design or not data:

- the two dimensions, the named object dictionary, and the table style carry CADCraft's own
  extension data (exact dimension links, its own dictionary entries, complete cell styles);
- the DIMSTYLE record: AutoCAD leaves out the groups that hold defaults and CADCraft writes them,
  and the style's persistent reactors are not kept;
- four TABLE headers: AutoCAD's record counts in them are hints (a table of 3 records says 1);
- the active viewport's view centre and height: the view is the editor's, so a save fits the
  drawing's extents, as before. Everything else in the VPORT record is kept.

\* R12 files have no handles on most records and CADCraft writes them as a later version, so
nothing matches one to one. R12 is measured for header variables only.

What every drawing in the corpus keeps: all entities (34 in model space), layers, linetypes, text
styles, blocks and their contents, and every handle of an entity. What it loses is below.

## 3. Structural gaps

These cut across every type. DWG needs all of them; DXF and `.cad` benefit too.

| # | Gap | Measured | What DWG needs | Change to the model |
|---|---|---|---|---|
| S1 ✓ | **Handles of tables, blocks, layouts, dictionaries and styles** are not kept: CADCraft gives them new ones when it writes. | Every table record, block and object that comes back gets a new one (2018: 138 of 138) | The object map is keyed by handle, and other objects, xrefs, data links and sheet sets point at handles. | A `handle` on every table record, block record, layout, style and kept object; the writer keeps it and only hands out new ones for new objects. **Done:** a `KeptHandle` on each (identity, not content: it doesn't take part in equality); `Foreign::handles` for every other role (tables, the dictionaries, a dimension's DIMASSOC); kept objects keep theirs; copies from other drawings drop theirs. |
| S2 ✓ | **Header variables:** the model holds all of them, but the DXF writer only writes the 107 it knows. | 146 of 253 lost on save (2018) | DWG's header is a fixed list per version, so the DWG writer needs a value for every one. | Keep each unmodelled variable's group code next to its value, so any writer can write it. Add the public DXF reference's full variable table (name, code, default). **Done:** `Header::codes`; the DWG writer will need the full table. |
| S3 ✓ | **The dictionary tree** (named object dictionary, extension dictionaries, ownership flags) is rebuilt by the writer, not kept. | 19 of 20 dictionaries rewritten (2018); entries renamed or reordered | DWG stores the tree as objects with owners and hard/soft pointers. | Keep the tree: each dictionary's handle, owner, entries (name → handle), hard-owner and duplicate-record flags. CADCraft's own entries are added into it rather than replacing it. **Done:** dictionaries keep their handles and owner reactors, list entries in key order, keep AutoCAD's keys (`A0`… for scales), and objects CADCraft writes keep their extension dictionaries (a table style's cell style map). Left: CADCraft adds two XRECORDs of its own; a multiline style's key comes back upper case. |
| S4 ✓ | **Reactors on entities** (`102 {ACAD_REACTORS}`) are lost on entities that a dimension follows. | 4 of 20 lines and arcs lose them (2018) | DIMASSOC and associative hatches need their reactors on both ends. | Entities keep reactors and their extension dictionary in `ForeignExtra`, as table records already do. **Done:** the geometry a dimension follows names its DIMASSOC as a reactor. |
| S5 | **DWG-only data:** control objects, the class list with DWG class numbers, the preview image, summary info, app info, file dependencies, revision history, the auxiliary header, VX_CONTROL. | Not in DXF, so not measured; all 8 DWGs have control objects and classes | Writing a DWG needs every one; keeping them makes DWG → DWG lossless. | A `dwg` part in `Foreign`: the sections the model doesn't model, kept as the file's bytes with the version they came from. |
| S6 | **Objects kept raw are kept only as DXF groups.** | 7 types kept raw in the corpus (§4) | A DWG writer has to re-encode groups into DWG bits for every kept type, and that encoding is where the `cad-dwg` writer failed (group-order errors, §6). | A kept object holds its DXF groups and, when it came from a DWG, its DWG bits and version. Saving to the same DWG layout writes the bits back unchanged. |

## 4. Type by type

**Status:**
- **✓** = modelled, values come back the same.
- **◐** = modelled, with the field gaps listed.
- **R** = kept raw as DXF groups.
- **✗** = dropped.
- **·** = not in the corpus, so the status comes from the reader's code.

"In corpus" counts the 2018 DWG.

### 4.1 Entities

| DWG / DXF type | CADCraft model | In corpus | Status | Gaps |
|---|---|---|---|---|
| LINE | `Line` | 22 | ✓ | |
| ARC | `Arc` | 6 | ✓ | |
| CIRCLE | `Circle` | 4 | ✓ | |
| POINT | `Point` | 6 | ✓ | |
| SOLID, TRACE | `Solid`, `Trace` | 4 | ✓ | |
| TEXT | `Text` | 8 | ✓ | |
| MTEXT | `MText` | 2 | ✓ | |
| DIMENSION (all seven DWG types) | `Dimension` | 2 linear | ◐ | 73–75 (not in the DXF reference) lost; CADCraft adds its own associativity xdata |
| INSERT, MINSERT, ATTRIB | `Insert` | – | · | |
| ATTDEF | `AttDef` | – | · | |
| LWPOLYLINE | `LwPolyline` | – | · | |
| POLYLINE 2D, VERTEX 2D | `LwPolyline` | – | · | converted: the 2D/LW distinction and fit/spline vertices aren't kept |
| POLYLINE 3D, VERTEX 3D | `Polyline3d` | – | · | |
| POLYLINE PFACE / MESH, their VERTEXes | `Unknown` | – | R · | not modelled |
| ELLIPSE | `Ellipse` | – | · | |
| SPLINE | `Spline` (2D) | – | · | Z of 3D splines |
| RAY, XLINE | `Ray`, `XLine` | – | · | |
| 3DFACE | `Face3d` | – | · | |
| HATCH | `Hatch` | – | · | |
| LEADER | `Leader` | – | · | |
| MULTILEADER | `MLeader` | – | · | |
| MLINE | `MLine` | – | · | |
| TOLERANCE | `Tolerance` | – | · | |
| VIEWPORT | `Viewport` | – | · | |
| IMAGE | `Image` | – | · | |
| WIPEOUT | `Wipeout` | – | · | |
| ACAD_TABLE | `Table` | – | · | |
| HELIX | `Helix` | – | · | |
| REGION | `Region` (bulge loops) | – | · | its ACIS body is kept raw |
| 3DSOLID, BODY, SURFACE types | `Unknown` | – | R · | ACIS bodies: kept raw |
| SHAPE, OLE2FRAME, LIGHT, PDF/DWF/DGN UNDERLAY, proxy entities | `Unknown` | – | R · | |

### 4.2 Tables (each with its control object in DWG)

| DWG / DXF type | CADCraft model | In corpus | Status | Gaps |
|---|---|---|---|---|
| LAYER | `Layer` | 7 | ✓ | |
| LTYPE | `Linetype` | 3 | ✓ | the built-in dashless linetypes take the name and description the file gave them |
| STYLE | `TextStyle` | 1 | ✓ | |
| DIMSTYLE | `DimStyle` | 1 | ◐ | the writer adds groups for defaults (3, 40–50, 71, 72); persistent reactors not kept |
| BLOCK_HEADER (BLOCK_RECORD), BLOCK, ENDBLK | `Block` | 4 | ✓ | |
| VPORT | `VportConfig` and `VportSettings` (the lone `*Active` tile: `active_vport`) | 1 | ◐ | every field but the view's centre and height (the editor's view) |
| APPID | `AppId` (`appids`) | 4 | ✓ | CADCraft adds its own applications |
| VIEW | `NamedView` | – | · | |
| UCS | `Ucs` | – | · | |
| VX_TABLE_RECORD (VX_CONTROL) | – | 1 (R14, 2000) | ✗ | S5 |

### 4.3 Objects

| DWG / DXF type | CADCraft model | In corpus | Status | Gaps |
|---|---|---|---|---|
| DICTIONARY, ACDBDICTIONARYWDFLT | written by CADCraft with kept handles, or kept raw | 19 + 1 | ✓ | the named object dictionary also lists CADCraft's own entries; a multiline style key comes back upper case |
| ACDBPLACEHOLDER | written by CADCraft | 1 | ✓ | |
| LAYOUT (with its PLOTSETTINGS) | `Layout` (`page`: `PageSetup` with `PlotSettings` and `PlotRead`; `record`: `LayoutRecord`); the Model layout in `model_layout` | 2 + Model | ✓ | the plot record's own values are kept until the page changes |
| SCALE | `AnnoScale` | 17 | ✓ | |
| MLINESTYLE | `MLineStyle` | 1 | ✓ | |
| MLEADERSTYLE | `MLeaderStyle` (`props`: `MLeaderStyleProps`) | 1 | ✓ | |
| TABLESTYLE | `TableStyle` (`props`: `TableStyleProps`) | 1 | ✓ | CADCraft adds its complete cell styles as xdata |
| CELLSTYLEMAP | kept raw, in the table style's extension dictionary | 1 | R ✓ | |
| DIMASSOC | `Dimension` associativity (`DimAssoc.read`: `OsnapRead`) | 2 | ✓ | the file's snap data is kept until the link changes |
| GROUP | `Group` | – | · | |
| SORTENTSTABLE | draw order of the store | – | · | rebuilt |
| IMAGEDEF, IMAGEDEF_REACTOR, RASTERVARIABLES | `Image` | – | · | |
| WIPEOUTVARIABLES | – | – | · | |
| XRECORD | layer states, constraints and others | 20 | R ✓ | |
| DICTIONARYVAR | kept raw | 5 | R ✓ | |
| VISUALSTYLE | kept raw | 24 | R ✓ | |
| MATERIAL | kept raw | 3 | R ✓ | |
| ACDBDETAILVIEWSTYLE, ACDBSECTIONVIEWSTYLE | kept raw | 1 + 1 | R ✓ | |
| FIELD, FIELDLIST, DATALINK, GEODATA, SUN, LIGHTLIST, LAYER_FILTER, LAYER_INDEX, SPATIAL_FILTER, SPATIAL_INDEX, IDBUFFER, the ACDBASSOC* network, dynamic block parameters, proxy objects | kept raw | – | R · | |

## 5. DWG reader and writer in CADCraft

`crates/dwg` becomes CADCraft's own codec on layer 2 (it depends on `model`, `geom` and `color`).
It is ported from WithShalom's open `cad-dwg` (§1):

| `cad-dwg` module | Lines | Coupled to its own model? | Here |
|---|---|---|---|
| `bits.rs`, `crc.rs`, `lz77.rs`, `objmap.rs`, `section.rs` | 2,000 | no | ported as is |
| `r2000.rs`, `r2004.rs`, `container.rs`, `streams.rs`, `write/container.rs` | 1,130 | barely | ported, model types removed |
| `classes.rs`, `eed.rs`, `header.rs`, `hvars.rs` | 1,990 | yes | ported; the header goes to `Header` with codes (S2) |
| `object.rs` (the common object and entity header) | 1,210 | lightly | ported; owners, reactors and xdictionaries go to S1/S3/S4 |
| `entities.rs`, `tables.rs`, `nongraphic.rs`, `styles.rs` | 3,800 | yes | rewritten to produce CADCraft's types; field layouts kept |
| `write/objects.rs`, `write/sections.rs`, `write/mod.rs` | 2,920 | yes | rewritten from CADCraft's types, not from DXF groups (S6) |

The rule for every type is that kept DWG bits are written back unchanged when the file is saved
in the same layout. Otherwise the type is encoded from the model. DXF groups are used only for
kept objects that came from a DXF.

`acadrust` stays behind it for what the new codec doesn't do yet (2007 and versions it can't
write), exactly as today, and leaves when the codec covers every version.

## 6. Why the current bridge loses data (for the record)

On `dwg-fidelity` today, a DWG goes DWG → `cad-dwg`'s model → DXF text → CADCraft's DXF reader. It
saves the reverse way. Two separate losses add up:

- **`cad-dwg`'s model:** it can't convert some objects, so the file goes to `acadrust`. Every
  version before 2018 does.
- **Re-encoding DXF:** the `cad-dwg` writer expects groups in its own order, and refuses MATERIAL,
  VISUALSTYLE and the view styles in the order `acadrust` and AutoCAD write them.

On the corpus only the 2018 files go through `cad-dwg` both ways. §5 removes both translations.

## 7. Order of work

1. **S1 handles, S2 header, S3 dictionaries, S4 reactors** in the model, with `.cad`, DXF read and
   DXF write following. Measured with `model_map.rs`: the corpus should come back with the same
   values and the same handles.
2. **The field gaps of §4,** type by type, each with a synthetic test from the public DXF
   reference.
3. **The codec, read side:** port the low level, map every type to the model, keep DWG bits (S5,
   S6). Every corpus DWG from R14 to 2018 should open with nothing left out.
4. **The codec, write side:** 2018 first (byte-checked against AutoCAD), then 2013 and 2010
   (same container), then 2004 and 2000.
5. **Tests:** value-level round trips (this test, extended to DWG), synthetic drawings for
   every type in §4 not in the corpus, damaged files.
6. **Bring it to `dwg-fidelity` and the PR,** without the `.cad` format.

## Revision history

- 2026-10-23: first map, from the MI-201 corpus and the reader's code.
- 2026-10-23: S1–S4 done: kept handles, header group codes, the dictionary tree, reactors.
- 2026-10-23: measured through CADCraft's save path; MTEXT defined height, dimension attachment
  and version, block uniform scaling, block record flags, the scale unit flag, the default text
  style.
- 2026-10-23: the field gaps of §4: plot settings and layout data (also the Model layout), the
  viewport's view settings, multileader and table style fields, dimension link snap data,
  arrowhead flips (groups 73–75), the APPID table, built-in linetype names; the named object
  dictionary is written in key order on a first save; `.cad` records 1.2. MI-201: 183 of 194
  records (2018) with the same values.
