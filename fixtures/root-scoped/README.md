# Root-scoped compile fixture

This sanitized schema-v2 bundle contains three selected roots and shared token/
asset metadata:

- `28:valid` is a native frame with a bound color and one SVG child; it compiles
  into an isolated deterministic artifact directory.
- `28:invalid` has an invalid child counter-axis override and retains its
  node/property-scoped normalization diagnostic.
- `28:runtime` requires the unsupported mixed-grid runtime route and retains
  `FR-ASSET-001` instead of disappearing.

Normal `compile` remains strict and publishes no outputs for this bundle. Explicit
`compile --root-scoped` writes `root-status.json`, succeeds for the valid root,
keeps the two failures separate, and returns exit 1 because the report is partial.
