# Figma -> GPUI Research

Status: researched against upstream sources on 2026-08-25. This document records
capabilities and constraints, not product promises. The pinned GPUI revision for
the first implementation is `5631830c564afa89b3aba679f45d9c3345f9460f`.

## Executive findings

1. The Figma Plugin API is the primary extraction surface. It exposes the live
   node hierarchy plus plugin-only semantics such as `boundVariables`, mixed text
   ranges, component properties, direct instance overrides, reactions, and fast
   `JSON_REST_V1` export.
2. The REST API is useful for unattended/remote extraction and image endpoints,
   but it is not a semantic superset of the Plugin API. REST imports must carry a
   lower-fidelity diagnostic when plugin-only supplemental data is unavailable.
3. Figma Dev Mode Codegen can return `RUST`, but every `generate` callback has a
   15-second deadline and cannot call `figma.showUI`. The compiler therefore stays
   in Rust behind a localhost service; the callback only extracts, calls, and
   displays a bounded result. The standalone CLI never depends on Dev Mode.
4. Current GPUI uses Taffy for layout and natively covers flex, uniform grid,
   fixed/flexible sizing, min/max bounds, absolute positioning, clipping,
   per-edge borders, per-corner radii, drop/inner shadows, two-stop linear
   gradients, text shaping, SVG, images, opacity, focus, events, and scrolling.
5. GPUI does not natively express every Figma paint/layout semantic. Mixed grid
   tracks, richer gradients, general subtree transforms, Figma stroke alignment,
   arbitrary masks, advanced effects, and exact image crop semantics need a
   narrow runtime primitive, SVG preservation, or an explicit raster fallback.
6. GPUI now has a cross-platform `HeadlessAppContext`, deterministic test
   scheduling, real text-system injection, geometry selectors, and screenshot
   hooks. At the pinned revision the built-in headless GPU renderer is still
   macOS-only. Linux tests can be deterministic for layout/geometry, while pixel
   captures require a real compositor/window until a Linux headless renderer is
   available.
7. Code Connect is not a compiler input replacement. Its framework-agnostic
   templates can optionally seed component mappings and improve Dev Mode snippets,
   but publishing requires an Organization/Enterprise plan and a suitable seat.
   The core registry remains an open local format.

## Figma extraction surface

### Plugin API

The maintained TypeScript contract used for this research is
`@figma/plugin-typings 1.135.0`.

- `SceneNode` identity and hierarchy are available directly. Dynamic-page
  manifests require async access such as `getNodeByIdAsync`, page loading, and
  `InstanceNode.getMainComponentAsync`; deprecated synchronous access must not be
  used by the extractor.
- Auto Layout exposes `layoutMode` (`NONE`, `HORIZONTAL`, `VERTICAL`, `GRID`),
  axis sizing, alignment, wrapping, padding, item/counter-axis spacing, child
  grow/alignment, and `layoutPositioning` (`AUTO` or `ABSOLUTE`).
- `layoutSizingHorizontal` and `layoutSizingVertical` expose the user-facing
  `FIXED`, `HUG`, and `FILL` semantics. Their validity depends on the parent and
  axis, so normalization must be parent-aware.
- Grid exposes row/column counts, `FLEX`/`FIXED`/`HUG` tracks, independent gaps,
  automatic rows, row-major auto-flow, child anchors, spans, and cell alignment.
- `minWidth`, `maxWidth`, `minHeight`, and `maxHeight` are explicit for Auto
  Layout frames and direct children.
- Constraints use `MIN`, `CENTER`, `MAX`, `STRETCH`, or `SCALE` per axis.
  Absolute Auto Layout children continue to respect constraints.
- Geometry includes width/height, parent-relative 2x3 transforms, rotation,
  absolute render bounds, clipping, opacity, masks, and blend modes. The relative
  matrix deliberately does not encode resize scaling.
- Fills include solid, linear/radial/angular/diamond gradient, image, video,
  pattern, and shader paints. Strokes include per-edge widths, alignment,
  dashes, cap/join, variable width, brush, and dynamic forms.
