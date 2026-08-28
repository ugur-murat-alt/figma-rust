# Node-scoped verification attribution fixture

This public synthetic fixture contains three independent regressions:

- node `1:3` has a horizontal sibling gap of `3px` instead of `2px`;
- node `1:3` is shifted `1px` on the horizontal layout's cross axis;
- one color-only pixel inside node `1:2` changes from white to red.

`verify.json` intentionally uses zero tolerance and therefore exits `1`. The JSON
report must retain global metrics while naming the spacing pair (`1:2` -> `1:3`),
the parent/child alignment (`1:1` -> `1:3`), and the smallest common node owning
the changed pixel (`1:2`). The PNGs are stored in deterministic base64 wrappers
so the text-only public fixture remains reviewable; decoded images are tiny,
lossless, and 1:1 with the `12x8` geometry viewport so attribution has no implicit
scale.
