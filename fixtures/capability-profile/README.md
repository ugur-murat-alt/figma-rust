# Capability profile fixture

`extraction.json` is a synthetic, public schema-v2 bundle for the versioned
`orbitline-minimal-v1` usage profile. It contains no file key, private Figma
content, screenshots, or customer data.

Generate the deterministic inventory:

```sh
cargo run -p figma-rust-cli -- \
  profile fixtures/capability-profile/extraction.json \
  --profile orbitline-minimal-v1 --json
```

`orbitline-minimal-v1.report.json` is the expected report. The profile requires
bounded schema-v2 extraction, component metadata, modeled token dimensions, child
alignment, and typed text metadata; asset payload export is optional. REST
snapshot/import, Code Connect, mixed grid, media/pattern/shader paints, generated
action contracts, and custom effects remain explicitly quarantined. Missing
required, present quarantined, undeclared manifest capabilities, and bundle-level
extension fields fail with `FR-PROFILE-*` diagnostics before an opt-in profiled
compile reaches normalization.

The `advertised` field means the extraction manifest or an explicit bundle field
declares the capability. It does not claim that every source node exercises that
feature.