- Effects include drop/inner shadow, normal/progressive layer/background blur,
  noise, texture, glass, and shader effects. Several modern effects intentionally
  have no variable-binding support.
- Text can contain mixed values. `getStyledTextSegments` returns text plus UTF-16
  ranges and requested fields including font, size, weight, line height, letter
  spacing, fills, OpenType features, styles, paragraph/list fields, hyperlinks,
  and bound variables. Reading one mixed property from the node is insufficient.
- Variables are preserved through `boundVariables` and range-level text bindings.
  Extraction records both a token reference and the resolved current-mode value;
  normalization never silently replaces a binding with a literal.
- Component sets expose property definitions and variants. Instances expose
  component properties (`VARIANT`, `TEXT`, `BOOLEAN`, `INSTANCE_SWAP`, and newer
  property forms), the async main component, nested exposed instances, and direct
  `overrides` with overridden field names.
- Reactions expose triggers and one or more actions. They are retained as typed
  metadata, but navigation or application business logic is never invented.
- `exportAsync({ format: "SVG_STRING" })` preserves vectors. Image exports return
  bytes. `JSON_REST_V1` returns an object equivalent to the REST
  `/v1/files/:file_key/nodes` response and is explicitly documented as the fast
  path for serializing large subtrees.

`inferredAutoLayout` is a Figma heuristic used by Dev Mode for frames that were
not authored with Auto Layout. It may be reported as a suggestion, but must not
silently replace authored structure. Using it requires an explicit compiler mode
and an `INFO` or `WARNING` diagnostic.

### REST API

Use REST when extraction must run outside Figma or in automation. It offers file
and node retrieval, versioned HTTP caching, image render/export endpoints, and
server-side access without an open editor. It also provides a stable JSON shape
that `JSON_REST_V1` can mirror.

Compared with a live plugin, REST has important limits for this compiler:

- plugin-only convenience and async object relations are absent;
- current editor selection and local runtime state are absent;
- mixed text, variable bindings, component-property details, and overrides must
  be checked field-by-field against the REST schema and may be less complete;
- assets need separate image/SVG endpoint calls;
- remote variables and library data can require separate endpoints and plan
  permissions.

A future CLI REST importer should therefore be a secondary input with explicit
fidelity diagnostics, not the canonical source. The current CLI does not import
REST nodes directly; an extraction bundle may only retain an optional
`rest_snapshot` for provenance.

### Dev Mode Codegen

`figma.codegen.on("generate", callback)` receives the selected `SceneNode` and a
language identifier. A result contains title, code, and a language that includes
`RUST`. The callback can be asynchronous but must finish in 15 seconds. Concurrent
generate events are possible and `figma.showUI` is forbidden inside the callback.

Figma's current API reference and plugin typings 1.135.0 both state 15 seconds,
while an older Codegen guide still states 3 seconds. The implementation treats
15 seconds as the current contract but uses a shorter internal network deadline
so it also fails clearly under the stricter documented value. This documentation
conflict remains a compatibility test, not a reason to run heavy work in callback.

The safe architecture is:

1. extract a bounded raw bundle in the callback;
2. POST it to a loopback-only `figma-rust serve` endpoint with an internal timeout;
3. return generated Rust and diagnostics;
4. return a clear setup/error snippet when the service is unavailable;
5. keep full asset export and expensive render verification outside the callback.

The service accepts JSON only, binds to `127.0.0.1`, does not accept arbitrary
filesystem paths, and caps request size. This avoids turning Codegen into an
unbounded local command-execution surface.

## GPUI capability findings

### Upstream identity

- Repository: `https://github.com/zed-industries/zed`
- Revision: `5631830c564afa89b3aba679f45d9c3345f9460f`
- `gpui` manifest version: `0.2.2`
- Layout engine dependency: `taffy = 0.13.0`
- Rendering/vector dependencies include `resvg`, `usvg`, and `lyon`.
- Runtime boot uses `gpui_platform::application()`.

The revision is pinned by Git SHA in every GPUI workspace dependency. A moving
branch or an unpinned Git dependency is not acceptable for generated-code tests.

