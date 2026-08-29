# GPUI Design MCP and Studio Architecture

Status: **accepted foundation / incremental migration**  
Target product: **GPUI Design** (working name)  
Compatibility adapter: **figma-rust**

## 1. Decision

The repository will evolve from a Figma-authoritative extraction compiler into a
GPUI-native design system and authoring engine exposed through MCP. Figma remains
an optional import/evidence adapter; it is no longer the long-term source of
truth for OrbitLine design work.

The migration preserves the proven parts of the current system:

- deterministic normalization and GPUI code generation;
- pinned GPUI revision and compile fixtures;
- token/component/asset diagnostics;
- generated-presentation ownership boundaries;
- geometry, render, pixel, and provenance verification;
- OrbitLine capability fixtures.

The migration does **not** rewrite those layers from zero. It introduces an
editable authoring source above the existing compiler IR and gradually routes
that source through the same code generation and verification gates.

## 2. Why Figma is no longer the authority

Figma was useful for establishing the initial visual language, but it creates a
second product model that does not understand GPUI types, Rust symbols, runtime
layout measurements, application ownership, or OrbitLine's shell/module
contracts. That creates repeated translation work and weakens agent feedback:

- visual changes are expressed in Figma concepts before being translated to
  GPUI concepts;
- source node identity is separate from Rust symbol identity;
- code changes cannot update the authoritative design graph safely;
- actual GPUI text/layout/render behavior is discovered after design work;
- a web or Figma preview can look correct while the real GPUI frame differs;
- large Figma documents increase extraction and agent context cost;
- component, module, workspace, dialog, and shell rules are not one coherent
  executable contract.

The new authority must know design semantics, code ownership, and actual GPUI
render evidence at the same time.

## 3. Core invariants

1. **Authoring graph is editable; compiler IR is not.**
   `AuthoringDocument` is the source of truth. Existing `DesignDocument` remains
   a normalized/lowered compiler representation.
2. **Actual GPUI render is the visual authority.**
   A browser canvas, imported Figma frame, or generated screenshot is never
   sufficient proof by itself.
3. **Every mutation is a command transaction.**
   No client directly patches arbitrary stored JSON. Transactions carry a
   stable ID, expected revision, and ordered commands; validation happens before
   commit.
4. **Generated presentation is replaceable.**
   Domain state, navigation, validation, networking, persistence, trading logic,
   and execution behavior remain handwritten.
5. **Handwritten behavior is reference-only.**
   A design agent may inspect its symbol contract, but design-to-code operations
   may not rewrite it.
6. **Stable identity crosses all projections.**
   Authoring IDs, code bindings, source maps, runtime hit-test IDs, diagnostics,
   snapshots, and verification evidence refer to the same logical entities.
7. **Unsupported capability remains visible.**
   Lowering and rendering gaps produce scoped diagnostics rather than silent
   approximation.
8. **Figma import never overwrites authority silently.**
   Import creates a proposed graph or transaction that must pass normal conflict
   and validation rules.

## 4. Target topology

```text
OpenCode2 / coding agents             GPUI Design Studio (web first)
              |                                  |
              +---------------+------------------+
                              |
                       MCP / local API
                              |
                     gpui-design service
          +-------------------+--------------------+
          |                   |                    |
  authoring workspace   code/symbol index    preview coordinator
          |                   |                    |
  revisioned commands   Rust AST bindings     real GPUI process
          |                   |                    |
          +--------- lowering / source map --------+
                              |
                    existing compiler IR
                              |
                 deterministic GPUI codegen
                              |
               compile + geometry + pixels

Optional inputs:
  Figma plugin/REST -> import adapter -> proposed authoring transaction
  existing GPUI code -> code index -> binding/update proposal
```

## 5. Layer ownership

### 5.1 `gpui-design-core`

Owns the Figma-independent authoring contract:

- document/tree identity and child order;
- GPUI-oriented layout and positioning rules;
- visual style and literal fallbacks;
- token scopes and mode values;
- component roles, variants, slots, states, and typed event contracts;
- Rust symbol bindings and code ownership;
- deterministic validation and content fingerprints;
- revisioned commands and atomic application;
- idempotent transaction replay in the workspace.

