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

Generated code never owns handwritten application behavior. Unsupported source
features produce node-scoped diagnostics instead of disappearing silently.

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

Ask OpenCode2 to load `figma-rust` explicitly for extraction, compilation,
verification, capture, debugging, or compiler development work. The skill contains:

- the complete plugin, CLI, server, verification, capture, and development workflow;
- capability and diagnostic interpretation rules;
- a mandatory decision gate that creates one GitHub issue per independently
  reproducible non-security root cause or coherent improvement found during use;
- sanitized bug and improvement templates.

Security-sensitive findings must never be opened as public issues. See
[`.opencode/skills/figma-rust/references/issue-policy.md`](.opencode/skills/figma-rust/references/issue-policy.md)
and [`SECURITY.md`](SECURITY.md).
