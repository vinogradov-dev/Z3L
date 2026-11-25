# Performance

What this program costs per frame, per edit, per click and per megabyte, what has already been
measured and what has not, and where the next gain plausibly is. `AGENTS.md` §4.9 and §6 are the
rules; this page is the record, so a later session does not re-derive a number or re-litigate a
verdict that was already decided.

Two rules hold for everything here:

- **A claim about speed is a probe and a number**, never an argument. Every measurement below names
  the probe that produced it, and a probe lives in the crate whose cost it measures.
- **A change ships with its number.** An optimisation that cannot be measured in a headless run is
  not ready; the probes take the best of five runs so a busy machine reports its best case instead of
  nothing.

The measured numbers of the frame and the upload also live in `plan/debt.md` under `[perf]`, because
each of them is a gap that is still open. This page keeps the reasoning; that file keeps the debt.

## 1. What is measured today

Dev profile, headless, 2026-11-17, root spheres of 32 × 16 facets at 500 / 2 000 / 5 000 nodes.

| Cost | 500 | 2 000 | 5 000 | Probe |
|---|---|---|---|---|
| An edit anywhere (walk of the tree) | 0.42 ms | 3.6 ms | 18.4 ms | `an_edit_costs_the_walk_of_the_tree_and_not_the_vertices` |
| A rebuild that changes nothing | 0.41 ms | 3.67 ms | 18.4 ms | the same probe |
| A frame that changes nothing, in the window | 1.7 ms | 4.2 ms | 9.5 ms | measured in the shell |
| A click, empty space / through the field | 0.06 / 0.14 ms | 0.25 / 0.35 ms | 0.62 / 0.79 ms | `a_click_costs_the_objects_and_not_their_triangles` |
| One drawable's upload after an edit | 0.26 ms | 0.92 ms | — | `an_edit_uploads_one_drawable_and_not_the_scene` |
| Cold evaluation of the scene | 0.58 s | 2.2 s | 5.5 s | `an_edit_costs_the_walk_of_the_tree_and_not_the_vertices` |

The three numbers that matter for the interaction bar: a rebuild costs **3.7 µs per node**, a chrome
frame costs **1.9 µs per tree row**, and a click costs **the objects, not their triangles** (X59
turned 16 / 64 / 161 ms into 0.06 / 0.25 / 0.62 ms). Before X56 and X57 the same edit cost
27 / 109 / 285 ms and every edited frame 102 / 371 ms.

`SLOW_REBUILD` is 250 ms: a scene whose last rebuild overran it paints «Перестроение…», waits one
frame and runs the work on the next one, so the window answers input while a large scene builds
(`app/src/document.rs`).

## 2. Memory layout and alignment

### 2.1 The four mechanisms, and what each is good for

1. **A guaranteed layout on a device boundary** — `#[repr(C)]` plus `bytemuck`. This is correctness,
   not speed: without a predictable field order the WGSL offsets and `cast_slice` are guesses.
2. **Alignment of a hot struct to a cache line or a vector width** — `#[repr(align(32|64))]`. It pays
   only when the struct is read **alone** in a hot loop (a BVH node) or written by **different cores**
   (a shared counter).
3. **Padding inside a struct** — the holes a field order leaves.
4. **Alignment for SIMD** — meaningless while there is no explicit SIMD.

### 2.2 What the tree does today

`#[repr(C)]` appears in exactly eight types, all at the GPU boundary: `GpuFace`, `GpuNode`,
`GpuDecor`, `Uniform` (`crates/final/src/gpu/layout.rs`), `RenderCamera`, `MeshVertex`, `FaceVertex`
(`crates/render/src/geometry.rs`, `crates/render/src/lib.rs`) and `ShadowUniform`. The final render's
layout keeps the invariant that every member is `vec4`-shaped, so the WGSL declarations name the same
offsets without a `@align` anywhere. `bytemuck::try_cast_slice` guards the read-back instead of
assuming the mapped range is aligned.

There is **no `#[repr(align)]` in the tree**, and that is correct: every domain type is `repr(Rust)`,
which sorts fields by alignment and leaves at most a few bytes of padding, while `#[repr(C)]` on a
domain type would *add* padding, and an `align` attribute would grow every element of a `Vec`.

