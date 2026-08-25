# OrbitLine Figma MCP conversion proof

This isolated fixture does not read or modify the local OrbitLine repository.
It evaluates two concrete nodes from Figma file `AShHVrUqsKDWavqH8D4S09`
against the pinned GPUI revision used by `figma-rust`:

- DataTable / Ready: `516:223`, 520x176
- DialogBody / Confirm: `518:96`, 460x220

## Visual proof

`reference.*.png` files are exact Figma MCP screenshots. `actual.*.png` files are
PNG frames decoded from retained Wayland VP8 screencasts of real windows rendered
by `orbitline-mcp-proof`.
`capture.compositor.json` retains the binary's readiness event, the compositor's
selected window record, activation result, capture timing, dimensions, GPUI
revision, and output hash in one record. `proof.png` places each Figma reference
beside its corresponding GPUI capture.

The GNOME Shell `StopScreencast` call was made from a separate D-Bus client and
returned `false`; the fixture does not treat that value as success. Instead it
retains `capture.*.webm`, binds each source hash and `ffprobe` result, and records
the successful `ffmpeg` first-frame decode whose bytes equal the corresponding
`actual.*.png` file.

The comparison manifests intentionally use zero tolerance. Their reports are
expected to fail because this fixture demonstrates visual similarity, not pixel
identity. The geometry comparison covers the exact viewport only; child geometry
is not claimed for the hand-authored proof binary.

- Table: MAE 0.02445, changed pixels 13.184%, edge error 0.01718, SSIM 0.50640.
- Dialog: MAE 0.01686, changed pixels 5.548%, edge error 0.01018, SSIM 0.55091.

```sh
cargo run -p figma-rust-cli -- verify \
  fixtures/orbitline-figma-mcp/compare.table.json --json
cargo run -p figma-rust-cli -- verify \
  fixtures/orbitline-figma-mcp/compare.dialog.json --json
```

## Compiler pipeline

`extraction.table.json` and `extraction.dialog.json` were produced by the project's
real `extractNodes` implementation through the Figma Plugin API. The nodes live on
different Figma pages, so each extraction retains its own `page_id`.

- Dialog: 18 nodes; extraction and normalization complete with 11 warnings and no
  errors. Code generation stops at the root runtime asset route caused by
  pass-through compositing.
- Table: 66 nodes; normalization stops on 9 extraction errors: four unsupported
  bound corner-radius tokens and five mixed stroke-width values. Its 65 text/stroke
  preservation warnings remain node-scoped in the checked-in reports.

`inspect.*.json`, `lint.*.json`, and `compile.*.stderr` are the corresponding CLI
artifacts. The hand-authored `orbitline-mcp-proof` output must not be described as
code generated from these extractions.

## Known visual losses

The conversion deliberately keeps the Figma token values and dimensions. GPUI at
the pinned revision has no letter-spacing style method, so the table header's
0.4px tracking is the one known source-level omission. Font rasterization can also
differ between Figma's renderer and the Linux compositor, and VP8 compression can
contribute additional pixel-level error to the comparison metrics.
