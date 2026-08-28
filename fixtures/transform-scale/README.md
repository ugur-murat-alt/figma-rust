# Uniform subtree transform fixture

Synthetic nodes `15:center` and `15:top-left` exercise the bounded transform
lowering: positive uniform scale plus translation. A 100x100 subtree at scale
`0.985` becomes 98.5x98.5; translation `(0.75, 0.75)` expresses a centered
origin while `(0, 0)` expresses a top-left origin.

The lowering scales concrete layout, text, stroke, radius, and shadow geometry,
so GPUI clipping and hit testing use the transformed bounds. Skew, rotation,
non-positive/non-uniform scale, and scaled variable-bound numeric geometry stay
node/property-scoped codegen errors rather than approximate output.

```sh
cargo run -p figma-rust-cli -- compile fixtures/transform-scale/extraction.json \
  --out fixtures/transform-scale/generated
```
