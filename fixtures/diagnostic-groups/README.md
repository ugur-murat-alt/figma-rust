# Grouped diagnostics fixture

This synthetic schema-v2 bundle has two selected roots, repeated extraction-loss
and token-loss diagnostics, and one unique fatal error. It proves deterministic
grouping by severity, code, property, and selected root while preserving the
complete lossless list in JSON and `--diagnostics full` output.

Each group reports its full count and at most three unique, sorted samples.
Errors are ordered before warnings and info.

```sh
cargo run -p figma-rust-cli -- inspect \
  fixtures/diagnostic-groups/extraction.json --diagnostics grouped
cargo run -p figma-rust-cli -- lint \
  fixtures/diagnostic-groups/extraction.json --diagnostics full
```