Measured with `size_of` on the shipped crates, 2026-11-24:

| Type | Size | Align | Padding |
|---|---|---|---|
| `units::Length` | 8 | 8 | 0 |
| `model::Placement` | 56 | 8 | 0 |
| `model::Panel` | 400 | 8 | ≤ 4 |
| `model::Node` (enum, largest variant `Panel`) | 400 | 8 | ≤ 7 |
| `model::Container` | 280 | 8 | ≤ 7 |
| `model::CameraNode` | 152 | 8 | — |
| `model::Project` | 312 | 8 | — |
| `scene::Drawable` | 312 | 8 | 0 |
| `scene::SceneFrame` | 584 | 8 | — |
| `geom::Aabb` | 48 | 8 | 0 |
| `geom::MeshData` (three `Vec`s) | 72 | 8 | 0 |
| `final::ray::Triangle` / `Hit` | 72 / 48 | 8 | 0 |
| `final::soup::Face` | 144 | 8 | 0 |
| `final::bvh::Node` | 72 | 8 | **7** |
| `render::MeshVertex` / `FaceVertex` | 40 / 48 | 4 | 0 |
| `final::gpu::layout::GpuNode` | 48 | 4 | 0 |
| `final::gpu::layout::GpuFace` | 144 | 4 | 0 |

### 2.3 Verdicts

- **Where alignment is already right:** the GPU boundary. It is hand-padded, `repr(C)`, and the
  padding is named in the comments rather than left to a reader counting bytes.
- **Where it would be wrong:** everywhere on the CPU side of the document and the geometry port. The
  document's cost is heap (`Vec`, `Arc`, `String`) and copy volume, not padding; `Panel` and `Node`
  carry a handful of bytes of slack each and no arrangement of attributes removes them.
- **Where it is a myth here:** false sharing. Nothing in the tree writes a hot value from two cores —
  there is no thread pool and no `rayon`; the tracer's `Progress` atomics are written by the tracing
  thread alone and read by the window once per sample.
- **The one place worth measuring:** `GpuNode` is 48 bytes, so a node begins on a 48-byte boundary and
  nearly always straddles two memory sectors of the traversal. Padding it to 64 (one full fetch per
  node) is a plausible trade — +33 % node memory (about 2 M nodes and +32 MB at the million-triangle
  cap) against fewer transactions per step of a loop that runs `pixels × samples` times. It is a
  hypothesis until `z3l render` shows a number; nobody should pad it on taste.
- **The second place, worth less:** `bvh::Node` carries 7 bytes of padding and could be exactly 64
  bytes with `u32` indices. But `Bvh::intersect` runs in tests only — the shipped traversal is in the
  shader — so the gain today is zero. Do it when a CPU tracer returns, not before.

## 3. How the rest of the code stands

**Evaluation and meshing — good, incremental, measured.** A mesh is keyed by the revision and the
features it was derived from, and a cache hit copies `Arc`s (`NodeGeometry` holds no vertex data). A
rebuild that changes nothing is the walk alone. Cold evaluation is 1.1 ms per 32 × 16 sphere and is
dominated by the kernel, which is the domain's cost, not a defect: the only real lever is not doing it
(more cache hits), and the cache already keys holes separately from the tessellation so a moved hinge
costs a clip and no meshing.

**Upload — good per drawable, weak per transform.** X57 keys a buffer by mesh, transform and colour,
so an edit uploads one drawable (0.26 / 0.92 ms). The remaining defect is that the world transform is
baked into the vertices: moving a body re-uploads its geometry. One buffer per unique mesh plus a
per-drawable transform in a storage buffer is the fix, and it is filed as `[perf]` debt.

**Picking — good.** A drawable is rejected by the box its mesh cache measured and cast only when the
ray reaches it; the cost is a small multiple of the object count rather than of the triangles. The
test asserts a 20 ms ceiling at 5 000 objects and measures 0.6–0.8 ms.

