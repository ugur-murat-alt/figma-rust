---
name: figma-rust-visual-verification
description: Verify figma-rust or handwritten GPUI output against Figma using
  source-linked geometry, deterministic pixel comparison, exact fonts, Linux
  Wayland compositor capture, hashes, and provenance. Use for fidelity,
  screenshots, geometry bounds, pixel metrics, capture, thresholds,
  transparency, or visual regression work.
metadata:
  oc-skill-power: managed
  oc-skill-power-category: development
---
# figma-rust Visual Verification

Use this skill whenever a task claims that generated or handwritten GPUI matches Figma. Load the umbrella `figma-rust` skill first. A compile, unit test, screenshot, or subjective visual inspection is not fidelity proof.

## Trigger and outcome

Use this skill for:

- geometry, hierarchy, clipping, spacing, alignment, or viewport comparison;
- typography, color, stroke, gradient, effect, image, SVG, transform, or alpha verification;
- fresh Linux Wayland compositor capture;
- verification manifests, thresholds, image metrics, hashes, or provenance;
- visual regression diagnosis for generated or handwritten GPUI;
- deciding whether a compiler capability can be marked supported.

The successful outcome is a source-linked report with explicit thresholds, passing geometry/pixel results where required, reproducible capture provenance, and a clear distinction between generated and handwritten rendering.

## Required reading and skills

1. Read `.opencode/skills/figma-rust/references/usage.md`, sections 6-9.
2. Read `.opencode/skills/figma-rust/references/fidelity-fonts.md` before any text pixel comparison.
3. Read `.opencode/commands/capture-linux.md` before fresh Linux capture.
4. For Linux transparent-window capture, load both `transparent-window-rgba-capture` and `atomic-artifact-publication`.
5. Load `opencode-rust-coder` when changing Rust capture, verifier, fixture, codegen, or runtime behavior.
6. Use `figma-rust-extract-compile` when the source extraction/generated reference is missing or stale.

## Hard boundaries

- Do not claim fidelity from `cargo check`, successful element construction, a screenshot alone, or human inspection alone.
- Do not mix evidence from different commits, Figma node revisions, viewports, scales, font sets, GPUI revisions, or generated outputs.
- Preserve source Figma node IDs in reference geometry, measured geometry, source maps, report context, and issue evidence.
- Use source-specific thresholds. Never copy Foundation thresholds to an unrelated file without evidence.
- Call `figma_gpui_runtime::configure_figma_fidelity(cx)` before opening a pixel-verification window; it selects grayscale glyph rasterization.
- Use exact source font files through an isolated `FONTCONFIG_FILE`. Never rely on whichever fonts are globally installed and never alter global Fontconfig state.
- Treat hand-authored GPUI and compiler-generated GPUI as different evidence classes. Label each honestly.
- Do not publish private Figma URLs, extraction bundles, screenshots, assets, or unsanitized reports.
- Do not read or modify `/home/ugur/Projects/OrbitLineV2` from this repository.

## Evidence model

Keep these proofs separate:

1. **Structural proof**: correct root, node order, diagnostics, source map, and generated artifact ownership.
2. **Pinned-GPUI compile proof**: generated or handwritten Rust uses the pinned API successfully.
3. **Geometry proof**: measured GPUI bounds/hierarchy compare with source-linked reference geometry.
4. **Pixel proof**: reference and actual images compare under declared thresholds.
5. **Capture provenance**: exact process/window identity, size, scale, source, fonts, hashes, and publication path are bound to the report.

Passing a later proof does not erase a missing earlier proof. Geometry can pass while pixels fail, and compiler-generated output can differ from a hand-authored proof.

## Inputs required before comparison

- source Figma file/node identity, kept private when necessary;
- immutable extraction/generated output identity or handwritten module identity;
- pinned GPUI revision and Rust toolchain;
- exact viewport width and height;
- reference geometry and actual measured geometry;
- reference and actual PNGs when pixels are in scope;
- exact font files/hashes and text rendering mode when text is present;
- a verification manifest with explicit thresholds;
- capture source and provenance requirements.

If any required identity is unknown, mark the comparison unproven instead of guessing.

## Step-by-step workflow

### 1. Establish a clean, immutable baseline

Record the active commit/worktree and hash the extraction, generated output, reference geometry, and reference image. Confirm that all files describe the same source node and viewport. Regenerate stale compiler artifacts through `figma-rust-extract-compile`; do not edit them.

For handwritten semantic GPUI, record the module/component and map each measured selector to its Figma node ID or an explicitly documented composition boundary.

### 2. Define thresholds before looking at the result

Create a manifest that names geometry inputs, optional image inputs, and thresholds:

```json
{
  "reference_geometry": "reference.geometry.json",
  "actual_geometry": "actual.geometry.json",
  "reference_image": "reference.png",
  "actual_image": "actual.png",
  "thresholds": {
    "geometry_absolute_px": 0.0,
    "geometry_relative": 0.0,
    "mean_absolute_pixel_error": 0.0,
    "changed_pixel_ratio": 0.0,
    "edge_error": 0.0,
    "minimum_ssim": 1.0
  }
}
```

Zero tolerance is appropriate only for a fixture expected to be byte/geometry identical. For production-scale text rendering, derive bounded thresholds from an approved profile and record why they are acceptable. Never relax thresholds after failure merely to obtain `passed:true`.

### 3. Prove geometry first

Render at the exact logical viewport and collect final GPUI bounds using deterministic debug/source selectors. Compare root and relevant child nodes, including clipping and absolute positioning. An empty child-node map proves only viewport geometry; do not describe it as component geometry.

Run:

```sh
"${CARGO:-cargo}" run -p figma-rust-cli -- \
  verify path/to/verify.geometry.json --json
```

