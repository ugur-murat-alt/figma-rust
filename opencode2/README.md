# @vaur94/figma-rust

OpenCode V2 plugin that registers the public `figma-rust` operator skill and its
three task-specific skills:

- `figma-rust-extract-compile`
- `figma-rust-semantic-gpui`
- `figma-rust-visual-verification`

Add the exact package version to the global or project `plugins` list:

```jsonc
{
  "plugins": ["@vaur94/figma-rust@0.4.0"]
}
```

Project-local skills with the same ID take precedence. The package fills only
missing skill IDs, so a figma-rust checkout can keep using its repository-owned
instructions and references.

The compiler CLI and Figma development plugin are not included in this npm
package. Install the CLI from the repository and build `plugin/manifest.json`
separately as documented in the root README.
