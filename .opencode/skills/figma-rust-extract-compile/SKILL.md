---
name: figma-rust-extract-compile
description: Extract Figma selections into schema-v2 figma-rust bundles, inspect
  and lint diagnostics, compile deterministic GPUI artifacts, operate the local
  plugin/loopback bridge, or debug failures in that pipeline. Use for extraction
  JSON, plugin export, inspect, lint, compile, serve, generated artifacts,
  source maps, or asset manifests.
metadata:
  oc-skill-power: managed
  oc-skill-power-category: development
---
# figma-rust Extraction and Compilation

Use this skill for the pipeline from a Figma selection to compiler-owned GPUI artifacts. Load the umbrella `figma-rust` skill first. This project is not an MCP server: the source extractor is a local Figma development plugin, the compiler/verifier is a Rust CLI, and `serve` is a loopback HTTP bridge used only by that plugin.

## Trigger and outcome

Use this skill when the task includes any of these:

- export a Figma selection for figma-rust;
- inspect or lint an extraction bundle;
- generate or regenerate `generated.rs` and sidecars;
- run or diagnose the Figma Codegen bridge;
- prove deterministic output or pinned-GPUI compilation;
- change the extractor, Raw Model, Design IR, code generator, CLI, or asset publication path.

The successful outcome is a schema-v2 extraction with understood diagnostics plus a complete compiler-owned artifact set. Compilation alone is not proof of GPUI API compatibility, geometry, pixels, or application behavior.

## Required reading and skills

1. Read `.opencode/skills/figma-rust/references/usage.md`, sections 1-5 and 8-9.
2. Read `.opencode/skills/figma-rust/references/capability-and-diagnostics.md` before classifying a warning or unsupported property as a bug.
3. For substantial Rust changes, load `opencode-rust-coder`.
4. When calling Figma MCP `get_design_context`, load `figma-design-to-code` first. Its React/Tailwind output is orientation material, not a replacement for the figma-rust schema-v2 plugin extraction.
5. Never call Figma `use_figma` unless the task requires writing to Figma and `figma-use` has been loaded.

## Non-negotiable boundaries

- Preserve schema version `2`, Rust `1.97.1`, and GPUI revision `5631830c564afa89b3aba679f45d9c3345f9460f` unless the task explicitly upgrades every dependent fixture and proof.
- Do not infer missing Figma values, application state, navigation, validation, networking, or domain behavior.
- Preserve Figma node IDs and property paths through extraction diagnostics, IR, source maps, fixtures, and reports.
- Never edit `generated.rs`, generated sidecars, or decoded compiler assets to repair behavior. Change the owning extractor/core/codegen/runtime layer and regenerate.
- Do not merge roots from different Figma pages; `source.page_id` is singular.
- Do not read or modify any local OrbitLineV2 checkout from this repository.
- Treat extraction bundles, Figma URLs, screenshots, and decoded assets as private until explicitly sanitized.

## Choose one input path

### A. Existing extraction bundle

Use this path when a schema-v2 JSON file already exists. Confirm the file belongs to the intended Figma root and inspect `schema_version`, `source.page_id`, `source.selected_node_ids`, `roots`, `variables`, `components`, `assets`, and `extraction_diagnostics` before compiling.

### B. Local Figma development plugin

Use this path for authoritative extraction:

```sh
npm --prefix plugin ci
npm --prefix plugin test
npm --prefix plugin run check
npm --prefix plugin run build
```

In Figma Desktop, import `plugin/manifest.json`, select the smallest frame/component/instance/scene root that owns the design, run **Lint selection**, then use **Export compiler JSON**. Use **Export evidence JSON** only when the optional REST snapshot is required. Inspect the downloaded bundle for private material before placing it under `fixtures/`.

If desktop interaction is unavailable, stop at the extraction boundary and state that a fresh bundle could not be produced. Do not fabricate a bundle from screenshots or `get_design_context` output.

### C. Dev Mode Codegen bridge

Use the bridge only for quick preview feedback:

```sh
"${CARGO:-cargo}" run -p figma-rust-cli -- serve --port 38421
```

The plugin calls fixed `POST /lint` and `POST /compile` routes at `http://localhost:38421`; the service binds `127.0.0.1`, accepts at most 8 MiB, and performs no Cargo build, renderer launch, capture, or artifact publication. Use the CLI path for durable outputs.

## Step-by-step workflow

### 1. Establish the repository baseline

```sh
git status --short --branch
"${CARGO:-cargo}" run -p figma-rust-cli -- version
```

Confirm the active worktree, preserve unrelated changes, and compare the reported GPUI revision with `Cargo.toml`.

### 2. Inspect structure before policy

```sh
"${CARGO:-cargo}" run -p figma-rust-cli -- \
  inspect path/to/extraction.json --json
```

