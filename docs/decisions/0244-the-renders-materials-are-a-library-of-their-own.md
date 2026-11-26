# 0244. The render's materials are a library of their own, and a catalogue record points into it

- Status: Accepted; extends 0243 with the record's home and shape, and shapes O16, O18 and Y7
- Date: 2026-11-25

## Context

The owner's requirement, 2026-11-25: *the render must be independent, so that it can be lifted into
another project, and it must follow the standards of the popular renderers.* In the application the
materials are two lists: **sheet materials** — chipboard, glass, MDF — and **decorative coatings** —
paint, PVC film, veneer, lacquer. Each of them will name a material of the render's own base, by an
id or otherwise; the viewport takes a material from that base and draws it in a simplified form, so
that the chipboard's texture is visible while a panel is dragged; and the panel card will hold, for
the plate's face and for the edge, an **ordered list of decors** — a veneer, then a lacquer at 90 % —
whose order is the order of the layers, which the render composes into the picture.

What the tree has: `crates/catalog` is the workshop's nomenclature and knows nothing about a picture;
`crates/scene` carries a `Surface` (roughness, metallic, ior, transmission, absorption) and a `Decor`
(the picture, the board it covers, the grain's turn) with no author; `crates/final` traces them. ADR
0243 decided that a surface is a base plus at most one coat, and said the record Y8 builds carries
it. Nothing decided where that record lives, what it is made of, or how a catalogue record reaches
it.

## Decision

**The render's materials are a library of their own — one portable file — and a catalogue record
only points at one.**

- **One crate owns the render's words: `materials` (L2), beside `catalog` and not inside it.** It
  holds `MaterialId`, a record (a base and at most one coat), the validated `Library` and the folder
  that carries them. It depends on `units` and `serde` and knows no document, no scene, no renderer,
  no `egui` and no `wgpu`, so a renderer lifted into another project takes the crate and its file
  with it. The catalogue never sees this crate's job: it stores an id.
- **A record is the renderer's vocabulary and nothing else** (ADR 0243 folds into this): the base is
  a colour, an optional decor, roughness, metallic, ior, transmission and absorption; the coat is a
  weight, a gloss in percent and its own ior. A record that is only a coat is a finish — a lacquer —
  and one that is only a base is a veneer, a paint, a metal or a patina. There is no kind field: the
  catalogue knows what a thing is called, the renderer knows how it is drawn.
- **The library is the workshop's, not the scene's.** `library.json` sits beside `settings.json` in
  the user's settings folder, with a `textures/` folder next to it; a picture is named by a path
  relative to that folder, so a shop carries its whole workspace by copying one directory. A scene
  never stores a colour, a picture or a number: it stores the id a record has, and an edit to the
  library reaches every part that names it, exactly as the nomenclature directory does (ADR 0148).
- **A picture is an ordinary file at a relative path and is not pinned.** The part documents of a
  package are pinned by a content hash because a moved cup bore changes the holes a shop drills (ADR
  0149); a decor is a scan a person replaces, so the library names the path and the shell reads the
  file again when its stamp moves. A replaced picture therefore reaches the frame without a restart.
- **The whole library is swapped, and the frame follows a revision.** The folder is watched off the
  frame path, one validated `Library` replaces the previous value, the session carries it beside the
  directory, and the tab re-derives its frame — never the document, its revision or its undo
  history. A record's picture is decoded when a frame first asks for it, one shared picture per
  record, so a cabinet of forty boards is one set of texels.
- **A decor's number is allocated, never derived.** A picture the shop replaced must arrive under a
  `DecorId` nothing has drawn before, because the renderer and the tracer keep a texture under that
  number until the frame stops naming it. The frame's decor set allocates a fresh id per record per
  library revision and drops the previous ones.
- **The composition of layers happens where a record becomes a frame.** The panel card's ordered
  list is document and catalogue work (O18 and the card's own task); the frame merges the list into
  one `Surface` and one `Decor` before the tracer sees it, so a stack costs no extra lobe and no
  extra drawable (ADR 0243). This record decides only that the layers are **records of this
  library**, chosen in order, and never a record per combination.

## What we do not decide here

- How many layers a face may name: ADR 0243 caps a surface at one base plus one coat, and a card may
  name a veneer and a lacquer — two records, two slots. A third layer needs its own record.
- Formats other than PNG. The decoder is `png`, already in the tree; a shop's JPEG is a later
  record, and its own ADR if it needs a new dependency.
- How a catalogue record says *which* of its two lists the material belongs to. That is O16's, and
  the two lists differ in a kind (a sheet, a coating), never in how they name a render material.

## Consequences

- The render can be lifted out with `materials`, its file and its pictures, and draws the same
  surfaces. A catalogue is one consumer of the library, not its owner.
- The library is additive where the catalogue is multiplicative: 30 veneers and 5 lacquers are 35
  records and 150 combinations assembled at build time (O18).
- Nothing edits a material yet: a shop writes the file by hand and drops its scans into `textures/`.
  A card that authors a record is its own task, and it writes the same file through the same store.
- A library that breaks is a sentence naming the record and the file, and the records in use stay in
  place; a decor whose picture is missing is refused at load, not drawn black.
