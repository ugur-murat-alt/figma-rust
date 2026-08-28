# Architecture

## Goal and non-goals

`figma-rust` is a deterministic design compiler:

```text
Figma Plugin or REST
        -> extraction bundle / Raw Model
        -> validation + normalization passes
        -> framework-independent Design IR
        -> token/component semantic resolution
        -> GPUI target model
        -> Rust tokens / AST
        -> prettyplease + rustfmt
        -> cargo check/test
        -> GPUI render + geometry
        -> fidelity report
```

It is not screenshot-to-code, an LLM layout guesser, an OrbitLine-only exporter,
or a replacement application framework. Generated view code and handwritten
business logic remain separate.

## Workspace boundaries

The initial workspace uses four Rust crates and one TypeScript plugin:

| Package | Responsibility |
| --- | --- |
| `figma-rust-core` | Raw model, Design IR, parent-aware normalization, diagnostics, token/component metadata, asset decisions. No Figma or GPUI dependency. |
| `figma-rust-codegen` | GPUI target lowering, component mappings, Rust token/AST generation, deterministic formatting, source-map production. Depends on core, not GPUI. |
| `figma-gpui-runtime` | Only proven missing GPUI primitives, token/component contracts, source metadata, and test helpers. Depends on pinned GPUI core only. |
| `figma-rust-cli` | `inspect`, `lint`, `compile`, `verify`, and localhost `serve`; file/asset orchestration and exit codes. |
| `plugin/` | Figma selection extraction, diagnostics UI/export, and Dev Mode Codegen bridge. |

Shared manifests, GPUI pins, schemas, and fixture contracts are owned centrally.
The runtime must not grow controls, themes, application state, navigation, or
business behavior.

## Input contracts

### Extraction bundle

The plugin emits a versioned JSON bundle. The current version is `2`:

```text
ExtractionBundle
  schema_version
  source { extractor?, extractor_version?, file_key?, page_id, selected_node_ids, plugin_api_version }
  roots: RawNode[]
  variables: RawVariable[]
  components: RawComponent[]
  assets: RawAsset[]
  extraction_diagnostics: Diagnostic[]
  extraction_manifest { capabilities, traversal }
  rest_snapshot?: JSON_REST_V1 object
```

Schema v2 stores each referenced variable once per canonical consumer mode
context. Both `RawVariable` and token-bound values carry the applicable
`collection_id -> mode_id` map, including standard cross-collection alias chains.
Modeled numeric values use
`RawBoundValue<number>` so token identity and literal fallback survive for Auto
Layout gap/padding, corner radii, and per-edge stroke widths.

`RawAsset` can carry an optional base64 payload for compiler exports. Preview and
lint extraction retain asset metadata but skip payload export; Compiler JSON and
Dev Mode compilation include payloads. The CLI removes payloads from `ir.json`,
decodes supported SVG/PNG/JPEG/GIF/WebP data into deterministic flat file names,
and records them in `asset-manifest.json`.

`RawNode` deliberately uses Figma terms such as `layout_mode`, axis sizing,
constraints, grid tracks, bound-variable aliases, component properties, and
UTF-16 text ranges. Unknown source fields can be retained in an extension map so
new Figma fields do not disappear silently.

V1 bundles are deserialized only far enough to produce the explicit
`FR-SCHEMA-001` compatibility error. They are never compiled as v2 data.

Schema v2 readers accept additive optional/defaulted fields. Removing a field,
changing its meaning, or making an optional field required needs a new schema
version. Version 1 is legacy diagnostic-only; structurally parseable
unknown/future versions are unsupported diagnostic-only. The writer emits only
the current version.

`inspect` reports fingerprint contract v1: SHA-256 over the canonical typed
bundle with the domain prefix `figma-rust-extraction-fingerprint-v1\0`.
Deserialization removes JSON object-key/whitespace differences; capability flags
are sorted and deduplicated, while semantically ordered arrays remain ordered.
The digest binds extractor/API versions, source identity, selected roots,
capabilities, diagnostics, and extracted semantic content without printing the
private source values that were hashed.

