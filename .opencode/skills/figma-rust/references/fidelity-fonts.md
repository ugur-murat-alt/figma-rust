# Deterministic fidelity fonts

Font family and weight names are not enough for pixel evidence. The exact font
files and GPUI text mode affect glyph advance, wrapping, antialiasing, changed
pixels, and SSIM. Record font file hashes in capture provenance and isolate the
capture process from globally installed fonts.

## Canonical Foundation 419:2 profile

The private Foundation comparison was classified without publishing source
content. With identical generated geometry, the host's Inter 4.1 plus GPUI's
platform-default subpixel text produced:

- changed-pixel ratio `0.04661874937599361`;
- SSIM `0.9578977710273127`.

Grayscale text reduced the changed ratio but still failed. Isolated Inter 3.19,
JetBrains Mono 2.304, grayscale text, and no-wrap lowering for width-hugging text
produced a passing compositor capture:

- MAE `0.004245065681235763`;
- changed-pixel ratio `0.035064359059036765`;
- edge error `0.005327723565221191`;
- SSIM `0.9807245978098481`;
- geometry differences `0` at the declared `0.5px` absolute threshold.

This is a fixture profile, not a claim that every Figma file uses Inter 3.19.
For other files, identify and preserve the source font files instead of reusing
this profile blindly.

Official sources:

- Inter 3.19: <https://github.com/rsms/inter/releases/tag/v3.19>
- JetBrains Mono 2.304:
  <https://github.com/JetBrains/JetBrainsMono/releases/tag/v2.304>

The setup script verifies both release archives and every extracted font file by
SHA-256 before atomic publication. It never changes global Fontconfig state.

## One-time preparation

From the repository root:

```sh
export FONTCONFIG_FILE="$(
  .opencode/skills/figma-rust/scripts/prepare-fidelity-fonts.sh
)"
```
The default private destination is
`~/.local/share/figma-rust/fonts/foundations-419-2`. Override it only with
`FIGMA_RUST_FIDELITY_FONT_ROOT`.

Before opening the capture window:

```rust
gpui_platform::application().run(move |cx: &mut gpui::App| {
    figma_gpui_runtime::configure_figma_fidelity(cx);
    // Open the verification window after this call.
});
```

Include `FONTCONFIG_FILE`, the eight font hashes, GPUI revision, capture scale,
and text rendering mode in provenance. Do not install an older Inter globally;
that can silently change unrelated applications and future captures.
