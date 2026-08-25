# Real Figma GROUP fixture

- Source: <https://www.figma.com/design/XRjBi41NCw78G6lkyJjmbV>
- Root node: `1:4`
- Plugin API typings: `1.135.0`
- Reference PNG: `reference.png`, 100x60 RGBA, SHA-256
  `00b870f8e967362deb405959a68758d31f10615aa949eb44e7cfa6cdad84db89`

Figma reports the group at `(160, 180)`. Its children are reported at
`(160, 180)` and `(230, 220)`, because `x`, `y`, and `relativeTransform` skip
`GROUP` as a containing parent. The extractor rebases these to direct-parent
coordinates `(0, 0)` and `(70, 40)` before normalization.

`plugin/tests/fixtures/real-group.plugin-api.json` is the Plugin API snapshot
captured from this file. `extraction.json` is the corresponding versioned Raw
Model fixture consumed by the CLI.

The bounds in `actual.geometry.json` are produced from GPUI's `test-support`
layout pass, not from the Figma reference. Source IDs, hierarchy, and sibling
indices are fixture metadata tied to the generated source map. The
generated-fixture test opens a deterministic 100x60 `TestWindow`, reads each
generated `debug_selector` through `VisualTestContext::debug_bounds`, and
validates the complete checked-in snapshot:

```sh
cargo test -p figma-generated-gpui-fixture real_group_geometry_is_measured_by_gpui
cargo run -p figma-rust-cli -- verify fixtures/real-figma/verify.geometry.json
```

The geometry manifest uses zero tolerance and compares root/child hierarchy,
sibling order, and bounds. `verify.image.json` additionally compares the captured
RGBA pixels at zero tolerance.

## Linux compositor capture status

The explicit capture probe opens the generated view in a real 100x60 transparent
Wayland client window, waits until that frame is rendered, and asks pinned GPUI
to read the rendered client texture:

```sh
cargo run -p figma-generated-gpui-fixture \
  --features capture-real-window \
  --bin capture-real-group -- \
  fixtures/real-figma/actual.png
```

On this host (`XDG_SESSION_TYPE=wayland`, `WAYLAND_DISPLAY=wayland-0`) the window
opens and reports a 100x60 client, but GPUI returns
`render_to_image not implemented for this platform`; the command exits 1 and
does not create `actual.png`. This is a pinned-backend limitation rather than a
headless or missing-compositor result:

- `PlatformWindow::render_to_image` has a default unsupported implementation.
- The Linux Wayland window does not override that method.
- GPUI's compositor-backed `VisualTestAppContext` is exported only on macOS.
- Wayland `screen_capture_sources` separately returns
  `Wayland screen capture not yet implemented.`

The fixture does not stop at that unavailable API. The explicit display mode keeps
the rendered window open over a controlled opaque backdrop and emits a JSON
readiness event after the first frame:

```sh
target/debug/capture-real-group --display black
target/debug/capture-real-group --display white
```

For each run, capture the window titled `figma-rust-real-group-ready` through the
desktop portal without resizing. Require both coordinate and payload dimensions
to be 100x60 with scale 1. Reconstruct straight-alpha RGBA from the two opaque
composites and publish it atomically:

```sh
printf '%s' "$COMPUTER_USE_DATA_URL" | target/debug/capture-real-group \
  --ingest-data-url fixtures/real-figma/capture.black.png

target/debug/capture-real-group \
  --reconstruct fixtures/real-figma/capture.black.png \
  fixtures/real-figma/capture.white.png \
  fixtures/real-figma/actual.png

target/debug/figma-rust verify fixtures/real-figma/verify.image.json --json
```

The project-local OpenCode command `/capture-linux fixtures/real-figma` runs the
complete Computer Use workflow, including exact window checks, both ingests,
process cleanup, reconstruction, `capture.verify.json`, and provenance
finalization. The exact portal composites are retained as `capture.black.png` and
`capture.white.png`. `capture.compositor.json` records their paths and hashes plus
the portal, window, reconstruction method, final artifact hash, and complete
verifier result. On this host image verification passes at zero tolerance: MAE 0,
changed-pixel ratio 0, edge error 0, and SSIM 1. Every Rust publication path uses a
crash-safe lock, unique temporary sibling, sync, and atomic rename.
