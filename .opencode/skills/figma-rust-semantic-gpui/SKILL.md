---
name: figma-rust-semantic-gpui
description: Turn figma-rust extraction and generated evidence into maintainable
  handwritten GPUI design-system code. Use for Foundation tokens, semantic
  components, variants, actions, application integration,
  TokenResolver/AssetResolver wiring, or replacing structural generated output
  with production GPUI components without losing visual traceability.
metadata:
  oc-skill-power: managed
  oc-skill-power-category: development
---
# figma-rust Semantic GPUI Integration

Use this skill when building production GPUI foundations or components from figma-rust evidence. Load the umbrella `figma-rust` skill first and load `opencode-rust-coder` before substantial Rust work.

The core rule is: generated GPUI is a presentation scaffold and measurable reference, not the application's long-term architecture. Build handwritten semantic components while preserving source-node traceability and use figma-rust verification to measure convergence.

## Trigger and outcome

Use this skill for:

- design tokens, themes, typography, spacing, radii, colors, and asset resolution;
- handwritten GPUI primitives and semantic components such as Button, Input, Dialog, DataTable, or composed screens;
- mapping Figma components/variants/properties into Rust props and typed actions;
- integrating a generated view in a host GPUI application;
- separating presentation from state, navigation, validation, networking, and domain behavior;
- migrating a large structural generated tree into maintainable production code.

The successful outcome is a small, typed, reusable GPUI API with tests and visual evidence. A copied or manually edited `generated.rs` is not an acceptable production integration.

## Required evidence and skills

1. Read `.opencode/skills/figma-rust/references/capability-and-diagnostics.md` and the relevant rows in `docs/figma-gpui-capability-matrix.md`.
2. Load `figma-rust-extract-compile` when a fresh schema-v2 bundle or generated baseline is needed.
3. If the task starts from a Figma URL and calls `get_design_context`, load `figma-design-to-code` first. Treat its output as a reference alongside the schema-v2 bundle, never as final GPUI code.
4. Use the `gpui-docs` MCP for pinned GPUI symbols, recipes, styling methods, examples, and compiler-error decoding. Do not guess current GPUI APIs.
5. Use `docs-rs` only for non-GPUI crates and resolve versions from `Cargo.lock` first.
6. Load `figma-rust-visual-verification` before claiming geometry or pixel fidelity.

## Hard boundaries

- Generated code contains presentation only. Application state, navigation, validation, networking, persistence, and domain behavior remain handwritten.
- Never edit compiler-owned output to add semantics. Regeneration must be safe and must not erase application work.
- Do not infer a business action from visual appearance. A button-shaped frame is not proof of a submit, delete, navigation, or network action.
- Preserve Figma node IDs in mapping notes, fixture manifests, diagnostics, source maps, and test/evidence names.
- Preserve literal fallbacks when resolving tokens; absence of an application token mapping must not make the generated reference unusable.
- Do not silently flatten unsupported variants, text runs, effects, transforms, masks, or assets. Keep a node/property-scoped diagnostic or explicit fallback.
- Do not read or modify any local OrbitLineV2 checkout while operating from this repository.
- Do not expose private extraction content, asset payloads, screenshots, file URLs, or credentials.

## Runtime contracts to use

The stable integration surface is `crates/figma-gpui-runtime/src/lib.rs`:

- `TokenResolver`: maps Figma token identity and `TokenContext` into application theme values while retaining extracted literal fallbacks.
- `FallbackTokens`: uses literals and is suitable for isolated compile/render baselines.
- `AssetResolver`: maps compiler-owned manifest file names to local application paths.
- `DirectoryAssets`: resolves assets relative to one application-owned output directory.
- `ActionSink<A>`: typed action boundary; generated/presentation code emits an action, handwritten application code decides behavior.
- `configure_figma_fidelity`: test/capture configuration, not normal application policy.

Generated signatures vary by required contracts:

```rust
// No fallback assets
pub fn generated_view<Tokens>(
    tokens: &Tokens,
) -> impl gpui::IntoElement + use<Tokens>
where
    Tokens: figma_gpui_runtime::TokenResolver;

// Fallback assets present
pub fn generated_view<Tokens, Assets>(
    tokens: &Tokens,
    assets: &Assets,
) -> impl gpui::IntoElement + use<Tokens, Assets>
where
    Tokens: figma_gpui_runtime::TokenResolver,
    Assets: figma_gpui_runtime::AssetResolver;
```

Verify the actual generated signature instead of assuming which form applies.

## Step-by-step workflow

### 1. Define one semantic slice

Choose the smallest useful unit: one token family, primitive, component, state, or composed section. Record:

- source Figma root and node IDs;
- intended Rust module and public type/function;
- visual variants and explicit properties;
- behaviors known from application requirements;
- behaviors that are unknown and therefore out of scope;
- applicable capability-matrix rows and diagnostics.

Do not start by transplanting an entire large generated screen.

### 2. Establish an immutable visual baseline

Use `figma-rust-extract-compile` to inspect, lint, and compile the source selection into a separate compiler-owned directory. Retain:

- `ir.json` for normalized layout/style/token/component facts;
- `source-map.json` for node-to-generated traceability;
- `diagnostics.json` for represented loss and unsupported properties;
- `asset-manifest.json` plus listed assets;
- `generated.rs` only as a structural/render reference.