### Layout and style

- `Styled` writes a `Style` consumed by Taffy. Flex direction/wrap/grow/shrink,
  gaps, alignment, fixed/relative sizing, min/max bounds, absolute insets,
  visibility, overflow, and scroll are available.
- Native grid is real Taffy grid, but GPUI's public convenience model currently
  represents repeated uniform tracks (`grid_cols`, `grid_rows`, min/max-content
  variants) plus row/column span. Arbitrary Figma track sequences therefore need
  a runtime layout primitive or a deterministic nested-layout lowering.
- Borders expose per-edge widths and one color/style. Figma's inside/center/
  outside alignment and multi-paint strokes are not equivalent to GPUI borders.
- Corner radii are represented per corner. Figma corner smoothing is a different
  curve and requires a custom path/SVG when non-zero fidelity matters.
- `BoxShadow` supports offset, blur, spread, multiple shadows, and inset shadows.
- Background supports solid color and a two-stop linear gradient. Figma
  multi-stop, radial, angular, and diamond gradients require runtime painting or
  SVG/asset preservation.
- Opacity is native. General arbitrary transforms are available in lower-level
  scene/path/SVG rendering, not as a complete CSS-like transform on every styled
  subtree; the compiler must not pretend otherwise.

### Text, assets, and behavior

- GPUI has a platform text system with shaping, wrapping, font resolution,
  glyph rasterization, runs/highlights, line layout, and font-feature support.
  Figma font family/style names still need deterministic resolution and a
  missing-font diagnostic.
- `svg().path(...)` uses GPUI's asset source and the upstream SVG renderer.
  `img(...)` supports asset, file, and URI sources plus sizing/object-fit behavior.
- Focus handles, key contexts, hover/active style closures, mouse/keyboard events,
  actions, and scroll containers are core APIs. Generated code emits typed action
  contracts; it does not generate application behavior.
- Custom `Element` provides request-layout, prepaint, and paint phases. `canvas`,
  paths, scene primitives, and custom painting are available for narrowly scoped
  runtime primitives.

### Test and render support

- `#[gpui::test]`, `TestAppContext`, and `TestDispatcher` provide deterministic
  task scheduling and controllable window sizes.
- `debug_selector` records final element bounds in test-support builds. Generated
  nodes use stable selectors derived from source IDs, allowing per-node geometry
  and hierarchy regression reports.
- `HeadlessAppContext` accepts a real platform text system and optional
  `PlatformHeadlessRenderer`; `capture_screenshot` calls `Window::render_to_image`.
- At the pinned revision `gpui_platform::current_headless_renderer()` returns a
  renderer only on macOS. Linux returns `None`. Linux pixel tests therefore use a
  visible compositor-backed fixture app and external capture; geometry tests stay
  headless/deterministic. This limitation remains visible in CI documentation.
- DPI, display scale, color profile, installed fonts, font backend, subpixel
  rasterization, and OS compositor affect pixels. Visual verification must combine
  geometry with perceptual metrics and record the environment fingerprint.

## Third-party lessons

Third-party projects are idea sources only; none is a required runtime dependency.

- Figma Code Connect demonstrates explicit design-component to code-component
  mappings and property templates. This directly supports the component registry
  concept, but Code Connect itself is optional and plan-gated.
- TeleportHQ's UIDL/code-generator work demonstrates a renderer-neutral serializable
  intermediate model followed by target generators. The useful lesson is the
  boundary, not its web-specific schema or runtime.
- Builder.io Mitosis demonstrates multiple deterministic backends from one
  component model. It does not solve Figma layout normalization or GPUI rendering.
- Commercial Figma-to-code products can inform expected diagnostics and mapping
  workflows, but closed implementations and marketing claims are not capability
  evidence.
- Screenshot-to-code systems are explicitly out of scope because they discard the
  structural source of truth that the Plugin API already provides.

## Code generation tool choice

Generated Rust is built with `proc_macro2` and `quote`, parsed as a `syn::File`,
and printed with `prettyplease` before a final `rustfmt` check. String formatting
is limited to generated comments, file paths, and post-format source-map line
measurement; Rust syntax itself is token/AST based.

