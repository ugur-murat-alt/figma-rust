# FIT, FILL, and CROP image fixture

Nodes `16:fit`, `16:fill`, and `16:crop` share one sanitized 4x4 checker PNG.
FIT lowers to GPUI `ObjectFit::Contain`, FILL to `ObjectFit::Cover`, and the
axis-aligned CROP matrix `[0.5, 0, 0, 0.8, 0.25, 0.1]` produces a 200x100 image
at `(-50, -10)` inside the clipped 100x80 node. This preserves a non-centered
focal region and image opacity.

The Figma Plugin API documents `imageTransform` as CROP-only and rotation as
automatic for CROP: <https://developers.figma.com/docs/plugins/api/Paint/#imagepaint>.
Rotation, nonzero filters, TILE, non-axis-aligned/out-of-range transforms, and
dynamic CROP container sizes remain precise unsupported routes.

```sh
cargo run -p figma-rust-cli -- compile fixtures/image-crop/extraction.json \
  --out fixtures/image-crop/generated
```
