import { build } from "esbuild";
import { copyFile, mkdir } from "node:fs/promises";

await mkdir("dist", { recursive: true });
await build({
  entryPoints: ["src/main.ts"],
  bundle: true,
  outfile: "dist/main.js",
  format: "iife",
  target: "es2022",
  logLevel: "info",
});
await copyFile("src/ui.html", "dist/ui.html");
