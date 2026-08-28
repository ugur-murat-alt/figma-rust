import assert from "node:assert/strict";
import { existsSync, readFileSync } from "node:fs";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";

const packageRoot = fileURLToPath(new URL(".", import.meta.url));
const pluginPath = resolve(packageRoot, "dist", "plugin.js");
assert.ok(existsSync(pluginPath), "run npm run build before npm test");

const plugin = (await import(`${new URL("./dist/plugin.js", import.meta.url)}?test=${Date.now()}`))
  .default;
assert.equal(plugin.id, "figma-rust");

const allAdded = [];
await plugin.setup({
  skill: {
    transform: async (transform) =>
      transform({
        list: () => [],
        add: (skill) => allAdded.push(skill),
      }),
  },
});
assert.deepEqual(
  allAdded.map((skill) => skill.id),
  [
    "figma-rust",
    "figma-rust-extract-compile",
    "figma-rust-semantic-gpui",
    "figma-rust-visual-verification",
  ],
  "a project without local copies must receive the complete skill suite",
);

const added = [];
await plugin.setup({
  skill: {
    transform: async (transform) =>
      transform({
        list: () => [{ id: "figma-rust" }],
        add: (skill) => added.push(skill),
      }),
  },
});

assert.deepEqual(
  added.map((skill) => skill.id),
  [
    "figma-rust-extract-compile",
    "figma-rust-semantic-gpui",
    "figma-rust-visual-verification",
  ],
  "project-local skills must win while missing package skills are registered",
);

for (const skill of allAdded) {
  assert.equal(skill.slash, false);
  assert.equal(skill.autoinvoke, true);
  assert.ok(existsSync(skill.location), `${skill.id} location must exist`);
  assert.equal(skill.content, readFileSync(skill.location, "utf8"));
  assert.match(skill.content, new RegExp(`\\nname: ${skill.id}\\n`));
}

console.log("OpenCode2 skill package tests passed");
