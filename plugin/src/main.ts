import {
  createExtractionDeadline,
  extractNodes,
  extractSelection,
  isExtractionDeadlineError,
  withExtractionDeadline,
  type ExtractionDeadline,
} from "./extract";
import {
  includeRestSnapshotForExport,
  summarizeBundle,
  type ExportKind,
} from "./export";
import type { CompilerResponse, ExtractionDiagnostic, ExtractionBundle } from "./schema";

declare const __html__: string;

const COMPILER_URL = "http://127.0.0.1:38421";
const CODEGEN_TIMEOUT_MS = 2_500;
const UI_COMPILER_TIMEOUT_MS = 10_000;

if (figma.editorType === "dev") {
  figma.codegen.on("generate", async ({ node, language }) => {
    if (language !== "RUST_GPUI") return [];

    const deadline = createExtractionDeadline(CODEGEN_TIMEOUT_MS);
    try {
      const result = await withExtractionDeadline(
        generateCodegenResult(node, deadline),
        deadline,
      );
      return [result];
    } catch (error) {
      const message = isExtractionDeadlineError(error)
        ? "The 2500ms budget expired during extraction, the compiler request, or response body parsing."
        : error instanceof Error
          ? error.message
          : String(error);
      return [
        {
          title: "figma-rust codegen unavailable",
          code: [
            "// figma-rust could not complete Rust / GPUI code generation.",
            `// ${message}`,
            "// The Dev Mode budget covers node extraction, the compiler HTTP request, and response.json().",
            "// Start the local compiler bridge if this was not a deadline:",
            "//   figma-rust serve --port 38421",
          ].join("\n"),
          language: "RUST",
        },
      ];
    }
  });
} else {
  figma.showUI(__html__, { width: 420, height: 560, themeColors: true });

  figma.ui.onmessage = async (message: { type?: string; export_kind?: ExportKind }) => {
    if (message.type === "extract") {
      try {
        const exportKind: ExportKind = message.export_kind === "EVIDENCE" ? "EVIDENCE" : "COMPILER";
        const bundle = await extractSelection(includeRestSnapshotForExport(exportKind));
        figma.ui.postMessage({
          type: "bundle",
          bundle,
          export_kind: exportKind,
          summary: summarizeBundle(bundle),
        });
      } catch (error) {
        postUiError(error);
      }
      return;
    }
    if (message.type === "lint") {
      try {
        const bundle = await extractSelection(false, undefined, false);
        const result = await callCompiler(
          "/lint",
          bundle,
          createExtractionDeadline(UI_COMPILER_TIMEOUT_MS),
        );
        figma.ui.postMessage({ type: "lint-result", result });
      } catch (error) {
        postUiError(error);
      }
    }
  };

  void extractSelection(false, undefined, false).then((bundle) => {
    figma.ui.postMessage({ type: "bundle-preview", bundle, summary: summarizeBundle(bundle) });
  }, postUiError);
}

function postUiError(error: unknown): void {
  figma.ui.postMessage({
    type: "error",
    message: error instanceof Error ? error.message : String(error),
  });
}

async function generateCodegenResult(
  node: SceneNode,
  deadline: ExtractionDeadline,
): Promise<CodegenResult> {
  const bundle = await extractNodeSelection(node, deadline);
  const response = await callCompiler("/compile", bundle, deadline);
  const diagnostics = formatDiagnostics(response);
  return {
    title: "Rust / GPUI",
    code: `${response.code ?? "// Compiler returned no Rust output."}${diagnostics}`,
    language: "RUST",
  };
}

async function extractNodeSelection(
  node: SceneNode,
  deadline: ExtractionDeadline,
): Promise<ExtractionBundle> {
  return extractNodes([node], false, deadline);
}

async function callCompiler(
  path: string,
  bundle: ExtractionBundle,
  deadline: ExtractionDeadline,
): Promise<CompilerResponse> {
  const response = await withExtractionDeadline(
    fetch(`${COMPILER_URL}${path}`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(bundle),
    }),
    deadline,
  );
  if (!response.ok) {
    throw new Error(`compiler returned HTTP ${response.status}`);
  }
  return (await withExtractionDeadline(response.json(), deadline)) as CompilerResponse;
}

function formatDiagnostics(response: CompilerResponse): string {
  const diagnostics = response.diagnostics ?? [];
  if (diagnostics.length === 0 && !response.error) return "";
  return [
    "",
    "",
    "// figma-rust diagnostics:",
    ...diagnostics.map(formatDiagnostic),
    ...(response.error ? [`// [ERROR] ${response.error}`] : []),
  ].join("\n");
}

function formatDiagnostic(diagnostic: ExtractionDiagnostic): string {
  return `// [${diagnostic.severity}] ${diagnostic.code}${diagnostic.node_id ? ` (${diagnostic.node_id})` : ""}: ${diagnostic.message}`;
}