It must not depend on GPUI, Figma SDK types, a browser framework, or an MCP SDK.

### 5.2 Existing `figma-rust-core`

Remains the compatibility Raw Model and normalized compiler IR while migration is
in progress. Its `Raw*` types are valid inside the Figma adapter/normalization
path, but must not leak into the new authoring contract.

### 5.3 Authoring-to-IR lowering

A dedicated lowering layer will translate one validated authoring revision into
existing target-neutral compiler IR. It will:

- preserve authoring IDs as source IDs;
- resolve token mode and literal fallback;
- map authoring layout to proven IR capabilities;
- select semantic component call, native GPUI primitive, runtime route, SVG, or
  raster route explicitly;
- emit node/property diagnostics for unsupported authoring features;
- publish a deterministic lowering manifest and source map.

The current compiler remains available for Figma extraction during migration.

### 5.4 `gpui-design-mcp`

The first server is stdio-only and process-local. It exposes:

- tools for document creation, validation, opening, reading, closing, command
  transactions, and lowering readiness;
- resources for workspace manifest, canonical documents, validation reports,
  and lowering manifests;
- prompts for component, shell, and tokenization workflows;
- modern `2026-07-28` discovery and legacy initialization compatibility.

The initial MCP intentionally does not apply Rust source edits, launch arbitrary
commands, or expose a network listener.

### 5.5 Durable project store

The next store layer will use an explicit project directory rather than hidden
process state:

```text
.gpui-design/
  project.json
  documents/<document-id>.json
  transactions/<document-id>/<revision>.json
  snapshots/<content-hash>/...
  bindings/rust-symbols.json
  renders/<view>/<state>/manifest.json
  diagnostics/latest.json
```

Publication follows the repository's existing lock, staged validation, sync, and
same-directory atomic rename rules. The canonical document is reconstructed or
verified from the command log and content hashes.

### 5.6 Code and symbol index

A code index connects authoring entities to actual Rust symbols. The binding
contract identifies crate, module path, symbol, optional file path, ownership,
and sync policy.

Planned write flow:

1. index Cargo workspace and GPUI components;
2. resolve a binding to a stable symbol, not a guessed text span;
3. produce a typed patch plan and source-map impact report;
4. reject edits outside the configured project root;
5. compile/check the narrow target;
6. publish the patch only after the requested confirmation/policy gate;
7. rescan and bind the resulting source hash.

Text replacement is not an acceptable general round-trip mechanism. `syn` or a
verified semantic editing layer owns Rust changes; rust-analyzer evidence is used
for symbol resolution and impact analysis.

### 5.7 Preview coordinator

The preview service launches a bounded application-owned GPUI fixture/process
for a selected document root, variant, theme, density, viewport, and interaction
state. It returns:

- exact process/window identity;
- frame readiness and scale;
- measured node bounds and hit-test IDs;
- PNG/RGBA capture with font/render provenance;
- diagnostics and source-map revision;
- optional interaction trace.

The browser studio may draw selection outlines, handles, guides, and inspectors,
but the underlying visual frame comes from GPUI or is continuously compared with
it. HTML/CSS is never treated as a fidelity implementation of the product UI.

## 6. Token architecture

Everything is not forced into one flat token list. Tokens have explicit scope:

1. **Primitive** — raw palette, spacing steps, radii, font families/weights,
   durations, easing curves.
2. **Semantic** — surface, text, border, focus, positive/negative/warning,
   selected/disabled, density intent.
3. **Component** — Button/Input/DataTable/Dialog-specific dimensions and visual
   decisions.
4. **Module** — chart, watchlist, order entry, scanner, AI panel, and module
   chrome contracts.
5. **Shell** — workspace, navigation, right-widget panel, window chrome, docking,
   overlays, and global density.
6. **Platform** — DPI, OS window behavior, input modality, native title-bar and
   compositor differences.

