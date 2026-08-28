# Living Implementation Plan

Last updated: 2026-08-25.

Legend: `[x]` complete, `[~]` active/partial, `[ ]` pending.

## Acceptance baseline

The first usable release must prove one end-to-end path:

```text
Figma extraction JSON
  -> Raw Model
  -> normalized IR
  -> diagnostics
  -> deterministic GPUI Rust
  -> pinned-GPUI compile test
  -> controlled geometry/render artifacts
  -> fidelity report
```

It must not claim unsupported effects, fonts, grid semantics, or headless Linux
pixels are correct.

## M0: Research and contracts

- [x] Verify current Figma Plugin API node/layout/style/text/variable/component
  surfaces using official docs and plugin typings 1.135.0.
- [x] Verify Dev Mode Codegen result language and 15-second callback limit.
- [x] Verify `JSON_REST_V1`, SVG, and image export routes.
- [x] Compare Plugin API, REST API, and Code Connect roles.
- [x] Inspect current GPUI manifest, core source, examples, test contexts, and
  platform renderer limits.
- [x] Pin GPUI revision `5631830c564afa89b3aba679f45d9c3345f9460f`.
- [x] Record capability matrix and unsupported/ambiguous behavior.
- [x] Freeze initial package and schema boundaries in `docs/architecture.md`.

Exit criterion: the four required documents exist, cite upstream evidence, and
do not hide current Linux screenshot or advanced-effect limitations.

## M1: Compiler core

- [x] Create the Cargo workspace and `figma-rust-core` crate.
- [x] Define versioned raw extraction types with extension/unknown-field retention.
- [x] Traverse selections beyond 2,000 nodes in deterministic bounded chunks and
  retain root/subtree completeness in the extraction manifest.
- [x] Define target-neutral Design IR and stable JSON serialization.
- [x] Implement schema/hierarchy validation and finite-number checks.
- [x] Implement schema-v2 variable/token binding with full consumer mode context,
  multi-mode values, and literal fallback resolution.
- [x] Implement UTF-16-safe mixed text normalization.
- [x] Implement parent-aware HUG/FILL/FIXED, min/max, and per-child counter-axis
  alignment normalization.
- [x] Implement stack, grid, absolute, constraints, clipping, and scroll passes.
- [x] Implement style/effect normalization and asset-decision diagnostics.
- [x] Preserve component set, variant, property, instance, and override metadata.
- [x] Add component mapping registry and structural fallback.
- [x] Add raw parser, normalization, sizing, token, component, asset, and
  diagnostic unit tests.
- [x] Add first golden raw -> IR fixtures.

Exit criterion: the core crate has no Figma/GPUI dependency, all tests pass, and
ambiguous FILL/missing token/unmapped component/unsupported effect cases produce
node-scoped diagnostics.

## M2: GPUI backend and codegen

- [x] Create `figma-rust-codegen` with `proc_macro2`, `quote`, `syn`, and
  `prettyplease`.
- [x] Lower fixed/HUG/FILL stacks, child counter-axis overrides, absolute nodes,
  text, solid fills, per-edge borders, independent radii, opacity, and shadows to
  verified GPUI APIs.
- [ ] Lower simple grids to verified GPUI APIs.
- [x] Generate stable per-node functions and required imports.
- [x] Generate context-aware color and number token access with literal fallback;
  modeled number paths are dimensions/min-max, gap, padding, radii, stroke widths,
  and text size.
- [ ] Generate component call/fallback forms.
- [ ] Generate action contracts without application behavior.
- [x] Generate Figma ID -> Rust symbol/line sidecar map.
- [x] Prove same input produces byte-identical code and maps.
- [x] Create compile fixture against exact GPUI Git SHA.
- [x] Run rustfmt, cargo check, tests, and Clippy for generated code.

Exit criterion: a nested responsive fixture produces deterministic Rust that
compiles against the pinned upstream; no string-concatenated Rust syntax remains.

## M3: Narrow GPUI runtime

- [x] Create `figma-gpui-runtime` with the exact GPUI pin and no component library.
- [x] Add typed token resolver, consumer collection/mode context, and source
  metadata helpers.
- [ ] Add mixed-track grid primitive only if native/nested lowering cannot match
  fixtures.
- [ ] Add exact image crop/fit helper.
- [ ] Add stroke alignment, richer gradient, transform, and mixed-text helpers one
  at a time, each gated by a minimal fixture.
- [ ] Keep arbitrary masks/effects on SVG/raster paths until a tested primitive
  exists.

Exit criterion: every public primitive has an isolated compile, geometry, and
render fixture; runtime contains no controls or application state.

## M4: CLI and plugin