If the bundle is stale or lacks the required node/property, re-extract. Do not patch the JSON to make it look supported.

### 3. Build foundations before composites

Use this order unless repository evidence requires another dependency order:

1. color and opacity tokens;
2. spacing, sizing, radius, and stroke tokens;
3. typography families, weights, line heights, wrapping, and source font identity;
4. primitive surfaces, text, icons/assets, separators, and focus/interaction visuals;
5. reusable semantic components and variants;
6. composed sections and screens;
7. application state and behavior adapters.

This prevents screens from accumulating one-off literals that should belong to the design system.

### 4. Implement token resolution deliberately

Implement an application `TokenResolver` rather than rewriting generated literals. Prefer `color_with_context` and `number_with_context` when collection/mode identity affects the value. Match on stable token IDs or a verified mapping table, use the supplied fallback for unknown tokens, and test light/dark or other required modes.

Do not assume that a token name is globally unique. `TokenContext` can include `collection_id`, `mode_id`, and the consumer's resolved collection-mode pairs.

### 5. Define a typed component API

For each component, derive only visually and semantically supported inputs:

- Rust props for text, booleans, variants, optional icons/content, and layout options;
- typed enums for closed variant/state sets;
- typed actions/events for interaction contracts;
- an application callback or `ActionSink` adapter outside generated/presentation code;
- accessibility labels, focus behavior, disabled state, and keyboard behavior from application requirements, not visual guesses.

Figma `component_key`, `component_set_key`, variants, properties, and overrides are mapping evidence. Current registry resolution does not by itself prove that codegen can lower every mapped component into a production Rust component call. Require a current fixture before claiming automatic semantic lowering.

### 6. Reproduce presentation in handwritten GPUI

Use GPUI recipes and symbols verified through `gpui-docs`. Keep component rendering deterministic:

- map Auto Layout to the proven parent-aware GPUI layout;
- preserve HUG/FILL/FIXED, min/max, wrapping, clipping, stroke, and text behavior only where evidence supports it;
- route complex vectors/masks/strokes/effects through the declared asset/runtime path rather than approximating silently;
- resolve manifest assets through `AssetResolver`;
- keep debug/source selectors in testable presentation boundaries when useful for geometry mapping.

Never use a successful compile as evidence that SVG colors, image crop, text shaping, clipping, or effects render correctly.

### 7. Connect application behavior outside presentation

Create a handwritten owner that stores state and handles actions. The semantic component receives props/current state and emits typed intent. The owner performs navigation, mutation, validation, network calls, persistence, or domain transitions.

Preferred dependency direction:

```text
application/domain owner
  -> semantic GPUI component API
      -> token and asset resolver contracts
          -> GPUI presentation primitives
```

Generated modules and Figma-specific extraction types must not leak into domain models.

### 8. Verify progressively

For each slice:

1. unit-test token/variant/property conversion;
2. construct/render the component against pinned GPUI;
3. run Clippy with warnings denied;
4. compare measured child/root geometry with source IDs;
5. run pixel verification with exact fonts and explicit thresholds;
6. integrate into the next composite only after the slice's required gates pass.

Use `FallbackTokens` to isolate the compiler/reference render and the real application resolver to test production theming. A difference between those renders must be intentional and documented.

## Failure interpretation

- Generated code is huge or hard to extend: expected for structural output; reduce the semantic slice and build a handwritten component rather than editing output.
- `FR-COMPONENT-001`: no semantic mapping; structural children are retained. Define and test an explicit application mapping if one is required.
- mapped-component codegen error: registry recognition and target lowering are separate capabilities; do not bypass the codegen error by editing generated Rust.
- unresolved token: fix extraction/mode context or application resolver mapping; preserve fallback and source identity.
- compile passes but image differs: treat as a visual defect or unproven capability; inspect geometry first, then fonts/paint/assets/effects.
- lifetime/ownership compiler error around generated resolvers: preserve the generated `impl IntoElement + use<...>` contract and verify with rust-analyzer/compiler; do not leak short-lived local borrows.
- SVG/raster fallback compiles but is blank or recolored: asset existence is insufficient; add a focused rendered-pixel fixture and fix the owning codegen/runtime path.
- component looks correct but acts incorrectly: the behavior owner or API contract is wrong; visual extraction is not business-logic evidence.

## Completion gates

A semantic component is complete only when all applicable items are true:

- public props, variants, actions, and state ownership are explicit;
- generated artifacts remain replaceable and unedited;
- token and asset resolvers have focused tests;
- pinned GPUI construction/check passes;
- relevant Rust tests pass;
- formatting and Clippy pass;
- source node IDs remain traceable;
- required geometry and pixel thresholds pass;
- unsupported properties remain diagnosed;
- no private evidence was published.

Run repository gates for changes made here:

```sh
"${CARGO:-cargo}" fmt --all -- --check
"${CARGO:-cargo}" clippy --all-targets -- -D warnings
"${CARGO:-cargo}" test
```

If extraction/plugin behavior also changed, run the plugin gates from `figma-rust-extract-compile`. If capture code/features changed, run the feature-enabled gates from `figma-rust-visual-verification`.

Finish with the umbrella skill's GitHub issue decision. Report component/module paths, source node IDs, token/component mappings, generated-reference location, checks, geometry/pixel results, known unsupported properties, and behavior deliberately left to the application owner.
