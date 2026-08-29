---
name: gpui-design
description: Operate the Figma-independent GPUI Design authoring model and MCP for tokenized components, modules, shells, revisioned design transactions, Rust symbol ownership, lowering readiness, and actual-GPUI visual workflows. Use when designing OrbitLine directly for GPUI without making Figma the authority.
metadata:
  oc-skill-power: managed
  oc-skill-power-category: development
---
# GPUI Design MCP Workflow

Use this skill for GPUI-native design work where the editable source of truth is
the GPUI Design authoring document rather than a Figma file. The system is being
introduced incrementally beside the existing figma-rust compiler. Figma remains
an optional importer and historical evidence source; it must not silently
replace an accepted GPUI Design revision.

## Architecture truth

```text
GPUI Design AuthoringDocument
  -> deterministic validation
  -> expected-revision DesignTransaction
  -> authoring-to-compiler-IR lowering (planned next layer)
  -> deterministic GPUI presentation
  -> pinned-GPUI compile
  -> actual GPUI bounds/render/pixel evidence
```

The web studio is a client of this engine. HTML/CSS is not the OrbitLine
renderer and is never fidelity proof.

Read `docs/gpui-design-mcp-architecture.md` before changing the authoring schema,
transaction protocol, code ownership, lowering boundary, MCP surface, preview
model, or studio architecture.

## Start the MCP server

From the repository root:

```sh
"${CARGO:-cargo}" run -p gpui-design-mcp
```

The server uses newline-delimited JSON-RPC over stdio. It supports modern MCP
`2026-07-28` discovery and legacy initialization clients. It writes MCP messages
to stdout only and uses no network listener.

The first implementation is process-local. An open document disappears when the
MCP process exits; durable project storage is a later phase and must use locked,
validated, atomic publication.

## Available tools

Read-only:

- `gpui_design_capabilities`
- `gpui_design_validate_document`
- `gpui_design_list_documents`
- `gpui_design_get_document`
- `gpui_design_lowering_manifest`

Mutating process-local state:

- `gpui_design_create_document`
- `gpui_design_open_document`
- `gpui_design_close_document`
- `gpui_design_apply_transaction`

Resources expose the workspace manifest, canonical documents, validation
reports, and lowering manifests under `gpui-design://...`. Prompts route
component, shell, and tokenization work.

## Non-negotiable boundaries

- The editable `AuthoringDocument` sits above the existing normalized compiler
  IR. Do not make the current Figma-derived `Raw*`/`DesignDocument` model the
  studio mutation format.
- Apply changes through `DesignTransaction`; never directly mutate persisted
  authoring JSON.
- Every transaction has a stable `transaction_id`, exact `document_id`,
  `expected_revision`, and ordered commands.
- Validation must complete before commit. A failed command or invalid candidate
  leaves the accepted document unchanged.
- Replaying the same transaction ID with identical content is idempotent.
  Reusing it with different content is an error.
- Generated presentation may be replaced. Handwritten presentation requires a
  reviewed semantic patch path. Handwritten application/domain behavior is
  always `REFERENCE_ONLY`.
- Do not infer navigation, validation, networking, persistence, trading actions,
  or execution behavior from a visual node or event name.
- Preserve stable IDs across authoring nodes, tokens, components, code bindings,
  runtime hit testing, source maps, diagnostics, and evidence.
- Preserve literal fallbacks for token-bound values.
- Do not suppress dangling token/component/code references or alias cycles.
- Actual GPUI output is the visual authority. Browser/Figma appearance alone is
  not an acceptance gate.
- Preserve Rust `1.97.1` and the pinned GPUI revision unless an explicit upgrade
  changes every related fixture and proof.

## Token scope order

Design from foundations upward:

1. `PRIMITIVE`: raw palette, spacing, radius, typography, motion;
2. `SEMANTIC`: surface, text, border, status, focus, selected, disabled;
3. `COMPONENT`: control-specific decisions;
4. `MODULE`: Watchlist, Chart, Quick Order, Scanner, AI and module chrome;
5. `SHELL`: Login, Workspace, Main Shell, docking, navigation and overlays;
6. `PLATFORM`: DPI, OS, input and native-window differences.

Avoid one-off literals in composites when an existing lower-scope token owns the
decision. Do not create aliases that form cycles or erase literal fallbacks.

## Component and shell contracts

Use the role ladder:

```text
PRIMITIVE -> CONTROL -> COMPOSITE -> MODULE -> SHELL
                                  \-> OVERLAY
```

A component contract can contain variants, slots, visual states, typed intent
events, token bindings, and a Rust code binding. Slots declare accepted roles;
variants are closed sets with explicit defaults. Events describe presentation
intent only. The handwritten owner decides side effects.

Shell work must model module chrome, workspace composition, dialogs, overlays,
docking, minimized queues, density, right-widget behavior, and platform/window
concerns explicitly rather than drawing one large frame.

## Required operating sequence

1. Call `gpui_design_capabilities` and read
   `gpui-design://workspace/manifest`.
2. Read the target document, validation report, and lowering manifest.
3. Select the smallest semantic slice: token family, primitive, control,
   composite, module, overlay, or shell section.
4. Reuse existing IDs/contracts and record every new stable ID before mutation.
5. Build one transaction against the exact current revision.
6. Validate the complete candidate structure; do not issue speculative chains of
   mutations against stale revisions.
7. Apply the transaction once. On `REVISION_CONFLICT`, reread and reconcile;
   never simply increase `expected_revision` without understanding the change.
8. Read validation and lowering resources after commit.
9. Once lowering/preview tools are available, run compile, actual GPUI geometry,
   and required pixel gates separately.
10. Keep code-binding and behavior changes in their owning workflows; never hide
    them inside generated presentation.

## Current phase limitations

The foundation MCP does not yet:

- persist documents after process exit;
- lower native authoring documents into the existing compiler IR;
- compile or render GPUI views;
- inspect live geometry or perform hit testing;
- index Rust workspaces or apply AST-safe source patches;
- host the web studio.

Treat `gpui_design_lowering_manifest` as readiness evidence, not proof that GPUI
code was generated or rendered. Do not claim code round-trip or visual fidelity
until the corresponding phase and gates exist.

## OrbitLine first vertical slice

Prefer this proof order:

1. dark Foundation tokens;
2. semantic surface and typography primitives;
3. Button variants/states;
4. Password Input, including the eye-icon slot and measured alignment;
5. Login shell composition;
6. handwritten login state/validation owner;
7. actual GPUI bounds and pixels once preview lowering is connected.

Do not begin with the full chart, workspace, or all modules at once.

## Completion gates

For changes in this foundation:

```sh
"${CARGO:-cargo}" fmt --all -- --check
"${CARGO:-cargo}" clippy --all-targets -- -D warnings
"${CARGO:-cargo}" test
npm --prefix opencode2 run build
npm --prefix opencode2 test
```

Report only checks that actually ran. A valid authoring transaction is not a
successful lowering; a lowering is not a pinned-GPUI compile; a compile is not
geometry or pixel proof.
