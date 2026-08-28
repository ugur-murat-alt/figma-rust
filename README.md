# figma-rust

`figma-rust` is a deterministic Figma-to-Rust GPUI design compiler. It consumes
structural Figma data, normalizes it into a target-neutral Design IR, emits GPUI
code against a pinned upstream revision, and verifies structure, geometry, and
render artifacts.

It is not screenshot-to-code and does not infer application business logic.

## Status

The project is under active construction. Current scope and honest capability
limits are documented in:

- [`docs/research.md`](docs/research.md)
- [`docs/figma-gpui-capability-matrix.md`](docs/figma-gpui-capability-matrix.md)
- [`docs/architecture.md`](docs/architecture.md)
- [`docs/plan.md`](docs/plan.md)

## Intended CLI

```text
figma-rust inspect <extraction.json>
figma-rust lint <extraction.json>
figma-rust compile <extraction.json> --out generated/
figma-rust verify <verification.json>
figma-rust serve
```

## Local Installation

Install or update the CLI from this checkout without publishing to crates.io:

```sh
cargo install --path crates/figma-rust-cli --root ~/.local --locked --force
~/.local/bin/figma-rust version
```

Build the local Figma development plugin, then import `plugin/manifest.json`
from Figma Desktop:

```sh
npm --prefix plugin ci
npm --prefix plugin run build
```

For Dev Mode Codegen feedback, keep the loopback compiler bridge running in a
separate terminal:

```sh
~/.local/bin/figma-rust serve --port 38421
```

Generated code never owns handwritten application behavior. Unsupported source
features produce node-scoped diagnostics instead of disappearing silently.
The plugin's **Export compiler JSON** path omits the optional REST snapshot and
reports bundle size before the CLI publishes generated Rust, sidecars,
`asset-manifest.json`, and decoded SVG/PNG fallback files under a directory lock
with staged writes and handled-failure rollback.

## Linux Render Capture

Pinned GPUI does not expose Linux client-texture readback. The project command
`/capture-linux fixtures/real-figma` uses Computer Use to capture the exact GPUI
window through the Wayland compositor over black and white backdrops, reconstructs
straight-alpha RGBA, and runs the zero-tolerance verifier. Rust validates and
atomically publishes every PNG and provenance artifact; Computer Use only owns the
external compositor screenshot step.

## OpenCode2 Skill

The repository ships a project-local `figma-rust` skill under
`.opencode/skills/figma-rust/`. OpenCode2 discovers it automatically when started
from this repository or a child directory.

```sh
opencode2
```

To make the same skill suite available outside this checkout, add the published
OpenCode V2 plugin at an exact version:

```jsonc
{
  "plugins": ["@vaur94/figma-rust@0.4.0"]
}
```

The npm package registers only missing skill IDs; repository-local copies remain
authoritative when they are present. It does not contain the compiler CLI or the
Figma development plugin.

Ask OpenCode2 to load `figma-rust` explicitly for extraction, compilation,
semantic GPUI integration, verification, capture, debugging, or compiler
development work. It is the umbrella router for three task skills:

- `figma-rust-extract-compile`: schema-v2 plugin extraction, diagnostics,
  deterministic compilation, generated artifacts, and the loopback bridge;
- `figma-rust-semantic-gpui`: handwritten Foundation tokens/components,
  variants/actions, resolver wiring, and application ownership boundaries;
- `figma-rust-visual-verification`: source-linked geometry and pixels, exact
  fonts, Linux compositor capture, thresholds, hashes, and provenance.

For end-to-end work, agents use them in that order and keep compilation, GPUI
integration, geometry, and pixel evidence as separate gates. The suite contains:

- the complete plugin, CLI, server, verification, capture, and development workflow;
- capability and diagnostic interpretation rules;
- a mandatory decision gate that creates one GitHub issue per independently
  reproducible non-security root cause or coherent improvement found during use;
- sanitized bug and improvement templates.

Security-sensitive findings must never be opened as public issues. See
[`.opencode/skills/figma-rust/references/issue-policy.md`](.opencode/skills/figma-rust/references/issue-policy.md)
and [`SECURITY.md`](SECURITY.md).