Check root count/order, selected node IDs, page identity, variable/component counts, assets, and extraction diagnostics. If the requested component is absent, re-extract the correct root rather than patching JSON.

### 3. Lint and classify every relevant diagnostic

```sh
"${CARGO:-cargo}" run -p figma-rust-cli -- \
  lint path/to/extraction.json --json
```

Use `--strict` only when warnings are explicitly part of the acceptance gate. For every relevant item retain `severity`, `code`, `node_id`, `property_path`, and message. Compare it with the capability matrix before changing code.

Exit meanings:

- `0`: command and selected policy passed;
- `1`: domain diagnostics failed normal/strict policy;
- `2`: input, JSON, configuration, manifest, or verifier processing failed.

Exit `1` is not a CLI crash. Never downgrade or suppress a diagnostic merely to obtain exit `0`.

### 4. Compile into an isolated directory

For exploratory work, avoid replacing a checked-in fixture:

```sh
output=$(mktemp -d "${TMPDIR:-/tmp}/figma-rust-output.XXXXXX")
"${CARGO:-cargo}" run -p figma-rust-cli -- \
  compile path/to/extraction.json --out "$output"
```

A successful set consists of:

- `generated.rs`;
- `source-map.json`;
- `ir.json`;
- `diagnostics.json`;
- `asset-manifest.json`;
- every flat asset listed by the manifest.

Treat the entire directory as one lock-protected compiler output. Check that source-map entries and diagnostics still identify the source nodes and that every referenced asset exists.

### 5. Prove determinism when claiming it

Compile the same immutable input twice into separate empty directories. Compare sorted file names and SHA-256 hashes for all owned files. A changed byte is a determinism failure unless the input or pinned toolchain changed. Do not compare only `generated.rs` when assets or sidecars are present.

### 6. Prove generated Rust against pinned GPUI

`figma-rust compile` does not run Cargo. Include the generated module in the relevant fixture crate, construct it with `FallbackTokens` and, when required, `DirectoryAssets`, then run a focused check/test and Clippy. Existing examples live in `fixtures/generated-gpui/src/lib.rs`.

Do not claim visual correctness from construction or Cargo success. Route visual work to `figma-rust-visual-verification`.

### 7. Change the owning layer only

- `plugin/`: Plugin API reads, extraction, payload export, source diagnostics, Codegen callback.
- `crates/figma-rust-core/`: schema-v2 Raw Model, validation, normalization, Design IR, token/component metadata, asset decisions.
- `crates/figma-rust-codegen/`: GPUI target validation/lowering, deterministic Rust, source maps.
- `crates/figma-gpui-runtime/`: only small proven GPUI contracts; no controls or application state.
- `crates/figma-rust-cli/`: inspect/lint/compile/serve/verify orchestration and safe artifact publication.
- `fixtures/`: minimal regression and evidence inputs.

For a behavior change, first add the smallest failing synthetic fixture, then change the owner, regenerate goldens, and rerun the narrow failing test before broad gates.

## Failure interpretation

- `FR-SCHEMA-*`: source contract/version problem; re-extract or perform an explicit migration. Never relabel v1 JSON as v2.
- `FR-EXTRACT-*`: the plugin could not represent source data safely; inspect Plugin API data and extraction code.
- `FR-TOKEN-*`: token identity/mode context is unresolved or unsupported; preserve the literal fallback and source identity.
- `FR-COMPONENT-*`: semantic mapping is missing or inconsistent; structural fallback may be expected, but silent disappearance is not.
- `FR-ASSET-*` / `FR-PAINT-*`: export, route, payload, decode, or target-rendering issue; verify manifest and the actual rendered asset.
- `FR-CODEGEN-*`: valid IR lacks a proven GPUI lowering or generated syntax/API is invalid; fix codegen rather than the output.
- warning: represented loss/fallback; usable only if accepted by policy and evidence;
- error: invariant/required representation failed; compilation must not continue as a success.

## Completion gates

Run only gates matching the changed area, but report every one that ran:

```sh
"${CARGO:-cargo}" fmt --all -- --check
"${CARGO:-cargo}" clippy --all-targets -- -D warnings
"${CARGO:-cargo}" test
npm --prefix plugin test
npm --prefix plugin run check
npm --prefix plugin run build
```

Also require, when applicable:

- fresh inspect/lint reports for the target extraction;
- successful deterministic compile with complete manifest assets;
- two-output hash equality for determinism claims;
- focused pinned-GPUI fixture check/Clippy;
- no handwritten edits inside compiler-owned outputs;
- a separate visual gate for any geometry, typography, paint, SVG, raster, effect, clipping, transform, or layout claim.

Finish by applying the GitHub issue decision in the umbrella `figma-rust` skill. Report the input node IDs, output directory, diagnostic counts, artifact hashes, checks, unrun gates, and any private evidence that was intentionally not published.
