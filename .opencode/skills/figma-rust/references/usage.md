# Complete usage guide

All commands run from the repository root unless a command sets another directory. The project is under active construction; use checked-in fixtures and diagnostics as the source of truth.

## 1. One-time prerequisites

Required tools:

- Linux or macOS, or Windows through a POSIX-compatible shell such as WSL or Git
  Bash, for normal compiler work. The command examples use POSIX shell syntax.
- Rust `1.97.1` with `rustfmt` and `clippy`; `rust-toolchain.toml` pins this automatically.
- Node.js and npm for the Figma plugin.
- Figma Desktop for loading the local development plugin.
- Linux Wayland plus Computer Use/desktop-portal access for fresh Linux pixel capture.
- `jq`, `sha256sum`, `unzip`, `fc-match`, and `ffmpeg`/`ffprobe` for capture provenance workflows.
- GitHub CLI `gh` authenticated for mandatory issue creation.

Use Cargo from `PATH`, or set `CARGO` to an absolute executable path when the environment does not expose it:

```sh
"${CARGO:-cargo}" --version
```

Install plugin dependencies deterministically and build both surfaces:

```sh
npm --prefix plugin ci
npm --prefix plugin test
npm --prefix plugin run check
npm --prefix plugin run build
"${CARGO:-cargo}" build -p figma-rust-cli
"${CARGO:-cargo}" run -p figma-rust-cli -- version
```

Expected plugin build outputs are ignored artifacts under `plugin/dist/`.

Install or update the local CLI after all release checks pass:

```sh
cargo install --path crates/figma-rust-cli --root ~/.local --locked --force
~/.local/bin/figma-rust version
```

This is a local path installation; the workspace crates remain `publish = false`
and are not published to crates.io.

For the canonical Foundation `419:2` pixel profile, prepare exact fonts without
changing global Fontconfig state:

```sh
export FONTCONFIG_FILE="$(
  .opencode/skills/figma-rust/scripts/prepare-fidelity-fonts.sh
)"
```

See `references/fidelity-fonts.md` for hashes, scope, and the required GPUI
grayscale configuration. Other Figma files require their own source-font proof.

## 2. Load the Figma development plugin

1. Run `npm --prefix plugin run build`.
2. In Figma Desktop, import a development plugin from `plugin/manifest.json`.
3. Open a Figma design file and select the smallest frame, component, instance, or scene-node root that owns the behavior under test.
4. Use **Lint selection** first to review extraction diagnostics.
5. Use **Export compiler JSON** for the compact CLI input. Use **Export evidence JSON** only when the optional `JSON_REST_V1` snapshot is required.
6. Move the downloaded bundle into a purpose-specific fixture directory only after checking that it contains no private material that should not enter Git.

The bundle contract contains:

```text
schema_version (currently 2)
source { file_key?, page_id, selected_node_ids, plugin_api_version }
roots
variables
components
assets
extraction_diagnostics
rest_snapshot?
```

Assets can carry optional base64 payloads. Preview/lint extraction omits those
payloads; compiler/evidence exports include them when an SVG/raster fallback or
image asset is available. The UI reports the UTF-8 JSON size and warns at 2 MiB
and above or when the 8 MiB loopback limit is exceeded.

Schema v2 is required for multi-mode and modeled numeric bindings. A token-bound
value carries its literal fallback and consumer collection/mode context. V1
bundles remain readable only so lint/compile can emit `FR-SCHEMA-001`; re-extract
or explicitly migrate them instead of relabeling them.

Do not merge roots from different Figma pages into one bundle. Extract them separately so `source.page_id` stays truthful.

## 3. Inspect and lint an extraction

Build once or use Cargo directly:

```sh
"${CARGO:-cargo}" run -p figma-rust-cli -- \
  inspect path/to/extraction.json

"${CARGO:-cargo}" run -p figma-rust-cli -- \
  inspect path/to/extraction.json --json

"${CARGO:-cargo}" run -p figma-rust-cli -- \
  lint path/to/extraction.json

"${CARGO:-cargo}" run -p figma-rust-cli -- \
  lint path/to/extraction.json --json
```

Use `--strict` only for a gate where warnings must fail:

```sh
"${CARGO:-cargo}" run -p figma-rust-cli -- \
  lint path/to/extraction.json --strict
```

Interpretation:

- exit `0`: command completed and the selected policy passed;
- exit `1`: the document has domain diagnostics that fail normal/strict policy;
- exit `2`: file, JSON, manifest, or verifier configuration could not be read/processed.

Always inspect diagnostic `code`, `node_id`, and `property_path` before editing source.

## 4. Compile deterministic GPUI artifacts

```sh
output=$(mktemp -d "${TMPDIR:-/tmp}/figma-rust-output.XXXXXX")
"${CARGO:-cargo}" run -p figma-rust-cli -- \
  compile path/to/extraction.json --out "$output"
```

On success the compiler owns exactly:

- `generated.rs`
- `source-map.json`
- `ir.json`
- `diagnostics.json`
- `asset-manifest.json`
- zero or more deterministic flat asset files named by that manifest

Treat the fixed files, manifest, and listed asset files as one compiler-owned,
lock-protected artifact set. Each file is staged, synced, and published with a
same-directory rename; handled failures attempt to restore the previous set.
Readers that do not take the directory lock can still observe rename transitions.
Do not add handwritten behavior to `generated.rs`. Integrate by calling generated
view functions from application-owned code and handling actions/state outside
generated directories. A view with fallback assets also accepts an
`AssetResolver`; `figma_gpui_runtime::DirectoryAssets` resolves files from the
compiler output directory.

