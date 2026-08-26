# Changelog

All notable changes to this project are documented in this file.

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

[0.2.0]: https://github.com/ugur-murat-alt/figma-rust/releases/tag/v0.2.0
