import { cp, mkdir, readFile, rm, writeFile } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const packageRoot = dirname(fileURLToPath(import.meta.url));
const repositoryRoot = resolve(packageRoot, "..");
const sourceRoot = resolve(repositoryRoot, ".opencode", "skills");
const outputRoot = resolve(packageRoot, "dist");

const skills = [
  {
    id: "figma-rust",
    name: "figma-rust",
    description:
      "Operate, extend, debug, and verify the deterministic Figma-to-Rust GPUI compiler in this repository. Use for Figma extraction, Raw Model and Design IR inspection, GPUI code generation, plugin and loopback-server workflows, fixture verification, Linux Wayland capture, fidelity analysis, or any defect/improvement found while using figma-rust.",
  },
  {
    id: "figma-rust-extract-compile",
    name: "figma-rust-extract-compile",
    description:
      "Extract Figma selections into schema-v2 figma-rust bundles, inspect and lint diagnostics, compile deterministic GPUI artifacts, operate the local plugin/loopback bridge, or debug failures in that pipeline. Use for extraction JSON, plugin export, inspect, lint, compile, serve, generated artifacts, source maps, or asset manifests.",
  },
  {
    id: "figma-rust-semantic-gpui",
    name: "figma-rust-semantic-gpui",
    description:
      "Turn figma-rust extraction and generated evidence into maintainable handwritten GPUI design-system code. Use for Foundation tokens, semantic components, variants/actions, application integration, TokenResolver/AssetResolver wiring, or replacing structural generated output with production GPUI components without losing visual traceability.",
  },
  {
    id: "figma-rust-visual-verification",
    name: "figma-rust-visual-verification",
    description:
      "Verify figma-rust or handwritten GPUI output against Figma using source-linked geometry, deterministic pixel comparison, exact fonts, Linux Wayland compositor capture, hashes, and provenance. Use for fidelity, screenshots, geometry bounds, pixel metrics, capture, thresholds, transparency, or visual regression work.",
  },
];

await rm(outputRoot, { recursive: true, force: true });
await mkdir(resolve(outputRoot, "skills"), { recursive: true });

for (const skill of skills) {
  const source = resolve(sourceRoot, skill.id);
  const target = resolve(outputRoot, "skills", skill.id);
  const content = await readFile(resolve(source, "SKILL.md"), "utf8");
  if (!content.startsWith("---\n") || !content.includes(`\nname: ${skill.id}\n`)) {
    throw new Error(`invalid skill frontmatter: ${skill.id}`);
  }
  await cp(source, target, { recursive: true });
}

const definitions = JSON.stringify(skills, null, 2);
const pluginSource = `import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { Plugin } from "@opencode-ai/plugin";

const skills = ${definitions};

export default Plugin.define({
  id: "figma-rust",
  setup: async (ctx) => {
    await ctx.skill.transform((draft) => {
      const existing = new Set(draft.list().map((skill) => String(skill.id)));
      for (const skill of skills) {
        if (existing.has(skill.id)) continue;
        const location = fileURLToPath(
          new URL(\`./skills/\${skill.id}/SKILL.md\`, import.meta.url),
        );
        draft.add({
          ...skill,
          slash: false,
          autoinvoke: true,
          location,
          content: readFileSync(location, "utf8"),
        });
      }
    });
  },
});
`;

await writeFile(resolve(outputRoot, "plugin.js"), pluginSource, "utf8");
