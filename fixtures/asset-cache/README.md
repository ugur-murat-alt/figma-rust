# Content-addressed asset cache fixture

This sanitized schema-v2 pair proves the CLI cache contract. Nodes `20:1` and
`20:2` use identical SVG bytes and export semantics under different asset IDs, so
they share one verified cache blob. Node `20:3` changes between `extraction.v1.json`
and `extraction.v2.json`; compiling v2 over v1 retains the shared blob and removes
only the now-unreachable old blob. Repeating either compile keeps the same
generation pointer, cache names, and published bytes.

The cache key covers decoded bytes, media type, and sorted export settings. Cache
blobs and immutable generations are hard-linked for exact reuse; flat compatibility
artifacts are distinct copies so editing one cannot corrupt the cache. Any cache or
generation hash/size mismatch fails closed before replacing the prior projection.