Every bound value retains a literal fallback. Aliases are validated for missing
targets and cycles. A mode name is not assumed to be globally meaningful without
the token identity and scope.

## 7. Component-to-shell contracts

A reusable component is more than a visual frame. Its authoring contract records:

- closed variant axes and defaults;
- slots and accepted component roles;
- visual states;
- typed presentation events;
- token bindings;
- code binding and ownership;
- metadata needed by agents and verification.

Roles establish composition levels:

```text
Primitive -> Control -> Composite -> Module -> Shell
                                  \-> Overlay
```

OrbitLine examples:

- primitive: surface, text, icon, separator;
- control: button, password input, select, tab, menu item;
- composite: DataTable, dialog body, chart toolbar;
- module: Watchlist, Chart, Quick Order, Scanner;
- shell: Login, Workspace, Main Shell, module docking and minimized queue;
- overlay: context menu, tooltip, message box, command palette.

Visual events express intent only. A `submit`, `place_order`, `navigate`, or
network side effect is not inferred from appearance and is handled by the
handwritten owner.

## 8. Bidirectional design and code

“Round trip” does not mean regenerating the whole Rust application from a
canvas. It means several controlled projections of the same identities:

### Design to code

- validate authoring revision;
- lower to compiler IR;
- generate or update compiler-owned presentation;
- build semantic patch plans for explicitly bound handwritten presentation;
- compile and render;
- bind output source hashes and verification evidence.

### Code to design

- rescan bound Rust symbols;
- extract supported props, variants, slots, actions, token references, and
  composition metadata;
- compare with the binding's last source hash;
- produce an authoring transaction proposal;
- require conflict resolution when both code and design changed.

### Conflict model

A three-way comparison uses:

1. last accepted binding snapshot;
2. current authoring revision;
3. current code/runtime projection.

Conflicts are entity/property scoped. The system never resolves a conflict by
silently choosing the visually newer or textually newer side.

## 9. Studio architecture

The first product UI should be a web studio for development velocity, backed by
the Rust engine. A later GPUI-native desktop studio can use the same contracts.

Recommended panels:

- project/document explorer;
- layer and semantic component tree;
- actual GPUI preview canvas;
- layout/size/position inspector;
- fills/strokes/radii/effects inspector;
- text and typography inspector;
- tokens and aliases with scope/mode views;
- component variants, slots, states, and events;
- Rust symbol/code ownership inspector;
- diagnostics and unsupported-capability panel;
- transaction history, snapshots, and diff;
- render states and visual verification;
- agent activity/proposed changes review.

The studio talks to a local authenticated service. It does not receive arbitrary
filesystem or shell access. Browser-origin writes use capability tokens and
strict origin/host allowlists; a future Streamable HTTP MCP endpoint follows the
same requirements.

## 10. MCP surface roadmap

### Foundation tools (implemented in the first slice)

- `gpui_design_capabilities`
- `gpui_design_create_document`
- `gpui_design_validate_document`
- `gpui_design_open_document`
- `gpui_design_list_documents`
- `gpui_design_get_document`
- `gpui_design_close_document`
- `gpui_design_apply_transaction`
- `gpui_design_lowering_manifest`

### Compiler and preview tools

- `gpui_design_lower_document`
- `gpui_design_compile_view`
- `gpui_design_render_view`
- `gpui_design_hit_test`
- `gpui_design_measure_nodes`
- `gpui_design_verify_view`
- `gpui_design_capture_state`

### Code tools

- `gpui_design_index_workspace`
- `gpui_design_bind_symbol`
- `gpui_design_inspect_symbol`
- `gpui_design_plan_code_patch`
- `gpui_design_apply_code_patch`
- `gpui_design_rescan_binding`
- `gpui_design_reconcile`

Mutating code tools are not added until project-root confinement, symbol
resolution, patch review, narrow compilation, rollback, and audit evidence exist.

## 11. Security and safety

