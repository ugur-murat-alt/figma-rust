# figma-rust repository instructions

These instructions apply to the entire repository.

## Required workflow

- Load `gpui-design` for Figma-independent authoring, token/component/module/shell
  contracts, revisioned design commands, MCP work, Rust code ownership, native
  lowering, preview, or studio development. Read
  `docs/gpui-design-mcp-architecture.md` before changing those contracts.
- Load the project `figma-rust` skill for compatibility extraction, compilation,
  integration, verification, capture, or compiler development. Read the matching
  references before changing behavior.
- After the compatibility umbrella routes the task, load
  `figma-rust-extract-compile` for extraction/compiler work,
  `figma-rust-semantic-gpui` for handwritten Foundation or application
  integration, and `figma-rust-visual-verification` for geometry, pixel, capture,
  or provenance work. Multi-stage delivery uses them in that order and keeps
  each phase's acceptance gates separate.
- Load `opencode-rust-coder` for non-trivial Rust work. Treat rustc, Clippy, tests,
  and measured GPUI output as authoritative.
- Before Figma `get_design_context`, load `figma-design-to-code`; before Figma
  writes, load `figma-use`. Figma changes never silently replace an accepted
  GPUI Design authoring revision.
- For Linux capture, load `transparent-window-rgba-capture` and
  `atomic-artifact-publication`, then follow `/capture-linux`.

## Invariants

- Preserve native authoring schema v1, design command protocol v1, compatibility
  extraction schema v2, Rust `1.97.1`, and GPUI revision
  `5631830c564afa89b3aba679f45d9c3345f9460f` unless an explicit upgrade task
  changes all related fixtures and evidence.
- `gpui_design_core::AuthoringDocument` is the editable GPUI-native source of
  truth. The existing Figma-derived `DesignDocument` remains a normalized
  compiler IR and must not become the studio mutation format.
- Every authoring mutation is an expected-revision `DesignTransaction`.
  Validate the complete candidate before commit; failed commands and invalid
  candidates leave the accepted document unchanged.
- Stable IDs cross authoring nodes, tokens, component contracts, Rust bindings,
  source maps, runtime hit testing, diagnostics, and render evidence.
- Generated views contain presentation only. Keep application state, domain
  behavior, networking, validation, persistence, navigation, and trading actions
  in handwritten code. Handwritten behavior bindings are always reference-only.
- OrbitLine product code consumes approved semantic APIs such as `orbit_ui::*`.
  Raw GPUI primitives and styling stay inside framework/render implementations,
  not arbitrary product screens or agent-created one-off widgets.
- Never edit compiler-owned output to fix behavior. Change the authoring/lowering,
  extractor, Raw Model, compiler IR, code generator, or runtime owner and
  regenerate artifacts.
- Preserve Figma node IDs in compatibility diagnostics, source maps, fixtures,
  and evidence. Native documents preserve their own stable source IDs. Unsupported
  behavior must remain entity/property scoped rather than disappear.
- Actual GPUI output is the visual authority. A Figma frame, browser canvas, or
  successful compile alone is not geometry or pixel proof.
- Do not read or modify any local OrbitLineV2 checkout from this repository.
- Do not publish private authoring documents, extraction bundles, Figma URLs,
  screenshots, assets, credentials, or unsanitized evidence.

## Deterministic visual proof

- Call `figma_gpui_runtime::configure_figma_fidelity(cx)` before opening a
  compatibility pixel-verification window; it selects grayscale glyph
  rasterization.
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
npm --prefix opencode2 run build
npm --prefix opencode2 test
```

Capture-feature changes also require the feature-enabled fixture tests, Clippy,
and build documented in the project skill. A valid authoring transaction is not
native lowering; lowering is not a pinned-GPUI compile; compile is not geometry
or pixel proof.
