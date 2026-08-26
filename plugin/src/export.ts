import type { ExtractionBundle, ExtractionDiagnostic, RawNode } from "./schema";

export const LARGE_BUNDLE_BYTES = 2 * 1024 * 1024;
export const BRIDGE_MAX_REQUEST_BYTES = 8 * 1024 * 1024;

export type ExportKind = "COMPILER" | "EVIDENCE";

export interface BundleSummary {
  root_count: number;
  node_count: number;
  variable_count: number;
  asset_count: number;
  diagnostics: {
    info: number;
    warnings: number;
    errors: number;
  };
  estimated_bytes: number;
  includes_rest_snapshot: boolean;
  large_selection: boolean;
  bridge_compatible: boolean;
}

export function includeRestSnapshotForExport(kind: ExportKind): boolean {
  return kind === "EVIDENCE";
}

export function classifyBundleBytes(bytes: number): Pick<BundleSummary, "large_selection" | "bridge_compatible"> {
  return {
    large_selection: bytes >= LARGE_BUNDLE_BYTES,
    bridge_compatible: bytes <= BRIDGE_MAX_REQUEST_BYTES,
  };
}

export function summarizeBundle(bundle: ExtractionBundle): BundleSummary {
  const estimatedBytes = utf8ByteLength(JSON.stringify(bundle));
  return {
    root_count: bundle.roots.length,
    node_count: bundle.roots.reduce((total, root) => total + countNodes(root), 0),
    variable_count: bundle.variables.length,
    asset_count: bundle.assets.length,
    diagnostics: summarizeDiagnostics(bundle.extraction_diagnostics),
    estimated_bytes: estimatedBytes,
    includes_rest_snapshot: bundle.rest_snapshot !== undefined,
    ...classifyBundleBytes(estimatedBytes),
  };
}

function utf8ByteLength(value: string): number {
  let bytes = 0;
  for (let index = 0; index < value.length; index += 1) {
    const codeUnit = value.charCodeAt(index);
    if (codeUnit <= 0x7f) {
      bytes += 1;
    } else if (codeUnit <= 0x7ff) {
      bytes += 2;
    } else if (
      codeUnit >= 0xd800
      && codeUnit <= 0xdbff
      && index + 1 < value.length
      && value.charCodeAt(index + 1) >= 0xdc00
      && value.charCodeAt(index + 1) <= 0xdfff
    ) {
      bytes += 4;
      index += 1;
    } else {
      bytes += 3;
    }
  }
  return bytes;
}

function countNodes(node: RawNode): number {
  return 1 + node.children.reduce((total, child) => total + countNodes(child), 0);
}

function summarizeDiagnostics(diagnostics: ExtractionDiagnostic[]): BundleSummary["diagnostics"] {
  return diagnostics.reduce<BundleSummary["diagnostics"]>(
    (summary, diagnostic) => {
      if (diagnostic.severity === "INFO") summary.info += 1;
      else if (diagnostic.severity === "WARNING") summary.warnings += 1;
      else summary.errors += 1;
      return summary;
    },
    { info: 0, warnings: 0, errors: 0 },
  );
}