Reasons:

- `quote` makes invalid token assembly harder than free-form concatenation;
- parsing the complete file with `syn` catches malformed generated syntax before
  writing output;
- `prettyplease` gives deterministic source independent of an installed formatter;
- `rustfmt` remains the authoritative formatting gate;
- stable node function names and a sidecar map preserve Figma ID -> Rust symbol and
  line ranges.

## Verification metrics

Pixel equality alone is rejected. A fixture result combines:

- exact node hierarchy and source-ID presence;
- per-node x/y/width/height with configurable absolute and relative tolerances;
- padding, gap, alignment, and clipping violations inferred from geometry;
- sampled color and alpha differences;
- edge-map difference for alignment and border regressions;
- normalized mean absolute pixel error;
- a perceptual similarity score;
- text bounds and baseline envelope, without requiring identical glyph pixels;
- reference and actual environment fingerprints.

A failing report names the source Figma node whenever source mapping exists.

## Sources

### Figma

- https://developers.figma.com/docs/plugins/api/api-reference/
- https://developers.figma.com/docs/plugins/api/node-properties/
- https://developers.figma.com/docs/plugins/api/figma-codegen/
- https://developers.figma.com/docs/plugins/api/properties/figma-codegen-on/
- https://developers.figma.com/docs/plugins/api/ExportSettings/
- https://developers.figma.com/docs/plugins/api/TextNode/
- https://developers.figma.com/docs/plugins/api/ComponentNode/
- https://developers.figma.com/docs/plugins/api/InstanceNode/
- https://developers.figma.com/docs/plugins/api/figma-variables/
- https://developers.figma.com/docs/rest-api/files/
- https://developers.figma.com/compare-apis/
- https://developers.figma.com/docs/code-connect/
- https://github.com/figma/plugin-typings/tree/v1.135.0
- https://github.com/figma/code-connect/tree/f55fc3a8d9392df0dd80a9e859ffdddbd5c50019

### GPUI

- https://github.com/zed-industries/zed/tree/5631830c564afa89b3aba679f45d9c3345f9460f/crates/gpui
- https://github.com/zed-industries/zed/blob/5631830c564afa89b3aba679f45d9c3345f9460f/crates/gpui/Cargo.toml
- https://github.com/zed-industries/zed/blob/5631830c564afa89b3aba679f45d9c3345f9460f/crates/gpui/src/styled.rs
- https://github.com/zed-industries/zed/blob/5631830c564afa89b3aba679f45d9c3345f9460f/crates/gpui/src/style.rs
- https://github.com/zed-industries/zed/blob/5631830c564afa89b3aba679f45d9c3345f9460f/crates/gpui/src/taffy.rs
- https://github.com/zed-industries/zed/blob/5631830c564afa89b3aba679f45d9c3345f9460f/crates/gpui/src/app/test_context.rs
- https://github.com/zed-industries/zed/blob/5631830c564afa89b3aba679f45d9c3345f9460f/crates/gpui/src/app/headless_app_context.rs
- https://github.com/zed-industries/zed/blob/5631830c564afa89b3aba679f45d9c3345f9460f/crates/gpui_platform/src/gpui_platform.rs
- https://github.com/zed-industries/zed/blob/5631830c564afa89b3aba679f45d9c3345f9460f/crates/gpui/examples/grid_layout.rs
- https://github.com/zed-industries/zed/blob/5631830c564afa89b3aba679f45d9c3345f9460f/crates/gpui/examples/shadow.rs
- https://github.com/zed-industries/zed/blob/5631830c564afa89b3aba679f45d9c3345f9460f/crates/gpui/examples/svg/svg.rs
- https://github.com/zed-industries/zed/blob/5631830c564afa89b3aba679f45d9c3345f9460f/crates/gpui/examples/image/image.rs

### Architecture and comparison references

- https://github.com/teleporthq/teleport-code-generators
- https://github.com/BuilderIO/mitosis
- https://github.com/kornelski/dssim
- https://github.com/image-rs/image
