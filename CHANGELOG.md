# Changelog

All notable changes to this project are documented in this file.

## Unreleased

## [0.4.0] - 2026-08-28

### Added

- Deterministic extraction fingerprints, bounded traversal manifests, grouped
  diagnostics, typed text/layout metadata, and capability profiles.
- Root-scoped multi-selection compilation, content-addressed asset caching,
  crash-recoverable artifact generations, and source-node visual attribution.
- Exact image crop, child alignment, component property, transform scale, and
  runtime-route fixtures with pinned-GPUI construction checks.
- Repository-owned Linux Wayland capture plus macOS and Windows platform evidence
  workflows, including Windows `%100`, `%125`, and `%150` DPI profiles.
- Candidate GPUI qualification with deterministic positive reports and explicit
  incompatible-API classification.
- Public `@vaur94/figma-rust` OpenCode V2 package for the four repository-owned
  operator and task skills.

### Changed

- Enterprise extended variable collection contexts now fail closed with
  node/property-scoped `FR-TOKEN-MODE-005` before alias or value resolution;
  schema v2 continues to support standard collection mode contexts only.
- Compiler assets and complete generations now publish atomically under bounded
  recovery rules; multi-root work can retain successful roots while reporting a
  failing command status.
- Platform evidence records physical dimensions, renderer/font identity, hashes,
  declared thresholds, and deterministic two-run comparisons.

### Fixed

- Authored SVG fallback colors, token-bound dimensions, counter-axis overrides,
  text layout metadata, image crop geometry, subtree scale transforms, and hidden
  component children no longer disappear or lower through an unsafe route.
- Loopback compiler export now uses a fixed token-protected target with atomic
  replacement and explicit transfer/traversal completion.
- The Ubuntu GPUI qualification job installs its Fontconfig and XKBCommon native
  build dependencies before compiling the pinned renderer fixture.

## [0.3.1] - 2026-08-27

### Added

- Project-local agent skills for schema-v2 extraction/compilation, handwritten
  semantic GPUI integration, and source-linked visual verification.
- Isolated Inter 3.19 and JetBrains Mono 2.304 preparation with SHA-256 checks for
  the canonical Foundation pixel profile.
- Explicit cleanup for verified temporary Figma transfer nodes.

### Changed

- Generated GPUI functions retain precise Rust 2024 resolver captures and preserve
  SVG fallback semantics separately from raster image rendering.
- Width-hugging text keeps its measured Figma width without unintended wrapping;
  inside strokes render as absolute overlays without changing layout.
- Pixel verification configures grayscale glyph rendering and records exact font
  identity as provenance.

### Fixed

- Large Foundation-style fallback exports no longer stop at the previous asset
  count guard.
- Generated asset-backed views no longer retain resolver borrows beyond their
  required lifetime.
- The plugin bridge uses the manifest-compatible `localhost` origin while the
  service remains bound to IPv4 loopback.

### Known Limitations

- Authored-color SVG fallback is still tracked by issue
  [#6](https://github.com/ugur-murat-alt/figma-rust/issues/6); compile success does
  not prove that every SVG paints correctly on pinned GPUI.
- Private Foundation and DataTable extraction bundles, SVG payloads, screenshots,
  and unsanitized evidence are intentionally excluded from this release.
- The canonical Foundation profile is source-specific and must not be reused as a
  universal font or pixel threshold policy.

## [0.3.0] - 2026-08-26

### Added

- Separate compact compiler and optional REST-evidence JSON exports with node,
  variable, asset, diagnostic, and UTF-8 size summaries.
- Optional base64 SVG/PNG fallback payloads, deterministic asset file names,
  `asset-manifest.json`, stale-asset cleanup, and runtime `AssetResolver` support.
- A pinned-GPUI synthetic fixture for the generated SVG fallback path.

### Changed

- Opaque `PASS_THROUGH` groups remain native while non-opaque pass-through cases
  take the explicit runtime fallback route.
- Nonzero letter spacing now uses SVG fallback instead of being silently dropped.
- Preview and lint extraction omit asset payloads; compiler, evidence, and Dev Mode
  compiler paths retain payloads when available.

### Fixed

- Collapsed captured fallback subtrees to one generated asset without repeating
  nested SVG/raster exports while retaining reaction and component validation.
- Preserved UI error reporting when asynchronous plugin actions reject.
- Removed ambiguity between ordinary image assets and raster fallback payloads.

### Known Limitations

- Rich image crop/fit and runtime paint semantics still require isolated fixtures.
- The complete Orbitline Design node `419:2` extraction is not part of this public
  release fixture set, so its final geometry and pixel fidelity remain unverified.

## [0.2.0] - 2026-08-26

### Added

- Mode-aware variable extraction keyed by the complete standard collection-to-mode context.
- Context-aware color and number token resolution in the GPUI runtime.
- Numeric token bindings for Auto Layout gap and padding, independent corner radii, and per-edge stroke widths.
- Regression fixtures for multi-mode aliases, alias cycles, strict schema parsing, and legacy schema rejection.

### Changed

- Bumped the extraction schema and Design IR to version 2.
- Generated GPUI now lowers bound numeric values with literal fallbacks and preserves source-map ranges through the generated function closing line.
- Migrated checked-in extraction, IR, lint, inspect, and generated-code fixtures to schema version 2.

### Fixed

- Preserved multiple values for the same Figma variable when consumers resolve under different mode contexts.
- Prevented modeled numeric bindings and mixed per-edge stroke widths from being rejected during extraction.
- Kept schema version 1 readable only long enough to return the explicit `FR-SCHEMA-001` compatibility diagnostic.

### Known Limitations

- Enterprise extended-variable-collection overrides are not yet covered by a public fixture.
- Runtime asset routes remain an explicit GPUI code-generation boundary.

[0.4.0]: https://github.com/ugur-murat-alt/figma-rust/releases/tag/v0.4.0
[0.3.1]: https://github.com/ugur-murat-alt/figma-rust/releases/tag/v0.3.1
[0.3.0]: https://github.com/ugur-murat-alt/figma-rust/releases/tag/v0.3.0
[0.2.0]: https://github.com/ugur-murat-alt/figma-rust/releases/tag/v0.2.0