Resolve geometry failures before pixel tuning. Pixel metrics are hard to interpret when wrapping, sizing, alignment, or clipping is already wrong.

### 4. Configure deterministic text and environment

Before opening the window:

```rust
figma_gpui_runtime::configure_figma_fidelity(cx);
```

For the canonical Foundation `419:2` profile only, prepare the checked and isolated font configuration:

```sh
export FONTCONFIG_FILE="$(
  .opencode/skills/figma-rust/scripts/prepare-fidelity-fonts.sh
)"
```

That profile uses its documented font identities and thresholds. For every other source, identify and hash its actual font files rather than reusing Foundation fonts blindly. Record scale, locale, color mode, backdrop, renderer, and any platform-sensitive configuration.

### 5. Capture the exact renderer output

Prefer a deterministic framework readback when the pinned platform implements it. On Linux, pinned GPUI has no client-texture readback; use the project command:

```text
/capture-linux fixtures/real-figma
```

The command is authoritative. It must:

- call Computer Use `get_app_state` first and stop if exact capture/targeting is unavailable;
- build the feature-enabled display binary and CLI;
- wait for first-frame readiness;
- retain the launched PID and match exactly one compositor window by PID/title/class;
- require exact logical/compositor dimensions and scale `1`;
- capture black then white opaque backdrops from `xdg-desktop-portal` as PNG;
- ingest data URLs through the checked Rust path;
- reconstruct straight-alpha RGBA and validate channel consistency;
- run the verifier into a unique temporary report;
- publish images/report/provenance with locking and same-directory atomic rename;
- clean the retained process and temporary files on success or failure.

Never substitute an arbitrary desktop crop, lossy video frame, resized image, or unverified window for this proof. If a fixture intentionally uses a lossy source, label the limitation and retain decoder/source evidence.

### 6. Run pixel verification

```sh
"${CARGO:-cargo}" run -p figma-rust-cli -- \
  verify path/to/verify.image.json --json
```

Interpret exits:

- `0`: comparison completed and every threshold passed;
- `1`: comparison completed but one or more thresholds failed;
- `2`: inputs/configuration could not be processed.

A report with `passed:false` is useful diagnostic evidence, but it is not a passing fixture.

### 7. Diagnose in causal order

Use this order to avoid masking the root cause:

1. source node/revision and viewport identity;
2. generated or handwritten module freshness;
3. hierarchy and geometry;
4. scale, crop, backdrop, and alpha reconstruction;
5. font files, text rendering mode, wrapping, line height, and glyph metrics;
6. solid fills, opacity, borders, radii, and clipping;
7. SVG/raster assets, authored colors, image crop, gradients, effects, and transforms;
8. threshold policy.

Change the owning extractor/IR/codegen/runtime/semantic component. Do not paint over failures by editing the actual PNG, reference PNG, generated Rust, or report.

### 8. Bind and publish provenance

The final evidence must identify or hash:

- commit and GPUI revision;
- source extraction/generated module;
- source node IDs and viewport;
- black/white composites and reconstructed actual image when transparency is involved;
- reference and actual geometry/images;
- manifest and verifier report;
- exact font files and text mode;
- capture source, scale, PID, window title/class, dimensions, and readiness event.

Use a lock, unique same-directory temporary files, validation, sync, and atomic rename. On failure, preserve the last known-good final and remove only the current writer's temporary files.

## Metric interpretation

- geometry differences: source-linked position/size/hierarchy mismatch;
- mean absolute error (MAE): average normalized channel difference;
- changed-pixel ratio: fraction of pixels beyond the verifier's change rule;
- edge error: structural contour mismatch, useful for layout/text/stroke diagnosis;
- SSIM: perceptual structural similarity; high SSIM does not override a failed declared threshold.

Report all metrics and thresholds together. Do not summarize a mixed result as “looks close.”

## Failure interpretation

- geometry pass, pixels fail: inspect fonts/text mode first for text-heavy views, then paint/assets/effects;
- root-only geometry pass: viewport is proven, descendants are not;
- compile pass, SVG blank/recolored: add a minimal rendered asset fixture; manifest/path existence alone is insufficient;
- both backdrop captures look identical for transparent content: backdrop control or targeting is wrong;
- PID/title/class/size/scale mismatch: fail closed and publish nothing;
- `render_to_image` unavailable on Linux: expected pinned-platform limitation; use compositor capture, not a fake success;
- report hashes do not match current inputs: stale evidence; rerun comparison and provenance finalization;
- threshold failure after a known intended design change: update reference/threshold policy only with explicit approval and source evidence.

## Completion gates

For verifier-only/offline work, require the exact manifest run and report/hash inspection. For Rust/capture changes, run:

```sh
"${CARGO:-cargo}" fmt --all -- --check
"${CARGO:-cargo}" clippy --all-targets -- -D warnings
"${CARGO:-cargo}" test
"${CARGO:-cargo}" test -p figma-generated-gpui-fixture --features capture-real-window
"${CARGO:-cargo}" clippy -p figma-generated-gpui-fixture \
  --features capture-real-window --all-targets -- -D warnings
"${CARGO:-cargo}" build -p figma-generated-gpui-fixture \
  --features capture-real-window
```

Do not claim completion until:

- required geometry and pixel reports pass declared thresholds;
- exact source/actual identities and hashes are recorded;
- no stale process or temporary publication file remains;
- generated versus handwritten evidence is labeled;
- private artifacts remain private;
- every failed or unrun gate is reported;
- the umbrella skill's GitHub issue decision has been applied.

Final reporting must include source node IDs, viewport, generated/handwritten classification, toolchain/GPUI revision, capture source and scale, font identity, manifest/report paths, thresholds, metrics, hashes, checks, and residual limitations.