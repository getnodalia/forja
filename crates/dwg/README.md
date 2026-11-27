# forja-dwg

Forja's own DWG reader, written from the Open Design Alliance's public DWG specification. It is a
work in progress and **not wired into Forja yet**: the app still opens and saves DWG through
`nodalia-dwg` (acadrust). This crate replaces that path step by step, and only after it matches it.

## What it reads today

- Version **R2000 (AC1015)** only. Other versions return `Error::UnsupportedVersion`.
- The file container: header, section locators and their checksums, the header-variables and class
  sections, and the object map (pages, handle and offset deltas, checksums).
- Every object's frame (size, type, handle, checksum), checked against the object map.
- The entities **LINE, CIRCLE, ARC and POINT**, with their layer handle, colour, linetype scale,
  invisibility and lineweight.
- **Layers** (the layer table entries): name, frozen, off, locked, frozen in new viewports, plots,
  line weight and colour. `Drawing::layer_of` resolves an entity's layer. Names are recovered exactly
  for UTF-8 and Latin-1 text; the code page is not applied yet.
- **TEXT** (insertion and alignment points, height, rotation, oblique angle, width factor, text,
  generation and alignment codes, style handle), **LWPOLYLINE** (vertices, bulges, widths, closed,
  constant width, elevation) and **INSERT** (position, scale, rotation, block handle).
- **Blocks** (the block table entries): name, anonymous, external reference. `Drawing::block_of`
  resolves an insert to its definition and `Drawing::entities_of_block` lists what a block holds.
- Any other object is counted in `Drawing::skipped` by type code.

## What it does not read yet

The header variables, the other tables (line types, styles), every other entity (MTEXT, POLYLINE, SPLINE,
ELLIPSE, dimensions, hatches, attributes…), R2004 and later (compressed sections, R2007, R2013/R2018),
and writing. Until those land, nothing here is a replacement for acadrust.

## Never crash

Every number in a DWG is hostile. The reader uses checked arithmetic and `get()`, caps locators,
classes and objects, and never panics. A damaged object is recorded in `Drawing::damaged` and the
rest of the drawing is still read; a damaged header or object map is an error.

## Tests

- Unit tests for the bit reader (B, BB, BS, BL, BD, DD, BT, handles, MC, MS) and CRC-16.
- `tests/entities.rs` writes drawings with the `nodalia-dwg` bridge (R2000) and checks that this
  reader returns exactly the same entities: a small sample and 1,600 random entities over four seeds.
  The bridge is a test oracle only, a dev-dependency on native targets.
- `tests/more_entities.rs`: TEXT, 60 random LWPOLYLINEs, INSERTs with a block definition, all compared
  with what was written, plus 4,000 mutations of a file that holds them.
- `tests/hostile.rs`: unsupported versions, every truncation, 6,000 random mutations, a damaged
  object and a damaged header.

## Clean room

Built from the ODA public specification and from files we generate ourselves. No GPL, LGPL or AGPL
code (LibreDWG and others) was read or copied.