**The chrome — the largest per-frame number and the least optimised part of the frame.** At 5 000
nodes a frame that changes nothing costs 9.5 ms; at 500 it is 1.7 ms. That is 1.9 µs per tree row,
which is the scene tree drawing a row per node without virtualisation. A cabinet of a few hundred
parts does not notice; a scene tree that is left expanded at 5 000 does. Candidate: draw only the
rows inside the visible rectangle and keep each row's galley between frames.

**The rebuild walk — 3.7 µs per node is suspicious.** A cache-hit node costs a `HashMap` lookup, a
`Placement` compose and a box union: tens of nanoseconds of arithmetic. The suspects, in the order a
profile would confirm them: the per-node `NodeGeometry` clone (five `Vec`s), the second full tree
walk inside `authored_lighting_with`, the per-panel `PanelEval` and `HoleInputs::capture`, and the
`SipHash` lookups in `EvaluationCache`. This is the first thing to profile, because it is the cost
the owner feels as "the editor thinks before it draws" on every drag.

**History — a full document clone per command.** `Session` keeps `undo: Vec<Project>`, so one command
copies the whole tree: 400 bytes per node plus every heap payload. A kitchen of 10 000 nodes is 4 MB
per undo entry and a deep copy on each keystroke-sized edit. The fix is not alignment or padding: it
is a delta entry, or boxing `Panel` out of the `Node` enum so a camera no longer costs a panel's
width.

**The tracer — GPU, laid out by hand, unmeasured in the small.** The BVH build on the CPU produces
`Face` at 144 bytes and a 216-byte build item per triangle; the traversal is a compute shader over
`GpuNode`s. Its costs are a `z3l render` timing, and there is no probe for the node layout yet — see
§2.3.

**The denoiser — the host, threaded, measured on one picture.** `denoise::Frame::filtered` runs three
à-trous levels over the traced picture, one pass per level, its rows shared between threads. A
1280 × 960 frame — 1.2 megapixels, every pixel's guide and mean — took **243 ms** on this machine's
sixteen cores, i.e. 0.2 s per megapixel: 1.8 s for an A4 at 300 dpi against the ~15 s the same page
extrapolates to at 256 samples from Y1's 2.2 s for 1 280 × 960. The first version, one core and an index computed per tap, took
1.64 s for the same frame; sharing the rows out and clamping each tap's row and column once is what
made the difference (Y4, ADR 0246). It is worth a compute pass the day a render is shorter than its
own filter — at 16 samples a small image nearly is.

## 4. What to do next, in this order

1. **Profile the rebuild walk** with the existing probe and name the per-node cost. This is the
   highest-value unknown in the tree and the cheapest to answer.
2. **Virtualise the scene tree rows** (9.5 ms → under 2 ms at 5 000 nodes) if a profile still shows
   the chrome leading the frame.
3. **A/B `GpuNode` 48 vs 64** on `z3l render` with a fixed seed and sample count, and keep the answer
   with the number.
4. **Take the transform out of the vertex buffers**, which removes the last full-frame upload
   (~370 ms at 5 000 spheres).
5. **Shrink the history entry** (delta, or `Box<Panel>`), so a large scene stops paying 400 bytes per
   node per command.
6. **Shrink `Face` / the BVH build item** if the tracer's build ever shows up in a profile.

Each of these lands with its probe, its before/after number in `plan/debt.md`, and — when the decision
is not reversible — an ADR. A change that only borrows a faster-looking shape without a number is not
part of this list.

## 5. How to measure

- A probe is a `#[test]` whose name says what it costs (`an_edit_…`, `a_click_…`) and whose body
  prints its number with `println!`; run it with `cargo test -p app --lib <name> -- --nocapture`.
- Interaction budgets are asserted, not just printed: the pick probe fails at 20 ms, so a regression
  breaks the gate rather than a table.
- The upload and the frame need a real adapter; the headless render is `z3l render <scene> --size
  <w>x<h> --samples <n>` with the seed fixed, and a change is judged on the same target size.
- A `Vec` that is rebuilt every frame is visible as a rebuild whose cost tracks the node count while
  an unchanged rebuild tracks nothing: compare "an edit" with "a rebuild that changes nothing".
