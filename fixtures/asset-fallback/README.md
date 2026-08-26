# Asset fallback fixture

This synthetic schema-v2 extraction proves the bounded SVG fallback path without
depending on private Figma content. Node `13:1` is exported as one SVG payload;
the CLI publishes its manifest and decoded file under one directory lock with
staged writes and handled-failure rollback, and the generated Rust is compiled by
`figma-generated-gpui-fixture` against the pinned GPUI revision.

Regenerate from the repository root:

```sh
cargo run -p figma-rust-cli -- \
  compile fixtures/asset-fallback/extraction.json \
  --out fixtures/asset-fallback/generated
```
