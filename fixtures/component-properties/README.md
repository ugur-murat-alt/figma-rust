# Bound component property fixture

Synthetic component roots `27:light` and `27:dark` exercise schema-v2 TEXT and
BOOLEAN component-property consumers in two variable modes. Their text children
reference `Label#27:1`; optional rectangle children reference `Visible#27:2`.

Generated code calls context-aware string and boolean token resolvers while
retaining `Light label`/`Dark label` and `true`/`false` as literal fallbacks.
`FallbackTokens` therefore proves the missing-resolver path without adding
application state or behavior.

```sh
cargo run -p figma-rust-cli -- compile \
  fixtures/component-properties/extraction.json \
  --out fixtures/component-properties/generated
```
