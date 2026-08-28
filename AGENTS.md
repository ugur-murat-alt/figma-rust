# figma-rust repository instructions

These instructions apply to the entire repository.

## Required workflow

- Load the project `figma-rust` skill for extraction, compilation, integration,
  verification, capture, or compiler development. Read the matching references
  before changing behavior.
- After the umbrella skill routes the task, load `figma-rust-extract-compile`
  for extraction/compiler work, `figma-rust-semantic-gpui` for handwritten
  Foundation or application integration, and `figma-rust-visual-verification`
  for geometry, pixel, capture, or provenance work. Multi-stage delivery uses
  them in that order and keeps each phase's acceptance gates separate.
- Load `opencode-rust-coder` for non-trivial Rust work. Treat the compiler and
  tests as authoritative.
- Before Figma `get_design_context`, load `figma-design-to-code`; before Figma
  writes, load `figma-use`.
- For Linux capture, load `transparent-window-rgba-capture` and
  `atomic-artifact-publication`, then follow `/capture-linux`.

## Invariants

- Preserve schema v2, Rust `1.97.1`, and GPUI revision
  `5631830c564afa89b3aba679f45d9c3345f9460f` unless an explicit upgrade task
  changes all related fixtures and evidence.
- Generated views contain presentation only. Keep application state, domain
  behavior, networking, validation, and navigation in handwritten code.
- Never edit compiler-owned output to fix behavior. Change the extractor, Raw
  Model, Design IR, code generator, or runtime owner and regenerate artifacts.
- Preserve Figma node IDs in diagnostics, source maps, fixtures, and evidence.
  Unsupported behavior must remain node/property scoped rather than disappear.
- Do not read or modify any local OrbitLineV2 checkout from this repository.
- Do not publish private extraction bundles, Figma URLs, screenshots, assets,
  credentials, or unsanitized evidence.

## Deterministic visual proof

- Call `figma_gpui_runtime::configure_figma_fidelity(cx)` before opening a
  pixel-verification window; it selects grayscale glyph rasterization.
- Font file identity is part of pixel provenance. Never rely on whatever font
  happens to be installed globally. For the canonical Foundation `419:2`
  profile, use
  `.opencode/skills/figma-rust/scripts/prepare-fidelity-fonts.sh` and export the
  returned `FONTCONFIG_FILE` only for the capture process.
- Require first-frame readiness, exact PID/window identity, scale `1`, validated
  dimensions, hashes, and explicit verifier thresholds. Publish artifacts with
  a lock plus same-directory atomic rename.

## Completion gates

Run the applicable checks and report only those that actually ran:

```sh
"${CARGO:-cargo}" fmt --all -- --check
"${CARGO:-cargo}" clippy --all-targets -- -D warnings
"${CARGO:-cargo}" test
npm --prefix plugin test
npm --prefix plugin run check
npm --prefix plugin run build
```

Capture-feature changes also require the feature-enabled fixture tests, Clippy,
and build documented in the project skill. A successful compile alone is not
geometry or pixel proof.
