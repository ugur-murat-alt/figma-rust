---
name: transparent-window-rgba-capture
description: Obtain deterministic RGBA pixel evidence of a transparent app
  window on Linux/Wayland when the UI framework cannot read back its own render
  texture. Covers an external display mode with first-frame readiness gating,
  compositor-portal captures at scale 1 over controlled black/white opaque
  backdrops, straight-alpha reconstruction (alpha=255-(white-black),
  RGB=black*255/alpha), channel consistency validation, and crash-safe
  publication with provenance. Use when verifying alpha/transparency rendering
  of a windowed app, when a renderer has no texture readback, or when
  reconstructing alpha from composite screenshots.
metadata:
  oc-skill-power: managed
  oc-skill-power-category: development
---
# Transparent Window RGBA Evidence Capture

Problem class: obtain deterministic, pixel-exact RGBA evidence of a transparent window when the UI framework cannot read back its own render texture (e.g., verifying a renderer's alpha output). Verified approach: capture the live window through the compositor over two controlled backdrops, reconstruct straight alpha from the pair, and keep a reproducible reconstruction path from checked-in inputs.

## Constraint

Fresh capture requires an external portal / MCP screen-capture tool — it cannot be done from inside the app. Reconstruction from the two composites is fully reproducible offline from checked-in inputs: always retain them (plus hashes/provenance) so evidence can be re-verified without re-capturing.

## Procedure

1. **Add an external display mode** that keeps the exact-size window open and marks readiness only after the first rendered frame. Never capture before readiness: an open window with an unrendered surface yields blank or black frames.
2. **Capture the targeted window twice** through the compositor portal at scale 1: once over an opaque black backdrop, once over an opaque white backdrop. Require opaque PNG inputs (no alpha channel) and exact window dimensions.
3. **Reconstruct straight alpha** per 8-bit channel:
   - `alpha = 255 - (white - black)`
   - `RGB = black * 255 / alpha`
   Composite over white is `W = a*F + (1-a)*255` and over black `K = a*F`; subtracting solves for alpha, dividing recovers the straight color F. Any other background content contaminates the math.
4. **Validate channel consistency** before treating the result as evidence (per-channel reconstruction must agree and stay in range).
5. **Publish crash-safe** with provenance: acquire a lock, write to a unique temp with sync, publish by atomic same-directory rename, and retain both composites plus hashes. Reject direct, hardlink, and symlink input/output aliases before cleanup — unlinking through an alias can destroy the wrong target.
6. **Add a regression test** that reconstruction from the checked-in composites matches the reference raw pixels, so future renderer changes are caught without a fresh capture.

## Pitfalls

- Readiness gating is mandatory: "window is open" is not "frame is rendered".
- Scale must be exactly 1 and backdrops must be opaque. A transparent backdrop makes the white and black captures identical and alpha collapses to zero.
- Inputs must be opaque PNGs of exact dimensions; alpha-carrying or resized inputs break the arithmetic.
- This math recovers straight (unpremultiplied) alpha; do not feed it premultiplied captures.
- Cleanup alias rejection must cover hardlinks and symlinks, not only the direct path.

## Cross-reference

- `atomic-artifact-publication` — full crash-safe publication checklist (crash-released lock, `create_new` unique temp, `sync_all`, same-directory atomic rename, failure semantics, symlink-safe cleanup).
