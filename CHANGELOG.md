# Changelog

All notable changes to this project are documented in this file.

## Unreleased

### Changed

- Enterprise extended variable collection contexts now fail closed with
  node/property-scoped `FR-TOKEN-MODE-005` before alias or value resolution;
  schema v2 continues to support standard collection mode contexts only.

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

[0.3.0]: https://github.com/ugur-murat-alt/figma-rust/releases/tag/v0.3.0
[0.2.0]: https://github.com/ugur-murat-alt/figma-rust/releases/tag/v0.2.0
