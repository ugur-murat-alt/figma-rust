# Capability and diagnostic guide

## Architecture truth

The supported path is:

```text
Figma Plugin extraction
  -> versioned Raw Model
  -> validation and normalization
  -> target-neutral Design IR
  -> semantic token/component and asset decisions
  -> GPUI target validation/lowering
  -> deterministic Rust AST/formatting
  -> pinned-GPUI compile
  -> geometry and pixel verification
```

The system is not screenshot-to-code, an LLM layout guesser, an OrbitLine-only exporter, or an application framework.

## Diagnostic severity

- `INFO`: route/decision evidence; does not fail normal lint.
- `WARNING`: represented loss, fallback, or unsupported precision that remains usable; fails only strict lint.
- `ERROR`: an invariant or required representation failed; normal lint and compilation fail.

Compilation fails on any error. Never downgrade a diagnostic only to make compilation pass.

## Diagnostic families

- `FR-LAYOUT-*`: sizing, grid, constraints, alignment, or layout ambiguity.
- `FR-TOKEN-*`: unresolved variables, aliases, modes, or unsupported binding paths.
- `FR-COMPONENT-*`: mapping identity/property conversion or structural fallback.
- `FR-TEXT-*`: fonts, mixed runs, metrics, or typography loss.
- `FR-PAINT-*`: fills, strokes, gradients, effects, blending, SVG/raster fallback.
- `FR-ASSET-*`: planner route, export, decode, cache, or path behavior.
- `FR-CODEGEN-*`: unsupported target lowering, invalid Rust path/AST, formatting, or compile failure.
- `FR-VERIFY-*`: hierarchy, geometry, image, environment, hash, or provenance mismatch.
- `FR-EXTRACT-*` and `FR-VALUE-EXTRACT-*`: Plugin API source data could not be represented safely.

A diagnostic is not automatically a defect. Compare it with `docs/figma-gpui-capability-matrix.md`, `docs/plan.md`, and an isolated fixture.

## Current supported core

Verified slices include:

- versioned extraction bundles and unknown-field retention;
- parent-aware HUG/FILL/FIXED sizing and min/max constraints;
- stack, absolute, plain-container, clipping, scrolling, text runs, tokens, component identity/fallback, and asset route decisions in IR;
- deterministic GPUI Rust for the currently validated subset;
- stable node functions and Figma-ID source maps;
- fixed compiler artifact ownership;
- loopback lint/compile bridge;
- hierarchy/bounds and PNG MAE/changed-pixel/edge/SSIM verification;
- pinned-GPUI compile fixtures and real Wayland evidence.

## Known limitations that are not new bugs by themselves

- Simple grid GPUI lowering remains pending; mixed-grid runtime support is not proven.
- Typed component calls and generated action contracts remain pending.
- Asset/SVG fallback extraction and compile-time asset manifests remain pending.
- Stroke alignment, multiple stroke paints, richer gradients, transforms, image crop/fit, and mixed typography need isolated runtime/codegen proofs.
- Advanced masks/effects/blend modes can require explicit SVG/raster/runtime routes.
- Linux headless pixel rendering is unavailable in the pinned GPUI revision; real compositor capture is required.
- Exact font pixels are platform dependent.
- Figma Codegen has a short callback deadline and cannot own full Cargo/render verification.
- A direct REST-to-Raw-Model importer is pending. The plugin can retain an optional REST snapshot, but the CLI does not accept REST nodes as a separate input format.
- Code Connect publishing is optional and plan/seat dependent.

Open an improvement issue for one of these only when there is new concrete evidence, a bounded implementation path, and acceptance criteria. Do not create duplicate placeholders for every unchecked plan item.

## Evidence meanings

- `inspect` success: the extraction file was readable and reportable.
- `lint` success: diagnostics pass the selected normal/strict policy.
- `compile` success: IR and GPUI lowering succeeded and the fixed artifact set was published.
- generated-fixture Cargo success: emitted APIs compile against the pinned revision.
- geometry success: selected hierarchy/bounds satisfy thresholds.
- pixel success: selected decoded images satisfy explicit thresholds.
- provenance success: report inputs and capture/reconstruction artifacts are hash-bound and validated.

None of these implies all later gates.

## Hand-authored versus generated proof

A hand-authored GPUI fixture can prove that GPUI can reproduce a visual treatment. It cannot prove extractor, normalization, or codegen support. Always keep these records separate:

- Figma reference and source identity;
- real `extractNodes` output;
- inspect/lint/compile reports;
- hand-authored proof source;
- compositor capture/provenance;
- comparison manifest/report.

If the compiler stops, report the exact stage and diagnostic. Never describe the hand-authored result as generated output.
