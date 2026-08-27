---
description: Capture and verify the real GPUI fixture through the Linux compositor
agent: build
---

Run the deterministic Linux compositor capture for the fixture directory in `$1`.
If `$1` is empty, use `fixtures/real-figma`. Complete the workflow end to end; do
not stop after describing commands.

1. Load `transparent-window-rgba-capture` and `atomic-artifact-publication`.
2. Call Computer Use `get_app_state` first with `include_screenshot=false` and
   `verbose=true`. Stop without changing checked-in capture artifacts if screenshot
   capture or exact window targeting is unavailable.
3. Build `capture-real-group` with feature `capture-real-window` and build
   `figma-rust-cli`. Use Cargo from `PATH`, or the executable named by `$CARGO`.
   The checked-in display binary must call
   `figma_gpui_runtime::configure_figma_fidelity(cx)` before opening its window.
4. Ensure no stale `capture-real-group` process or window exists before starting.
5. For each backdrop in this exact order, `black` then `white`:
   - Create one private unique directory below `${TMPDIR:-/tmp}`, then create
     unique readiness and stderr files there. Launch
     `target/debug/capture-real-group --display BACKDROP` with stdout/stderr
     redirected to those files and return `$!` from the launching shell. Retain
     this process PID before any window lookup.
   - Find exactly one window whose title is `figma-rust-real-group-ready` and whose
     `wm_class` is `figma-rust-real-group`. Require its PID to equal the retained
     launch PID.
   - Read exactly one JSON readiness event. Require matching PID, backdrop,
      `100x60` logical size, expected title/class, pinned GPUI revision, and
      `text_rendering=grayscale`.
   - Require compositor bounds `100x60`.
   - Capture that exact window with Computer Use `get_app_state`: PNG, scale `1`,
     `max_width=100`, `max_height=60`, and `max_bytes=2097152`.
   - Require source `xdg-desktop-portal`, format and MIME type PNG, coordinate and
     payload dimensions `100x60`, scale `1`, and `resized=false`.
   - Pipe the returned `screenshot.data_url` unchanged to
     `target/debug/capture-real-group --ingest-data-url OUTPUT`, using
     `capture.black.png` or `capture.white.png` in the fixture directory. Never
     decode or publish it with an unchecked shell redirect.
   - In a finally-style cleanup that runs after success or failure, terminate the
     retained PID, confirm it exited, and remove its readiness/stderr files. Never
     rely on successful window discovery to learn which process must be cleaned.
6. Reconstruct with:

   `target/debug/capture-real-group --reconstruct BLACK WHITE ACTUAL`

7. Create a unique same-directory temporary report and immediately install a shell
   trap/finally cleanup that removes it on every exit path. Run
   `target/debug/figma-rust verify VERIFY_MANIFEST --json` into it, require
   `passed=true`, then atomically rename it to `capture.verify.json` and clear the
   cleanup trap only after the rename succeeds. Use `verify.image.json` as the
   manifest.
8. Finalize provenance with:

   `target/debug/capture-real-group --finalize-provenance METADATA BLACK WHITE ACTUAL VERIFY_MANIFEST VERIFY_REPORT xdg-desktop-portal`

   Use `capture.compositor.json` as METADATA. This command must validate the
   reconstruction, passing verifier report, hashes, and co-located artifact paths
   before replacing metadata.
9. Re-run the image verifier, confirm no `capture-real-group` process and no
   `.*.tmp-*` capture artifact remains, and report hashes plus pixel metrics.
   When the fixture contains text, also report the exact font-file hashes and run
   the display process with the matching isolated `FONTCONFIG_FILE`; never change
   the global Fontconfig configuration for a fixture capture.
10. Update durable project memory only if verified capture facts changed, then
    refresh the exact repository's full codebase-memory index and wait for `ready`.

Fail closed: never publish provenance or claim success after a mismatched window,
scale, source, MIME type, dimension, failed ingest, failed reconstruction, failed
verification, or incomplete process/temp cleanup.