Compilation can stop in two places:

- normalization fails because Raw Model/Design IR invariants contain errors;
- code generation rejects a valid IR feature that lacks a proven GPUI lowering.

These are different defects/capability gaps and must remain distinct in diagnostics and issues.

A successful `figma-rust compile` does not invoke Cargo on the generated source. Prove pinned-GPUI API compatibility with the relevant generated fixture crate before claiming that gate.

## 5. Run the Figma Codegen bridge

Start the loopback-only service:

```sh
"${CARGO:-cargo}" run -p figma-rust-cli -- serve --port 38421
```

The development plugin manifest permits only `http://localhost:38421`; the service remains bound to the IPv4 loopback address `127.0.0.1`. The service exposes fixed `POST /lint` and `POST /compile` endpoints, rejects query/command paths, limits requests to 8 MiB, and performs no Cargo/render work.

Use this bridge for quick Figma Dev Mode feedback. Use the CLI for output files, assets, Cargo, render capture, and fidelity verification.

## 6. Verify geometry and pixels

A verification manifest names reference/actual geometry, optional reference/actual images, and thresholds. Example:

```json
{
  "reference_geometry": "reference.geometry.json",
  "actual_geometry": "actual.geometry.json",
  "reference_image": "reference.png",
  "actual_image": "actual.png",
  "thresholds": {
    "geometry_absolute_px": 0.0,
    "geometry_relative": 0.0,
    "mean_absolute_pixel_error": 0.0,
    "changed_pixel_ratio": 0.0,
    "edge_error": 0.0,
    "minimum_ssim": 1.0
  }
}
```

Run:

```sh
"${CARGO:-cargo}" run -p figma-rust-cli -- \
  verify path/to/verify.json --json
```

Interpretation:

- exit `0`: report completed and thresholds passed;
- exit `1`: report completed but one or more thresholds failed;
- exit `2`: verification could not be performed.

A report with `passed:false` can be useful evidence but is not a passing fixture. Preserve the report and state whether failure is expected or a regression.

Before opening a pixel-verification window, call
`figma_gpui_runtime::configure_figma_fidelity(cx)`. Run that process with an
isolated `FONTCONFIG_FILE` containing the exact source font files, and record the
font hashes and text mode in provenance. Geometry can pass while font mismatch,
subpixel antialiasing, or unintended text wrapping still fails pixels.

The canonical real fixture is reproducible offline:

```sh
"${CARGO:-cargo}" run -p figma-rust-cli -- \
  verify fixtures/real-figma/verify.geometry.json --json
"${CARGO:-cargo}" run -p figma-rust-cli -- \
  verify fixtures/real-figma/verify.image.json --json
```

## 7. Capture fresh Linux Wayland evidence

Pinned GPUI does not implement Linux client-texture readback. Use the project OpenCode command:

```text
/capture-linux fixtures/real-figma
```

That command is the authoritative workflow. It loads the capture/publication skills, waits for first-frame readiness, targets the exact compositor window and PID, captures black and white scale-1 portal composites, validates data-URL ingest, reconstructs straight-alpha RGBA, runs zero-tolerance verification, and finalizes hash-bound provenance.

Fail closed. Do not replace checked-in capture artifacts after any mismatch in PID, title/class, size, scale, source, MIME type, reconstruction, report, or cleanup.

For opaque hand-authored proofs such as `fixtures/orbitline-figma-mcp`, clearly separate:

- the hand-authored GPUI rendering;
- the real extractor/IR/codegen reports;
- the retained compositor source and its codec/loss characteristics;
- the comparison result and thresholds.

## 8. Develop the compiler by ownership layer

Choose the owning layer before changing code:

- `plugin/`: Figma Plugin API extraction, source diagnostics, JSON export, Codegen callback.
- `figma-rust-core`: Raw Model, normalization, Design IR, token/component metadata, asset decisions.
- `figma-rust-codegen`: GPUI target validation/lowering and deterministic Rust/source maps.
- `figma-gpui-runtime`: only a proven GPUI capability gap with an isolated fixture.
- `figma-rust-cli`: inspect/lint/compile/serve/verify orchestration and artifact publication.
- `fixtures/`: compile, geometry, pixel, capture, and regression evidence.

For each new capability:

1. verify Figma semantics from the Plugin API/typings;
2. verify the pinned GPUI API/source;
3. create the smallest failing fixture;
4. change the owning extractor/IR/codegen/runtime layer;
5. add node-scoped diagnostics and regression tests;
6. run unit/golden/plugin checks;
7. compile generated Rust;
8. capture geometry/pixels when behavior is visual;
9. compare and diagnose the actual difference;
10. update the capability matrix/plan when support status changes;
11. apply the mandatory GitHub issue decision from `SKILL.md`.

## 9. Validation matrix

Rust-only changes:

```sh
"${CARGO:-cargo}" fmt --all -- --check
"${CARGO:-cargo}" clippy --all-targets -- -D warnings
"${CARGO:-cargo}" test
```

Plugin changes:

```sh
npm --prefix plugin test
npm --prefix plugin run check
npm --prefix plugin run build
```

Capture-feature changes:

```sh
"${CARGO:-cargo}" test -p figma-generated-gpui-fixture --features capture-real-window
"${CARGO:-cargo}" clippy -p figma-generated-gpui-fixture --features capture-real-window --all-targets -- -D warnings
"${CARGO:-cargo}" build -p figma-generated-gpui-fixture --features capture-real-window
```

Do not report a check as passed unless it actually ran. After failures, diagnose and retry rather than skipping the gate.
