# Figma -> GPUI Capability Matrix

Classification is intentionally conservative:

- **GPUI native**: directly expressible with the pinned GPUI core.
- **figma-rust runtime**: implementable as a small GPUI primitive or deterministic
  lowering owned by this project.
- **asset/SVG**: source semantics can be preserved better as a vector asset.
- **raster fallback**: source can be preserved only by an explicit bitmap export.
- **unsupported/ambiguous**: no honest deterministic lowering is currently known.

Rows can name more than one route in preference order. The compiler emits a
diagnostic whenever it leaves the first route.

`figma-rust runtime` is a capability classification, not permission to emit an
unimplemented route. Until a row has a verified runtime primitive/lowering, a
planned `Runtime` asset decision fails normalization with node/property-scoped
`FR-ASSET-001`; operators must use a semantically valid source SVG/raster
fallback or add the missing runtime proof.

| Figma feature | Classification | Current lowering and limits |
| --- | --- | --- |
| Node hierarchy and source ID | GPUI native | Stable generated functions, element IDs, debug selectors, and sidecar source map. |
| Large selection traversal | figma-rust runtime | Deterministic preorder chunks contain at most 2,000 nodes; the schema-v2 extraction manifest records chunk/root counts and fails completeness closed on omitted subtrees. |
| Compiler bundle transfer | figma-rust runtime | Browser download remains available; an opt-in fixed-path loopback export atomically persists validated schema-v2 bytes and reports exact size, SHA-256, transfer completion, and traversal completion. |
| Compiler artifact generation publication | figma-rust CLI | Sorted artifact names, lengths, and bytes define a deterministic generation ID. Immutable generation directories are validated by size/SHA-256 and selected by one atomic `current-generation.json` pointer; a synced pending journal repairs interrupted flat projections on restart with bounded compiler-owned cleanup. |
| Compiler asset cache | figma-rust CLI | Decoded bytes, media type, and sorted export settings define a deterministic cache key. Exact payloads share one validated blob across immutable generations; current/pending reachability drives bounded garbage collection, while flat compatibility copies cannot corrupt linked cache content. |
| Multi-root compile outcome | figma-rust CLI | Strict all-or-nothing remains the default. Explicit `--root-scoped` emits one isolated generation for each successful selected root and a deterministic `root-status.json` containing every root's fingerprints, full diagnostics, status, path, and generation ID; failed normalization, unsupported Runtime routes, and codegen failures remain distinct. |
| Schema compatibility and extraction fingerprint | figma-rust runtime | Schema v2 permits additive optional/defaulted fields; v1 and unknown versions are diagnostic-only. `inspect` reports a privacy-safe fingerprint-v1 SHA-256 over canonical typed semantics and sorted capability flags. |
| Diagnostic reporting | figma-rust CLI/server | Inspect/lint JSON and loopback responses retain the complete lossless list plus deterministic blocker-first groups by severity, code, property, and selected root. Human output defaults to `--diagnostics full`; grouped mode bounds unique samples to three while preserving total counts. |
| Usage-led capability profile | figma-rust CLI/core | Explicit `orbitline-minimal-v1` profile runs produce a sorted used/unused inventory bound to extraction fingerprint v1. Required capability gaps, quarantined REST/Code Connect/grid/media/effect/action routes, undeclared flags, and bundle-level extension fields fail before opt-in profiled compilation with `FR-PROFILE-*`; default strict compilation remains compatible. |
| Frame/container | GPUI native | `div()` and Taffy layout. |
| Horizontal Auto Layout | GPUI native | Flex row; parent-aware sizing/alignment pass. |
| Vertical Auto Layout | GPUI native | Flex column; parent-aware sizing/alignment pass. |
| Child counter-axis alignment | GPUI native | `INHERIT`, deprecated `MIN`/`CENTER`/`MAX`, and `STRETCH` are preserved per child; overrides require an auto-positioned stack child, and stretch suppresses the fixed cross-axis dimension before GPUI `self_stretch`. |
| Auto Layout wrap | GPUI native | Flex wrap; verify counter-axis spacing. |
| Auto Layout SPACE_BETWEEN | GPUI native | `justify_between`; geometry regression required. |
| Auto Layout baseline alignment | figma-rust runtime | Text baseline metadata is needed; do not lower to center. |
| HUG sizing | GPUI native | Content/intrinsic sizing; text and transformed children need fixture proof. |
| FILL sizing | GPUI native | Flex grow/stretch only when the parent/axis permits it. Ambiguous uses are errors. |
| FIXED sizing | GPUI native | Explicit pixel width/height. |
| Min/max sizing | GPUI native | Taffy min/max dimensions. |
| Aspect-ratio lock | GPUI native | Ratio-constrained sizing where parent semantics are compatible. |
| Uniform grid tracks | GPUI native | GPUI repeated grid rows/columns. |
| Grid child row/column span | GPUI native | Native row/column span. |
| Mixed FIXED/FLEX/HUG grid tracks | figma-rust runtime | Deterministic mixed-track grid primitive or nested-layout lowering. |
| Grid manual placement | figma-rust runtime | Stable explicit placement; native convenience API is incomplete. |
| Grid row auto-flow | figma-rust runtime | Resolve cells before target lowering. |
| Constraints in non-Auto Layout frame | figma-rust runtime | Parent-size-aware anchors/stretch/scale pass. |
| Absolute Auto Layout child | GPUI native | Absolute element with parent-relative insets/size. |
| Absolute frame layout | GPUI native | Relative container plus absolute children. |
| Scroll container | GPUI native | Axis-specific overflow/scroll; scrollbar visuals remain platform-sensitive. |
| Rectangular clipping | GPUI native | Overflow hidden/content mask. |
| Rounded clipping | GPUI native | Radius plus clipping; verify nested shadows. |
| Arbitrary vector mask | asset/SVG, raster fallback | Preserve mask in SVG when possible; raster only when SVG cannot preserve effects. |
| Rotation of SVG/path | GPUI native | SVG/path transform APIs. |
| General subtree transform | figma-rust runtime | Positive uniform scale plus affine translation is deterministically folded into concrete generated layout/text/paint geometry, including center-origin matrices; clipping and hit testing therefore use transformed bounds. Rotation, skew, non-uniform/non-positive scale, and scaled variable-bound numeric geometry remain explicit codegen errors. |
| Skew/perspective | asset/SVG, raster fallback | Preserve vector transform in SVG; perspective has no Figma 2D equivalent in normal nodes. |
| Layer opacity | GPUI native | Native element opacity. |
| PASS_THROUGH blend | GPUI native, figma-rust runtime | An opacity-1 `GROUP` is structural and lowers natively; other node kinds or group opacity require explicit compositing. |
| Non-normal blend modes | raster fallback | No general GPUI Styled equivalent; never silently drop. |
| Solid fill | GPUI native | Native background color. |
| Two-stop linear gradient | GPUI native | Native angle and two color stops. |
| Multi-stop linear gradient | figma-rust runtime, asset/SVG | Runtime custom paint if validated; SVG otherwise. |
| Radial/angular/diamond gradient | figma-rust runtime, asset/SVG | Custom paint only after visual fixtures; SVG is safe fallback. |
| Image fill FIT | GPUI native | Compiler-owned image assets lower through GPUI `ObjectFit::Contain`. |
| Image fill FILL/CROP | GPUI native, figma-rust runtime | FILL lowers through `ObjectFit::Cover`; fixed-size, axis-aligned CROP matrices lower to an explicitly sized/offset image inside rounded clipping. Rotation, filters, TILE, dynamic crop bounds, and non-axis-aligned/out-of-range transforms remain fail-closed runtime routes. |
| Tiled image fill | figma-rust runtime | Repeated image primitive. |
| Video paint | unsupported/ambiguous | UI placeholder plus typed media contract; no behavior invented. |
| Pattern paint | asset/SVG, raster fallback | SVG if source pattern is exportable. |
| Shader paint | raster fallback | Controlled export with warning. |
| Uniform inside-like border | GPUI native | Native border is acceptable only when geometry matches. |
| Independent edge stroke widths | GPUI native | Per-edge border widths; one shared paint/style. |
| CENTER/OUTSIDE stroke alignment | figma-rust runtime, asset/SVG | Custom paint/path or SVG; GPUI border is not semantically equivalent. |
| Multi-paint stroke | asset/SVG | Preserve vector output. |
| Dashed stroke | GPUI native, asset/SVG | Native rectangular border; arbitrary paths use SVG. |
| Stroke caps/joins | asset/SVG | Preserve path semantics in SVG. |
| Variable-width/brush/dynamic stroke | asset/SVG, raster fallback | SVG first; raster if export cannot preserve the effect. |
| Uniform corner radius | GPUI native | Native radius; bound values use the context-aware number resolver. |
| Independent corner radii | GPUI native | Native four-corner radii; each corner retains its own token and fallback. |
| Corner smoothing/squircle | figma-rust runtime, asset/SVG | Custom path or SVG; circular radius is not equivalent. |
| Drop shadow | GPUI native | Offset, blur, spread, color; multiple shadows supported. |
| Inner shadow | GPUI native | Native inset shadow at pinned revision; fixture required. |
| Layer/background blur | raster fallback | No proven general GPUI equivalent. |
| Progressive blur | raster fallback | Explicit diagnostic. |
| Noise/texture effect | raster fallback | Explicit diagnostic. |
| Glass effect | raster fallback | Explicit diagnostic. |
| Shader effect | raster fallback | Explicit diagnostic. |
| Plain text | GPUI native | Platform text shaping and wrapping. |
| Multiline text | GPUI native | Fixed/hug dimensions plus typed auto-resize, alignment, truncation, and max-line metadata are normalized together; GPUI lowering still requires a property-specific proof before claiming non-default alignment or truncation. |
| Mixed font/size/weight/color runs | figma-rust runtime | Build explicit GPUI text runs; never collapse to one style. |
| Line height | GPUI native | Unit conversion and font fixture required. |
| Letter spacing | asset/SVG | Pinned GPUI exposes no proven styled letter-spacing API; nonzero spacing exports the text node as SVG instead of dropping the value. |
| Paragraph/list indentation and spacing | figma-rust runtime | Retained as source metadata with property-scoped warnings; explicit paragraph layout helper remains pending. |
| OpenType feature overrides | GPUI native, figma-rust runtime | Pass supported features; diagnose unavailable ones. |
| Leading trim/text wrap styles | figma-rust runtime | Retained as source metadata with property-scoped warnings; text helper plus geometry fixture remains pending. |
| Missing font | unsupported/ambiguous | Error in fidelity mode; configured substitution only with diagnostic. |
| Variable-bound color and modeled numbers | figma-rust runtime | Colors, width/height/min/max dimensions, text size, Auto Layout gap/padding, corner radii, and per-edge stroke widths retain `TokenRef`, full mode context, and literal fallback. Other numeric paths remain diagnostic until modeled. |
| Variable-bound string/boolean | figma-rust runtime | Component TEXT `characters` and BOOLEAN `visible` references lower through context-aware resolvers with literal fallback. Other string/boolean consumers remain unsupported and node/property-scoped. |
| Variable modes/collections | figma-rust runtime | Extraction schema v2 preserves each `(variable, mode context)` value for standard collections and cross-collection aliases; generated resolvers receive the applicable collection/mode map. Enterprise extension lineage is recognized from `isExtension`, `parentVariableCollectionId`, and `rootVariableCollectionId`, then rejected before alias/value resolution with node/property-scoped `FR-TOKEN-MODE-005`; override lowering remains unclaimed without a real Enterprise fixture. |
| Component set and variants | figma-rust runtime | Semantic registry resolves Figma key and variant properties. |
| TEXT/BOOLEAN component property | figma-rust runtime | Typed property mapping. |
| INSTANCE_SWAP property | figma-rust runtime | Registry lookup with structural fallback and warning. |
| Instance direct overrides | figma-rust runtime | Preserve fields and apply after component defaults. |
| Unmapped component instance | GPUI native | Structural subtree fallback plus `WARNING`; never disappear. |
| Click/hover/press/key reaction | GPUI native | Emit typed action metadata/contract only. |
| Navigation/open-overlay reaction | figma-rust runtime | Typed action payload; application owns navigation. |
| Smart animate | unsupported/ambiguous | Metadata retained; no invented transition implementation. |
| Rectangle/ellipse/simple primitive | GPUI native | Native box/path where geometry is exact. |
| Vector/boolean operation | asset/SVG | Figma SVG export is preferred. |
| Complex vector plus unsupported effect | raster fallback | Controlled fallback with node ID and reason. |
| Source geometry capture | GPUI native | Test-support `debug_selector` and final bounds. |
| Node-scoped verification attribution | figma-rust CLI | Global metrics remain stable; common source-node bounds attribute changed pixels to the deepest safe node at 1:1 scale, while sibling geometry derives deterministic spacing and cross-axis alignment deltas. Ambiguous axes, unknown scale, unmapped pixels, and the bounded work limit remain explicit `FR-VERIFY-*` diagnostics. |
| Headless pixel capture on macOS | GPUI native | `HeadlessAppContext` with the pinned platform Metal renderer; its `TestWindow` currently rasterizes at device scale 2. `.github/workflows/macos-headless-capture.yml` runs the no-text canonical generated fixture twice, compares the raw 200x120 device image against an integer nearest-scaled reference at zero tolerance, and retains hash-bound image/verifier/platform reports. Scale 1 is not claimed by this pinned test platform. |
| Headless pixel capture on Linux | figma-rust runtime | Pinned upstream returns no headless renderer. GNOME Wayland uses the repository-owned `gnome-screenshot` dual-backdrop adapter; other desktops retain the fail-closed Computer Use portal fallback. Both routes use the same validated ingest, reconstruction, verifier, and provenance contracts. |
| Windows render, input, font, and DPI evidence | figma-rust runtime | A real GPUI text/input probe runs on Windows at verified 100%, 125%, and 150% display profiles. Each profile requires two byte-identical client captures, observed GPUI click input, `GetDpiForWindow`, physical dimensions, Segoe UI file hash, GPU identity, and a zero-tolerance verifier report. |

## Completion rule

A row moves from runtime/asset/raster to **GPUI native** only after a pinned-source
API check, a minimal fixture, generated-code compilation, geometry verification,
and visual comparison. Documentation or a plausible API shape alone is not proof.
