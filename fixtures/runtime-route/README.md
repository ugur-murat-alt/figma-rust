# Runtime-route fail-closed fixture

Node `11:runtime` is a sanitized synthetic mixed-track grid. The normalized IR
retains `AssetRoute::Runtime`, but normal lint and compile fail before codegen with
node/property-scoped `FR-ASSET-001` at `asset_decision.route`. This is deliberate:
no generated GPUI is emitted until a verified runtime primitive owns the route.

```sh
cargo run -p figma-rust-cli -- lint fixtures/runtime-route/extraction.json --json
cargo run -p figma-rust-cli -- compile fixtures/runtime-route/extraction.json --out /tmp/runtime-route
```

Both commands exit `1`. The diagnostic help permits a source SVG/raster fallback
only when that fallback is semantically valid; it does not silently change routes.
