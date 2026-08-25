# Contributing

## Before opening an issue

Search open and closed issues, then compare the observation with `docs/plan.md`,
`docs/figma-gpui-capability-matrix.md`, and the relevant fixture README.

- Use the bug form for reproducible wrong behavior.
- Use the improvement form for a bounded proposal with observable acceptance
  criteria.
- Do not open public issues for security-sensitive findings or private Figma data.
  Follow `SECURITY.md` instead.

The project-local OpenCode2 `figma-rust` skill defines the full evidence and
duplicate-search policy in
`.opencode/skills/figma-rust/references/issue-policy.md`.

## Change boundaries

- Keep generated code separate from handwritten business logic.
- Preserve Figma node IDs and node/property-scoped diagnostics.
- Add the smallest fixture that proves the changed capability.
- Do not claim geometry or pixel fidelity from compile success alone.
- Do not update the Rust toolchain, GPUI revision, schema, or dependencies as
  incidental cleanup.

## Checks

Run the checks that match the changed area:

```sh
"${CARGO:-cargo}" fmt --all -- --check
"${CARGO:-cargo}" clippy --all-targets -- -D warnings
"${CARGO:-cargo}" test
npm --prefix plugin test
npm --prefix plugin run check
npm --prefix plugin run build
```

Capture-related changes also require feature-enabled fixture tests, exact window
evidence, artifact hashes, and the applicable verifier report.