- stdio is the default OpenCode2 transport;
- stdout contains MCP messages only; diagnostics go to stderr;
- messages and returned artifacts are bounded;
- no arbitrary command or path tool is exposed;
- workspace roots will be explicit and canonicalized;
- symlink escape is rejected;
- read-only and mutating tools are annotated distinctly;
- expected revision prevents lost updates;
- transaction IDs make retries idempotent and detect conflicting reuse;
- behavior bindings are reference-only;
- private OrbitLine/Figma assets are not embedded in public fixtures;
- future HTTP endpoints bind to loopback by default, validate Origin/Host, and
  require authentication/capability tokens.

## 12. Migration plan

### Phase 0 — preserve evidence

- keep current Figma extraction, compiler, fixtures, and verification passing;
- freeze schema-v2 compatibility and pinned GPUI revision;
- introduce GPUI Design names without deleting old commands.

### Phase 1 — native authoring and MCP foundation

- add `gpui-design-core`;
- add revisioned commands and validation;
- add process-local stdio MCP tools/resources/prompts;
- add the OpenCode2 `gpui-design` skill;
- document ownership and migration boundaries.

### Phase 2 — lower authoring to existing compiler IR

- implement authoring adapter and scoped diagnostics;
- generate deterministic artifacts from native documents;
- add synthetic component/token/layout fixtures;
- prove byte determinism and pinned-GPUI compilation.

### Phase 3 — real GPUI preview loop

- preview process lifecycle;
- live reload by document revision;
- node bounds/hit testing and selection overlay;
- state/theme/density/viewport matrix;
- geometry and pixel verification without Figma authority.

### Phase 4 — web studio

- hierarchy, canvas, inspectors, tokens, component contracts, diagnostics;
- command-based undo/redo and snapshots;
- agent proposal review;
- local authenticated API and project persistence.

### Phase 5 — code index and controlled round trip

- Rust symbol index and source hashes;
- semantic binding and impact analysis;
- AST-safe presentation patch plans;
- conflict-aware code-to-design proposals;
- compile/render verification and rollback.

### Phase 6 — OrbitLineV2 production migration

Migrate one vertical slice at a time:

1. Foundation tokens and primitive surfaces/text/icons;
2. Button, Input/Password Input, Select, Tabs, Menu and overlay controls;
3. Module chrome and docking contract;
4. Login shell;
5. Workspace/Main Shell;
6. Watchlist and DataTable;
7. Chart shell/tooling and render-intensive controls;
8. dialogs, right-widget panel, AI surfaces, and remaining modules.

A slice is complete only when token/component contracts, bindings, compile,
actual GPUI geometry, and required pixels pass independently.

## 13. First OrbitLine acceptance slice

The recommended first production proof is not the whole application. Use:

- dark Foundation token modes;
- one semantic surface and typography stack;
- Button variants/states;
- Password Input including the eye icon and exact inner alignment;
- Login shell composition;
- handwritten login behavior owner;
- actual GPUI render capture and node bounds;
- authoring transaction -> lowering -> code -> render traceability.

This slice exercises tokens, control contracts, slots/icons, shell composition,
code ownership, and visual verification without the complexity of charting or
workspace docking.

## 14. Non-goals

- reproducing every Figma editing feature;
- making HTML/CSS the OrbitLine renderer;
- inferring application behavior from visual design;
- regenerating handwritten domain code;
- hiding unsupported GPUI features with screenshots;
- moving all OrbitLine screens in one migration;
- making the initial process-local MCP a durable multi-user store.

## 15. Completion criteria for the platform

The architecture has reached its intended outcome when an agent can:

1. inspect the current OrbitLine tokens/components/shell and Rust bindings;
2. propose a scoped design transaction;
3. receive deterministic validation and conflict diagnostics;
4. lower the accepted revision to GPUI presentation;
5. compile against the pinned project revision;
6. launch and inspect the real GPUI frame;
7. compare geometry/pixels and iterate from evidence;
8. update an explicitly bound presentation symbol through a reviewed semantic
   patch;
9. preserve handwritten behavior and design language throughout regeneration;
10. reproduce the same artifacts from the same project revision and inputs.
