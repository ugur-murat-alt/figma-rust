# Component set and SLOT compile fixture

This synthetic schema-v2 fixture proves that a `COMPONENT_SET` root keeps its
typed key and SLOT property, the `SLOT` child remains associated through
`component_property_references.main_component`, authored child order survives,
and a nested `SLICE` is omitted from visual IR with `FR-NODE-001`.

Regenerate the compiler-owned artifacts from the repository root:

```sh
"${CARGO:-cargo}" run -p figma-rust-cli -- \
  compile fixtures/component-set-slot/extraction.json \
  --out fixtures/component-set-slot/generated
```

Only the five public compiler artifacts are checked in. Transactional generation
stores, cache directories, lock files, and the current-generation pointer are
not fixture files.
