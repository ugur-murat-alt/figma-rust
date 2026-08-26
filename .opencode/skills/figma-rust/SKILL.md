---
name: figma-rust
description: Operate, extend, debug, and verify the deterministic Figma-to-Rust
  GPUI compiler in this repository. Use for Figma extraction, Raw Model and
  Design IR inspection, GPUI code generation, plugin and loopback-server
  workflows, fixture verification, Linux Wayland capture, fidelity analysis, or
  any defect/improvement found while using figma-rust.
metadata:
  oc-skill-power: managed
  oc-skill-power-category: development
---
# figma-rust Operator Workflow

Use this skill for every end-to-end figma-rust task. The system is a structural design compiler, not screenshot-to-code: Figma data becomes a versioned Raw Model, target-neutral Design IR, deterministic GPUI Rust, and verification evidence. Never infer business logic from a design.

## Mandatory boundaries

- Keep generated view code separate from handwritten application state, navigation, network access, validation, and domain behavior.
- Preserve the source Figma node ID in diagnostics, generated source maps, fixtures, and issue evidence.
- Never hide unsupported source behavior. Emit or retain node/property-scoped diagnostics.
- Do not claim visual fidelity from `cargo check` alone. Require applicable structure, geometry, and pixel evidence.
- Use the pinned Rust toolchain and GPUI revision from the repository. Do not upgrade them implicitly.
- Keep full extraction, Cargo, rendering, and image comparison outside Figma Codegen's short callback budget.
- Do not publish proprietary Figma content, private file URLs, credentials, tokens, or unsanitized screenshots in GitHub issues.

## Required start

1. Read `references/usage.md` for the requested operating mode and exact commands.
2. Read `references/capability-and-diagnostics.md` before deciding that a diagnostic is a defect.
3. Load `figma-design-to-code` before Figma `get_design_context` calls and `figma-use` before any `use_figma` call.
4. Load `opencode-rust-coder` before substantial Rust implementation or debugging.
5. For Linux transparent-window evidence, load `transparent-window-rgba-capture` and `atomic-artifact-publication`, then use the project command `/capture-linux`.
6. Inspect the owning source and existing fixture before changing behavior. Prefer the smallest correct change.
7. Select one mode: extract, inspect/lint, compile, plugin bridge, verify, capture, or compiler development. Do not mix evidence from different modes.
8. For pixel verification, read `references/fidelity-fonts.md`; use exact source
   font files and configure grayscale fidelity rendering before opening the window.

## Core operating sequence

1. **Extract**: select the smallest useful Figma root and export one versioned JSON bundle. Roots on different pages require separate bundles because `source.page_id` is singular.
2. **Inspect**: run `figma-rust inspect` and review the raw preorder tree plus normalized diagnostic summary.
3. **Lint**: run `figma-rust lint`; use `--strict` only when warnings are intended to fail the gate. Exit 1 means domain diagnostics, not a CLI crash.
4. **Compile**: run `figma-rust compile` only after required errors are understood. Treat `generated.rs`, `source-map.json`, `ir.json`, and `diagnostics.json` as one compiler-owned artifact set.
5. **Integrate**: call generated view functions from handwritten application code. Never edit generated files to add business behavior.
6. **Verify**: compare hierarchy/bounds and optional images with an explicit manifest. Read the machine report; exit 1 is a completed comparison that failed thresholds, while exit 2 is a verification/configuration error.
7. **Capture**: on Linux, use a real compositor-backed fixture and preserve readiness, exact window identity, dimensions, scale, source captures, hashes, reconstruction/decoding details, and verifier output.
8. **Close**: run the applicable plugin and Rust gates, inspect all changed artifacts, then apply the mandatory issue decision below.

## Evidence rules

- A successful `figma-rust compile` proves normalization, code generation, and artifact publication only. Pinned-GPUI API compatibility requires a separate generated-fixture Cargo check; neither gate proves geometry or pixels.
- A hand-authored GPUI proof must be labeled hand-authored and must not be represented as compiler output.
- A zero-tolerance comparison with `passed:false` is valid diagnostic evidence, but it is not a passing fidelity fixture.
- Linux capture evidence must identify the ready process and exact compositor window. If a video source is lossy, state that clearly and retain the source plus decoder evidence.
- Treat font file hashes and GPUI text rendering mode as capture provenance. Do
  not install fixture-specific font versions globally.
- Publish generated artifacts with a directory lock, unique same-directory temporary file, sync, and atomic rename. Preserve the previous final on validation failure.

## Mandatory GitHub issue decision

**Standing decision:** before declaring a figma-rust task complete, create one GitHub issue for each independently reproducible root cause or coherent actionable improvement discovered during system use. If a duplicate exists, link the existing issue instead of creating another. This project-level rule is standing authorization for public, non-security issues in `ugur-murat-alt/figma-rust`; do not ask for another confirmation.

Create an issue when:

- documented input produces wrong Raw Model, IR, GPUI code, geometry, pixels, exit status, artifact ownership, or provenance;
- behavior is nondeterministic, silently lossy, unsafe, stale, or contradicts repository documentation;
- a repeated operator pain point has a bounded proposed improvement and observable acceptance criteria;
- a documented unsupported capability now has a concrete fixture and implementation path worth tracking.

Do not create an issue when:

- the result is an already documented limitation and no new evidence or proposal exists;
- the failure is only a local setup mistake that is corrected without a product change;
- evidence is speculative, unreproducible, duplicate, or contains private material;
- the report concerns a vulnerability, secret exposure, or exploitable security weakness. Never publish security-sensitive details; stop and request a private reporting path.

When the decision is positive, follow `references/issue-policy.md` in order. Search duplicates first, satisfy every evidence requirement, create the issue with the correct template/label, read it back, and include its URL in the task report. If issue creation fails, provide the ready-to-submit body and report the exact blocker; do not claim the issue was created.

## Completion gates

Run the gates that match the changed area:

```sh
"${CARGO:-cargo}" fmt --all -- --check
"${CARGO:-cargo}" clippy --all-targets -- -D warnings
"${CARGO:-cargo}" test
npm --prefix plugin test
npm --prefix plugin run check
npm --prefix plugin run build
```

For changes behind `capture-real-window`, also run feature-enabled fixture tests, Clippy, and the relevant binary build. For behavior or artifact changes, run the exact fixture verifier and provenance/hash checks described in `references/usage.md`.

Report only checks that actually ran. Distinguish passed checks, expected threshold failures, skipped checks, and blockers.

## Reference map

- `references/usage.md`: installation, plugin setup, CLI commands, server bridge, verification, capture, and development loops.
- `references/capability-and-diagnostics.md`: architecture boundaries, diagnostic interpretation, current limitations, and evidence meanings.
- `references/issue-policy.md`: mandatory issue classification, evidence, duplicate search, creation, and verification steps.
- `references/fidelity-fonts.md`: exact font isolation and grayscale rendering for deterministic pixel proof.
- `templates/bug-report.md`: sanitized bug body template for CLI-based issue creation.
- `templates/feature-request.md`: actionable improvement body template for CLI-based issue creation.