A direct REST-to-Raw-Model importer is planned but not implemented. The plugin
can retain an optional `rest_snapshot` for provenance; current CLI commands
consume the extraction bundle emitted by the plugin.

### Design IR

The IR is serializable, target-neutral, and stable enough for golden fixtures.
It includes:

- containers, stacks, grids, absolute layouts, text, vector, image, component,
  instance, and scroll nodes;
- axis sizing (`Hug`, `Fill`, `Fixed`) plus min/max constraints;
- alignment, padding, gap, grid tracks/placement, clipping, transforms, opacity;
- fills, gradients, strokes, independent radii, effects, and shadows;
- typography and explicit mixed-style runs;
- values carrying optional `TokenRef`, consumer mode context, and current literal
  fallback;
- component identity, set identity, variants, properties, direct overrides, and
  structural fallback children;
- typed interaction/state metadata;
- source Figma node ID on every node.

No IR type imports Figma plugin types or GPUI types.

## Normalization passes

Normalization is an ordered pipeline, not a recursive string generator.

1. **Schema validation**: reject malformed dimensions, non-finite numbers,
   duplicate IDs, impossible ranges, and unresolved required references.
2. **Hierarchy indexing**: build parent/child and component/instance indices.
3. **Variable resolution**: match `(variable ID, consumer mode context)`, attach
   token identity and literal fallback, and retain cycle-safe alias diagnostics.
4. **Text-run normalization**: convert Figma UTF-16 ranges into content-bearing
   runs without unsafe Rust string slicing.
5. **Parent-aware sizing**: interpret HUG/FILL/FIXED using parent layout and axis.
   FILL outside a compatible parent is an error, not a guess.
6. **Layout classification**: produce stack, grid, absolute, or plain container.
7. **Constraint lowering**: resolve anchors/stretch/scale from parent geometry and
   keep responsive formulas where target runtime support is required.
8. **Style normalization**: canonical colors, bound values, radii, strokes,
   effects, and transform matrices.
9. **Component semantics**: retain component identity and fallback subtree;
   mapping is target resolution, not IR flattening.
10. **Asset decision**: native -> runtime -> SVG -> raster, with a diagnostic on
    every fallback.
11. **IR validation**: assert normalized invariants before target lowering.

Every pass returns diagnostics with severity, code, message, source node ID, and
optional property path/help.

## Semantic component registry

The local registry is a versioned JSON format. A mapping contains:

- Figma component key and optional set key;
- validated Rust function/type path;
- variant/property mappings and target Rust names/types;
- token bindings;
- optional import path;
- supported states and a fallback policy.

The resolver produces either a typed GPUI component invocation or the original
structural subtree. Missing mappings produce `FR-COMPONENT-001` and never erase
the instance. Code Connect template import/export is an optional adapter around
this registry, not the registry's storage model.

## GPUI target and runtime

The backend first uses GPUI core:

- flex/grid/absolute layout and sizing;
- native colors, borders, radii, shadows, opacity, SVG, image, text, focus,
  events, hover/active styles, and scrolling;
- stable `.id(...)` and test-support `.debug_selector(...)` metadata.

SVG/raster fallback nodes are emitted as one GPUI asset element and their captured
descendants are not emitted a second time. Generated fallback views accept the
small `AssetResolver` contract; `DirectoryAssets` resolves manifest file names
relative to an application-owned directory.

Runtime primitives are accepted only when the capability matrix says native
GPUI is insufficient and a fixture proves the primitive. Candidate modules are:

- mixed-track grid lowering;
- stroke alignment/multiple stroke paints;
- richer gradient and mask painting;
- general subtree transform helper;
- exact image crop/tile behavior;
- mixed typography/paragraph layout helper;
- typed token resolver and component invocation contracts;
- source geometry collection.

Each module is independent and has unit, compile, geometry, and visual fixtures.
Unsupported high-cost effects remain SVG/raster fallbacks instead of turning the
runtime into a new framework.

## Rust code generation

The generator creates one stable function per source node. Function names use a
deterministic preorder index rather than sanitized mutable display names:

```rust
fn node_0007(tokens: &impl GeneratedTokens) -> impl IntoElement { /* ... */ }
```