- [x] Implement `figma-rust inspect` for tree/capability summaries.
- [x] Implement `figma-rust lint` with text/JSON diagnostics and strict mode.
- [x] Implement `figma-rust compile` for IR, Rust, source maps, and diagnostics.
- [x] Add compile-time asset manifest output with locked, staged SVG/PNG payload
  publication and handled-failure rollback.
- [x] Implement `figma-rust verify` for geometry and image reports.
- [x] Implement loopback-only `figma-rust serve` with bounded JSON requests.
- [x] Add an opt-in fixed-path loopback export with exact byte/hash/completion
  reporting and atomic replacement after schema-v2 validation.
- [x] Create Figma plugin manifest and a TypeScript extractor that preserves
  multi-mode variables plus modeled numeric bindings.
- [x] Add selection diagnostics UI and JSON export.
- [x] Add asset/SVG and raster extraction for requested fallback nodes.
- [x] Add Dev Mode Codegen bridge with internal timeout and clear unavailable
  service result.
- [x] Typecheck and bundle the plugin.

Exit criterion: the same extraction bundle compiles through CLI and local Codegen
service; Codegen callback performs no Cargo/render work and respects the deadline.

## M5: Verification loop

- [~] Add fixture manifest with Figma node IDs, viewport, expected geometry,
  reference image, fonts, and thresholds.
- [x] Add deterministic GPUI test-support geometry capture.
- [x] Add hierarchy/bounds/clipping comparison.
- [ ] Add derived spacing and alignment comparison.
- [x] Add PNG color/edge/perceptual comparison.
- [ ] Add node-scoped pixel difference reporting.
- [x] Add compositor-backed Linux capture harness and record its limitation.
- [ ] Add macOS headless capture job when a macOS runner is available.
- [ ] Build fixtures for horizontal, vertical, nested, HUG/FILL/FIXED, simple and
  mixed grid, absolute, min/max, text/multiline/mixed text, border/radius,
  opacity/shadow/gradient, clipping/transform, SVG/image, components/variants,
  states, and a complex screen.
- [x] Create at least one real Figma fixture file and run extract -> compile ->
  render -> compare.

Current real-fixture proof: Figma file `XRjBi41NCw78G6lkyJjmbV` supplies a GROUP
coordinate snapshot and reference PNG; extraction, strict lint, deterministic
compile, pinned-GPUI compile, measured GPUI geometry, and zero-tolerance geometry
verification pass. A controlled Wayland compositor capture renders the exact
100x60 window over black and white backdrops, captures both through the desktop
portal at scale 1, and reconstructs straight-alpha RGBA before atomic publication.
`verify.image.json` passes at zero tolerance with MAE 0, changed-pixel ratio 0,
edge error 0, and SSIM 1. Pinned GPUI still returns
`render_to_image not implemented for this platform`; the checked-in probe retains
that failure path while `/capture-linux` supplies a repeatable Computer Use
fallback. The Rust capture binary exposes explicit display, validated data-URL
ingest, reconstruction, and provenance-finalization modes; malformed or mismatched
capture evidence fails before artifact publication.

Exit criterion: every implemented capability has a fixture and a regression can
name the differing Figma node. Compile-only success is insufficient.

## M6: Hardening and release readiness

- [ ] Add schema compatibility policy and extraction fingerprinting.
- [ ] Add asset cache invalidation and deterministic content addressing.
- [ ] Add REST importer with lower-fidelity diagnostics.
- [ ] Add optional Code Connect template adapter without making it mandatory.
- [ ] Add license inventory and release packaging.
- [ ] Add Windows render evidence and font/DPI matrix.
- [ ] Define strict fidelity thresholds per fixture class and platform.

## Continuous feature loop

Every feature follows this gate before its checkbox becomes complete:

1. verify Figma semantics from an upstream source;
2. verify the pinned GPUI API/source;
3. create a minimal fixture;
4. implement parser/IR/runtime/codegen;
5. run unit tests;
6. run golden tests;
7. compile generated Rust;
8. capture geometry and render;
9. compare with Figma;
10. diagnose the actual cause;
11. fix and add regression coverage;
12. update this plan and the capability matrix.

## Current known constraints

- Extraction schema v2 is an explicit compatibility boundary; v1 bundles must be
  re-extracted or migrated. They remain readable only to emit `FR-SCHEMA-001`.
- Enterprise extended-variable-collection overrides are not yet covered by a
  public fixture; current verified mode handling covers standard collections and
  cross-collection alias chains.
- Linux headless pixel rendering is not supplied by pinned GPUI; a compositor is
  required for pixel artifacts.
- Exact font pixels are platform-dependent; geometry and text envelopes are
  first-class metrics.
- Figma Codegen has a 15-second deadline; full verification cannot run there.
- Code Connect publishing is plan/seat gated and is never a core dependency.
- Advanced Figma effects and blend modes require explicit SVG/raster fallback
  until proven runtime primitives exist.
