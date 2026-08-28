# GPUI revision qualification

The repository pin remains unchanged until a candidate produces a complete
machine-readable qualification report. Run the current pin twice:

```sh
scripts/qualify-gpui-revision.sh \
  5631830c564afa89b3aba679f45d9c3345f9460f /tmp/gpui-pass-1.json
scripts/qualify-gpui-revision.sh \
  5631830c564afa89b3aba679f45d9c3345f9460f /tmp/gpui-pass-2.json
cmp /tmp/gpui-pass-1.json /tmp/gpui-pass-2.json
```

The report records API compilation, runtime-helper tests, canonical geometry and
pixel verification, the platform capture capability, every source file bound to
the old revision, and the migrations required for a different candidate. The
source is an immutable archive of the reported commit; uncommitted work is not
qualified accidentally.

The classifier itself has a deliberate negative probe:

```sh
scripts/qualify-gpui-revision.sh \
  5631830c564afa89b3aba679f45d9c3345f9460f \
  /tmp/gpui-api-failure.json expect-api-failure
```

That probe injects a nonexistent GPUI API only inside the temporary source copy.
Success means the report returns `GPUI_API_COMPILE_FAILED`; the repository is
never modified. `.github/workflows/gpui-qualification.yml` runs both positive
passes and the negative classifier and retains reports plus stage logs.

Capture capability is intentionally separate from compile success. The current
pin reports a native headless renderer only on macOS, compositor-required capture
on Linux, and no native headless renderer on Windows. Platform workflows must add
their own actual capture/input/font evidence before a candidate pin is accepted.