This makes child composition straightforward and gives source mapping a stable
Rust symbol. Generated files contain a header and never contain application
logic. The pipeline is:

1. build `proc_macro2::TokenStream` with `quote`;
2. parse the complete stream as `syn::File`;
3. print deterministically with `prettyplease`;
4. compute symbol line ranges for the JSON source map;
5. run `rustfmt` and `cargo check` in compile fixtures.

Imports are collected from emitted target operations and component mappings.
Stable traversal uses source order for child painting and `BTreeMap`/sorted keys
for unordered metadata. Generated output contains no timestamps or random IDs.

## Tokens

A bound Figma value is represented as:

```text
BoundValue<T> { token: TokenRef?, fallback: T }
```

Generated code asks a typed token resolver for the token and supplies the source
fallback. A missing application mapping is diagnosed but still renders the
current Figma value. Truly literal source values bypass the resolver.

## Assets

The asset planner is deterministic:

1. native primitive;
2. proven runtime primitive;
3. SVG/vector export;
4. raster export.

Assets are content-addressed, keep their source node ID and export settings, and
are listed in a manifest. Raster output always includes diagnostic code,
property/effect reason, scale, and color profile. Re-generation cannot overwrite
handwritten assets outside the generated asset directory.

## Behavior boundary

IR preserves reactions. Generated views may emit a generated action enum or call
a typed sink:

```text
view state -> generated view
user event -> GeneratedAction
GeneratedAction -> handwritten application handler
```

Navigation, data loading, validation, order execution, and all other business
logic remain handwritten. Re-generation only writes generated directories and
source-map/asset manifests.

## Diagnostics

Severities are `INFO`, `WARNING`, and `ERROR`. Every diagnostic has a stable code.
Representative families:

- `FR-LAYOUT-*`: ambiguous FILL, invalid constraints, heuristic layout;
- `FR-TOKEN-*`: unresolved aliases/modes or missing target mapping;
- `FR-COMPONENT-*`: missing mapping, invalid property conversion;
- `FR-TEXT-*`: mixed runs, missing font, unsupported typography;
- `FR-PAINT-*`: unsupported blend/effect/stroke and raster fallback;
- `FR-ASSET-*`: export/decode/path problems;
- `FR-CODEGEN-*`: invalid mapping path, AST/format/compile failure;
- `FR-VERIFY-*`: geometry, hierarchy, image, or environment mismatch.

Compilation fails on any `ERROR`. Strict fidelity mode can promote selected
warnings, such as raster fallback or font substitution, to errors.

## Verification architecture

### Unit and golden

Raw parsing, every normalization pass, token/component resolution, asset choice,
diagnostics, and codegen have focused tests. Checked-in fixtures compare raw JSON
to normalized IR and generated Rust. Running codegen twice must be byte-identical.

### Compile

Generated fixture crates depend on the exact pinned GPUI SHA and runtime path.
The gate runs `cargo fmt --check`, `cargo check`, `cargo test`, and Clippy. A GPUI
API name is not considered supported until this gate passes.

### Geometry

Generated elements receive stable GPUI debug selectors. GPUI test-support renders
at controlled window sizes and records final bounds. The verifier compares the
node hierarchy, bounds, spacing, alignment, text envelopes, and clipping against
the Figma reference geometry.

### Pixels

- macOS can use GPUI's headless renderer at the pinned revision;
- Linux currently launches the isolated fixture app on a real compositor and
  captures its client area;
- the image verifier reports pixel, edge, and perceptual metrics;
- environment metadata includes OS, scale factor, font hashes, color profile,
  GPUI SHA, and fixture size.

No fixture passes solely because `cargo check` succeeded.

## Dev Mode and plugin process

Design mode shows extraction/lint diagnostics and exports a versioned bundle.
Dev Mode registers Codegen and calls the local bounded service. Full compile,
asset writing, Cargo, render, and comparison stay in the CLI because they exceed
Codegen's lifecycle and 15-second budget.

The localhost server is optional, loopback-only, request-size-limited, has no
arbitrary command or path parameters, and returns compiler diagnostics as JSON.
