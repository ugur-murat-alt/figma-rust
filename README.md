# figma-rust

`figma-rust` is a deterministic Figma-to-Rust GPUI design compiler. It consumes
structural Figma data, normalizes it into a target-neutral Design IR, emits GPUI
code against a pinned upstream revision, and verifies structure, geometry, and
render artifacts.

It is not screenshot-to-code and does not infer application business logic.

## GPUI Design direction

The repository is evolving into **GPUI Design**: a Figma-independent authoring
engine and MCP whose editable source of truth understands tokens, semantic
components, modules, shells, Rust symbol ownership, and actual GPUI render
evidence. Figma remains an optional import and historical-evidence adapter; it is
not the long-term authority for OrbitLine design work.

The migration is incremental. The existing extraction, compiler IR, deterministic
GPUI code generation, pinned-revision fixtures, and visual verification system
remain in place while a native authoring layer is added above them.

The first foundation is available in:

- `crates/gpui-design-core`: versioned authoring documents, component-to-shell
  contracts, token scopes, Rust code bindings, deterministic validation,
  revisioned commands, and idempotent transactions;
- `crates/gpui-design-mcp`: process-local MCP tools, resources, and prompts over
  newline-delimited stdio JSON-RPC;
- `.opencode/skills/gpui-design`: the OpenCode2 operating workflow;
- [`docs/gpui-design-mcp-architecture.md`](docs/gpui-design-mcp-architecture.md):
  target studio, preview, lowering, code-index, security, and migration design.

Run the MCP server from the repository root:

```sh
cargo run -p gpui-design-mcp
```

The current MCP foundation validates and revises in-memory authoring documents.
Native authoring-to-compiler-IR lowering, real GPUI preview/hit testing, durable
project storage, AST-safe Rust reconciliation, and the web studio are explicit
subsequent phases; the current lowering manifest is readiness evidence, not a
claim that code or pixels were produced.

OrbitLine production code should consume approved semantic APIs such as
`orbit_ui::*`. Raw GPUI primitives and styling belong inside the framework and
renderer implementation, not in arbitrary product screens or agent-generated
one-off widgets.

## Status

The project is under active construction. Current scope and honest capability
limits are documented in:

- [`docs/research.md`](docs/research.md)
- [`docs/figma-gpui-capability-matrix.md`](docs/figma-gpui-capability-matrix.md)
- [`docs/architecture.md`](docs/architecture.md)
- [`docs/plan.md`](docs/plan.md)
- [`docs/gpui-design-mcp-architecture.md`](docs/gpui-design-mcp-architecture.md)

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

## OpenCode2 Skills

The repository ships project-local skills under `.opencode/skills/`. OpenCode2
discovers them automatically when started from this repository or a child
directory.

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
authoritative when they are present. It does not contain the Rust binaries or the
Figma development plugin.

Use `gpui-design` for Figma-independent authoring, token/component/module/shell
contracts, design transactions, MCP operation, code ownership, and lowering
readiness. Use `figma-rust` when the task still depends on Figma extraction or the
compatibility compiler path. The compatibility umbrella routes three task skills:

- `figma-rust-extract-compile`: schema-v2 plugin extraction, diagnostics,
  deterministic compilation, generated artifacts, and the loopback bridge;
- `figma-rust-semantic-gpui`: handwritten Foundation tokens/components,
  variants/actions, resolver wiring, and application ownership boundaries;
- `figma-rust-visual-verification`: source-linked geometry and pixels, exact
  fonts, Linux compositor capture, thresholds, hashes, and provenance.

For end-to-end work, agents keep authoring, lowering, compilation, GPUI
integration, geometry, and pixel evidence as separate gates. The suite contains:

- the GPUI-native authoring/MCP workflow and the compatibility plugin/compiler;
- capability and diagnostic interpretation rules;
- a mandatory decision gate that creates one GitHub issue per independently
  reproducible non-security root cause or coherent improvement found during use;
- sanitized bug and improvement templates.

Security-sensitive findings must never be opened as public issues. See
[`.opencode/skills/figma-rust/references/issue-policy.md`](.opencode/skills/figma-rust/references/issue-policy.md)
and [`SECURITY.md`](SECURITY.md).
