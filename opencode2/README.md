# @vaur94/figma-rust

OpenCode V2 plugin that registers the GPUI-native `gpui-design` authoring/MCP
skill together with the deterministic Figma compatibility workflow:

- `gpui-design`
- `figma-rust`
- `figma-rust-extract-compile`
- `figma-rust-semantic-gpui`
- `figma-rust-visual-verification`

Use `gpui-design` for Figma-independent authoring documents, tokenized
component-to-shell contracts, revisioned design transactions, Rust code
ownership, lowering readiness, and actual-GPUI visual workflows. Use the
`figma-rust*` skills when a task still depends on Figma extraction or the
existing compiler and verification path.

Add the exact package version to the global or project `plugins` list:

```jsonc
{
  "plugins": ["@vaur94/figma-rust@0.4.0"]
}
```

Project-local skills with the same ID take precedence. The package fills only
missing skill IDs, so a checkout can keep using its repository-owned
instructions and references.

The Rust MCP server, compiler CLI, Figma development plugin, and future GPUI
Design web studio are not included in this npm package. Build/run them from the
repository as documented in the root README.
