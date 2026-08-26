import {
  PLUGIN_TYPINGS_VERSION,
  SCHEMA_VERSION,
  type ExtractionBundle,
  type ExtractionDiagnostic,
  type JsonValue,
  type RawAction,
  type RawAlignment,
  type RawAsset,
  type RawAxisSizing,
  type RawBlendMode,
  type RawBoundValue,
  type RawColor,
  type RawComponent,
  type RawComponentMetadata,
  type RawComponentValue,
  type RawConstraint,
  type RawEffect,
  type RawExtensions,
  type RawGradientKind,
  type RawGrid,
  type RawGridPlacement,
  type RawGridTrack,
  type RawImageScaleMode,
  type RawLayout,
  type RawLayoutMode,
  type RawLiteral,
  type RawModeContext,
  type RawNode,
  type RawNodeKind,
  type RawPaint,
  type RawPosition,
  type RawPositioning,
  type RawReaction,
  type RawRadii,
  type RawScroll,
  type RawSize,
  type RawStrokeAlign,
  type RawStyle,
  type RawText,
  type RawTextRun,
  type RawTextStyle,
  type RawTransform,
  type RawTrigger,
  type RawVariable,
} from "./schema";

export const MAX_TRAVERSAL_NODES = 2_000;
export const MAX_TRAVERSAL_DEPTH = 64;
export const MAX_IMAGE_ASSETS = 128;
const TRANSFORM_EPSILON = 1e-12;

const TEXT_SEGMENT_FIELDS = [
  "fontSize",
  "fontName",
  "fontWeight",
  "fontStyle",
  "lineHeight",
  "letterSpacing",
  "fills",
  "boundVariables",
] as const;

type UnknownRecord = Record<string, unknown>;
type FigmaTransform = RawTransform["matrix"];

interface ExtractedPosition {
  position: RawPosition;
  sourceTransform: FigmaTransform;
}

export interface ExtractionDeadline {
  readonly expiresAt: number;
}

interface VariableReference {
  id: string;
  node: SceneNode;
  propertyPath: string;
}

interface ExtractionContext {
  diagnostics: ExtractionDiagnostic[];
  variableReferences: Map<string, VariableReference[]>;
  boundValues: Map<string, Array<{ mode_context?: RawModeContext }>>;
  components: Map<string, RawComponent>;
  assets: Map<string, RawAsset>;
  assetChecks: Map<string, Promise<void>>;
  nodes: Map<string, SceneNode>;
  nodeCount: number;
  includeAssetPayloads: boolean;
  deadline?: ExtractionDeadline;
}

export function createExtractionDeadline(timeoutMs: number): ExtractionDeadline {
  return { expiresAt: Date.now() + Math.max(0, timeoutMs) };
}

export function withExtractionDeadline<T>(
  operation: Promise<T>,
  deadline: ExtractionDeadline,
): Promise<T> {
  const remaining = deadline.expiresAt - Date.now();
  if (remaining <= 0) return Promise.reject(deadlineError());

  return new Promise<T>((resolve, reject) => {
    const timeout = setTimeout(() => reject(deadlineError()), remaining);
    operation.then(
      (value) => {
        clearTimeout(timeout);
        resolve(value);
      },
      (error) => {
        clearTimeout(timeout);
        reject(error);
      },
    );
  });
}

export async function extractSelection(
  includeRestSnapshot = false,
  deadline?: ExtractionDeadline,
  includeAssetPayloads = true,
): Promise<ExtractionBundle> {
  return extractNodes(
    figma.currentPage.selection,
    includeRestSnapshot,
    deadline,
    includeAssetPayloads,
  );
}

export async function extractNodes(
  selection: readonly SceneNode[],
  includeRestSnapshot = false,
  deadline?: ExtractionDeadline,
  includeAssetPayloads = true,
): Promise<ExtractionBundle> {
  const context: ExtractionContext = {
    diagnostics: [],
    variableReferences: new Map(),
    boundValues: new Map(),
    components: new Map(),
    assets: new Map(),
    assetChecks: new Map(),
    nodes: new Map(),
    nodeCount: 0,
    includeAssetPayloads,
    deadline,
  };

  if (selection.length === 0) {
    addDiagnostic(
      context,
      "ERROR",
      "FR-EXTRACT-001",
      "Select at least one frame, component, instance, or scene node.",
    );
  }

  const roots: RawNode[] = [];
  for (const node of selection) {
    assertWithinDeadline(context.deadline);
    if (context.nodeCount >= MAX_TRAVERSAL_NODES) {
      addDiagnostic(
        context,
        "ERROR",
        "FR-EXTRACT-LIMIT-001",
        `Traversal stopped after ${MAX_TRAVERSAL_NODES} nodes.`,
        node.id,
        "roots",
      );
      continue;
    }
    roots.push(await extractNode(node, 0, context));
  }

  let restSnapshot: JsonValue | undefined;
  if (includeRestSnapshot && selection.length === 1) {
    assertWithinDeadline(context.deadline);
    try {
      const snapshot = await awaitWithOptionalDeadline(
        selection[0].exportAsync({ format: "JSON_REST_V1" }),
        context.deadline,
      );
      restSnapshot = toJson(snapshot);
    } catch (error) {
      if (isDeadlineError(error)) throw error;
      addDiagnostic(
        context,
        "WARNING",
        "FR-EXTRACT-REST-001",
        `JSON_REST_V1 export failed: ${errorMessage(error)}`,
        selection[0].id,
        "rest_snapshot",
      );
    }
  }

  if (context.assetChecks.size > 0) {
    await awaitWithOptionalDeadline(
      Promise.all(context.assetChecks.values()),
      context.deadline,
    );
  }

  const variables = await resolveVariables(context);
  assertWithinDeadline(context.deadline);
  context.diagnostics.sort(compareDiagnostics);

  return {
    schema_version: SCHEMA_VERSION,
    source: {
      ...(figma.fileKey ? { file_key: figma.fileKey } : {}),
      page_id: figma.currentPage.id,
      selected_node_ids: selection.map((node) => node.id),
      plugin_api_version: PLUGIN_TYPINGS_VERSION,
    },
    roots,
    variables,
    components: [...context.components.values()].sort(compareComponents),
    assets: [...context.assets.values()].sort(compareAssets),
    extraction_diagnostics: context.diagnostics,
    ...(restSnapshot === undefined ? {} : { rest_snapshot: restSnapshot }),
  };
}

async function extractNode(
  node: SceneNode,
  depth: number,
  context: ExtractionContext,
  coordinateParentTransform?: FigmaTransform,
  capturedByAncestor = false,
): Promise<RawNode> {
  assertWithinDeadline(context.deadline);
  context.nodeCount += 1;
  context.nodes.set(node.id, node);

  const record = node as unknown as UnknownRecord;
  const nodeType = stringValue(field(record, "type")) ?? "UNKNOWN";
  const extensions: RawExtensions = { figma_node_type: nodeType };
  const locked = field(record, "locked");
  if (typeof locked === "boolean") extensions.figma_locked = locked;

  const absoluteBoundingBox = field(record, "absoluteBoundingBox");
  if (absoluteBoundingBox !== undefined && absoluteBoundingBox !== null) {
    extensions.figma_absolute_bounding_box = toJson(absoluteBoundingBox);
  }

  const boundVariables = field(record, "boundVariables");
  if (boundVariables !== undefined) {
    extensions.figma_bound_variables = toJson(boundVariables);
  }

  const kind = nodeKind(nodeType);
  if (kind === undefined) {
    addDiagnostic(
      context,
      "ERROR",
      "FR-NODE-EXTRACT-001",
      `Node type ${nodeType} is not representable by the raw model; using structural GROUP fallback.`,
      node.id,
      "kind",
    );
  }

  const layout = extractLayout(record, node.id, context, extensions);
  const size = extractSize(record, node.id, context);
  const extractedPosition = extractPosition(record, node.id, context, coordinateParentTransform);
  const style = extractStyle(record, node.id, context, extensions);

  let text: RawText | undefined;
  if (nodeType === "TEXT") {
    text = extractText(node as TextNode, context, extensions);
  }
  const capturesSubtree = fallbackExportFormatFor(kind ?? "GROUP", style, text) !== undefined;
  const children = await extractChildren(
    record,
    node.id,
    nodeType,
    depth,
    context,
    extractedPosition.sourceTransform,
    capturedByAncestor || capturesSubtree,
  );

  const component = await extractComponentMetadata(node, context);
  const reactions = extractReactions(record, node.id, context, extensions);
  diagnoseNodeVariableBindings(record, node, context);

  const raw: RawNode = {
    id: node.id,
    name: node.name,
    figma_node_type: nodeType,
    ...(typeof locked === "boolean" ? { figma_locked: locked } : {}),
    kind: kind ?? "GROUP",
    visible: booleanValue(field(record, "visible"), true, node.id, "visible", context),
    opacity: numberValue(field(record, "opacity"), 1, node.id, "opacity", context),
    layout,
    size,
    position: extractedPosition.position,
    style,
    ...(text === undefined ? {} : { text }),
    ...(component === undefined ? {} : { component }),
    reactions,
    children,
    ...extensions,
  };

  if (!capturedByAncestor) registerNodeFallbackAsset(node, raw, context);

  return raw;
}

export type FallbackExportFormat = "SVG" | "PNG";

export function fallbackExportFormat(node: RawNode): FallbackExportFormat | undefined {
  return fallbackExportFormatFor(node.kind, node.style, node.text);
}

function fallbackExportFormatFor(
  kind: RawNodeKind,
  style: RawStyle,
  text: RawText | undefined,
): FallbackExportFormat | undefined {
  const paints = [...style.fills, ...style.strokes];
  const requiresRaster = !["NORMAL", "PASS_THROUGH"].includes(style.blend_mode)
    || style.effects.some((effect) => !["DROP_SHADOW", "INNER_SHADOW"].includes(effect.kind))
    || paints.some((paint) => paint.kind === "VIDEO" || paint.kind === "SHADER");
  if (requiresRaster) return "PNG";

  const requiresSvg = kind === "VECTOR"
    || style.is_mask
    || style.strokes.length > 1
    || paints.some((paint) => paint.kind === "PATTERN")
    || text?.runs.some((run) => run.style.letter_spacing !== undefined && run.style.letter_spacing !== 0) === true;
  return requiresSvg ? "SVG" : undefined;
}

function registerNodeFallbackAsset(
  node: SceneNode,
  raw: RawNode,
  context: ExtractionContext,
): void {
  const format = fallbackExportFormat(raw);
  if (format === undefined) return;
  const id = `node:${node.id}:${format.toLowerCase()}`;
  if (context.assets.has(id) || context.assetChecks.has(id)) return;
  if (context.assetChecks.size >= MAX_IMAGE_ASSETS) {
    addDiagnostic(
      context,
      "ERROR",
      "FR-ASSET-LIMIT-001",
      `Asset resolution stopped after ${MAX_IMAGE_ASSETS} unique assets.`,
      node.id,
      "asset_decision.route",
    );
    return;
  }

  if (!context.includeAssetPayloads) {
    context.assets.set(id, {
      id,
      source_node_id: node.id,
      media_type: format === "SVG" ? "image/svg+xml" : "image/png",
      export_settings: { format },
    });
    return;
  }

  let exportOperation: Promise<Uint8Array>;
  try {
    exportOperation = node.exportAsync({ format });
  } catch (error) {
    addDiagnostic(
      context,
      "ERROR",
      "FR-ASSET-EXTRACT-007",
      `${format} fallback export failed: ${errorMessage(error)}; no asset was added.`,
      node.id,
      "asset_decision.route",
    );
    return;
  }
  const check = exportOperation.then(
    (bytes) => {
      if (bytes.byteLength === 0) {
        addDiagnostic(
          context,
          "ERROR",
          "FR-ASSET-EXTRACT-006",
          `${format} fallback export returned no bytes; no asset was added.`,
          node.id,
          "asset_decision.route",
        );
        return;
      }
      const payload = encodeAssetPayload(bytes, node.id, "asset_decision.route", context);
      if (payload === undefined) return;
      context.assets.set(id, {
        id,
        source_node_id: node.id,
        media_type: format === "SVG" ? "image/svg+xml" : "image/png",
        export_settings: { format },
        payload_base64: payload,
      });
    },
    (error) => {
      addDiagnostic(
        context,
        "ERROR",
        "FR-ASSET-EXTRACT-007",
        `${format} fallback export failed: ${errorMessage(error)}; no asset was added.`,
        node.id,
        "asset_decision.route",
      );
    },
  );
  context.assetChecks.set(id, check);
}

async function extractChildren(
  record: UnknownRecord,
  nodeId: string,
  nodeType: string,
  depth: number,
  context: ExtractionContext,
  sourceTransform: FigmaTransform,
  capturedByAncestor: boolean,
): Promise<RawNode[]> {
  const value = field(record, "children");
  if (value === undefined || value === null) return [];
  if (!Array.isArray(value)) {
    addDiagnostic(
      context,
      "ERROR",
      "FR-NODE-EXTRACT-002",
      "The node children property is not an array.",
      nodeId,
      "children",
    );
    return [];
  }
  if (depth >= MAX_TRAVERSAL_DEPTH && value.length > 0) {
    addDiagnostic(
      context,
      "ERROR",
      "FR-EXTRACT-LIMIT-002",
      `Traversal stopped at maximum depth ${MAX_TRAVERSAL_DEPTH}.`,
      nodeId,
      "children",
    );
    return [];
  }

  const children: RawNode[] = [];
  // Figma skips groups and boolean operations when choosing a transform container.
  const childCoordinateParent = isNonContainerGroup(nodeType) ? sourceTransform : undefined;
  for (let index = 0; index < value.length; index += 1) {
    assertWithinDeadline(context.deadline);
    if (context.nodeCount >= MAX_TRAVERSAL_NODES) {
      addDiagnostic(
        context,
        "ERROR",
        "FR-EXTRACT-LIMIT-001",
        `Traversal stopped after ${MAX_TRAVERSAL_NODES} nodes.`,
        nodeId,
        `children[${index}]`,
      );
      break;
    }
    const child = value[index];
    if (!isRecord(child) || typeof field(child, "id") !== "string") {
      addDiagnostic(
        context,
        "ERROR",
        "FR-NODE-EXTRACT-003",
        "A child is not a Figma scene node.",
        nodeId,
        `children[${index}]`,
      );
      continue;
    }
    children.push(
      await extractNode(
        child as unknown as SceneNode,
        depth + 1,
        context,
        childCoordinateParent,
        capturedByAncestor,
      ),
    );
  }
  return children;
}

function extractLayout(
  record: UnknownRecord,
  nodeId: string,
  context: ExtractionContext,
  extensions: RawExtensions,
): RawLayout {
  const layoutMode = enumValue<RawLayoutMode>(
    field(record, "layoutMode"),
    ["NONE", "HORIZONTAL", "VERTICAL", "GRID"],
    "NONE",
    nodeId,
    "layout.mode",
    context,
  );
  const layoutWrap = enumValue(field(record, "layoutWrap"), ["NO_WRAP", "WRAP"], "NO_WRAP", nodeId, "layout.wrap", context);
  const primaryAlignment = mapAlignment(
    field(record, "primaryAxisAlignItems"),
    nodeId,
    "layout.primary_alignment",
    context,
  );
  const counterAlignment = mapAlignment(
    field(record, "counterAxisAlignItems"),
    nodeId,
    "layout.counter_alignment",
    context,
  );

  const itemSpacing = numberValue(field(record, "itemSpacing"), 0, nodeId, "layout.gap", context);
  const counterAxisSpacing = numberValue(
    field(record, "counterAxisSpacing"),
    itemSpacing,
    nodeId,
    "layout.counter_axis_spacing",
    context,
  );
  if (layoutWrap === "WRAP" && counterAxisSpacing !== itemSpacing) {
    addLossDiagnostic(
      context,
      nodeId,
      "layout.counter_axis_spacing",
      "Wrapped auto-layout uses a separate counter-axis gap that the raw model cannot represent.",
      extensions,
      "figma_counter_axis_spacing",
      counterAxisSpacing,
    );
  }

  const counterAxisAlignContent = field(record, "counterAxisAlignContent");
  if (counterAxisAlignContent !== undefined && counterAxisAlignContent !== "AUTO") {
    addLossDiagnostic(
      context,
      nodeId,
      "layout.counter_axis_align_content",
      "Counter-axis content alignment is not represented by the raw layout model.",
      extensions,
      "figma_counter_axis_align_content",
      toJson(counterAxisAlignContent),
    );
  }

  const layoutGrow = optionalNumber(field(record, "layoutGrow"), nodeId, "layout.layout_grow", context);
  if (layoutGrow !== undefined && layoutGrow !== 0) {
    addLossDiagnostic(
      context,
      nodeId,
      "layout.layout_grow",
      "Child layout growth is not represented independently of axis sizing.",
      extensions,
      "figma_layout_grow",
      layoutGrow,
    );
  }

  const layoutAlign = field(record, "layoutAlign");
  if (layoutAlign !== undefined && layoutAlign !== "INHERIT") {
    addLossDiagnostic(
      context,
      nodeId,
      "layout.layout_align",
      "Child counter-axis alignment is not represented independently by the raw model.",
      extensions,
      "figma_layout_align",
      toJson(layoutAlign),
    );
  }

  const gridAutoTracks = field(record, "gridAutoTracks");
  if (gridAutoTracks !== undefined && gridAutoTracks !== "NONE") {
    addLossDiagnostic(
      context,
      nodeId,
      "layout.grid_auto_tracks",
      "Automatically created grid tracks are not represented by the raw model.",
      extensions,
      "figma_grid_auto_tracks",
      toJson(gridAutoTracks),
    );
  }

  const gridItemsPositioning = field(record, "gridItemsPositioning");
  if (gridItemsPositioning !== undefined && gridItemsPositioning !== "MANUAL") {
    addLossDiagnostic(
      context,
      nodeId,
      "layout.grid_items_positioning",
      "Automatic grid item placement is not represented by the raw model.",
      extensions,
      "figma_grid_items_positioning",
      toJson(gridItemsPositioning),
    );
  }

  const gridChildHorizontalAlign = field(record, "gridChildHorizontalAlign");
  if (gridChildHorizontalAlign !== undefined && gridChildHorizontalAlign !== "AUTO") {
    addLossDiagnostic(
      context,
      nodeId,
      "position.grid_child_horizontal_align",
      "Grid child horizontal alignment is not represented by the raw model.",
      extensions,
      "figma_grid_child_horizontal_align",
      toJson(gridChildHorizontalAlign),
    );
  }
  const gridChildVerticalAlign = field(record, "gridChildVerticalAlign");
  if (gridChildVerticalAlign !== undefined && gridChildVerticalAlign !== "AUTO") {
    addLossDiagnostic(
      context,
      nodeId,
      "position.grid_child_vertical_align",
      "Grid child vertical alignment is not represented by the raw model.",
      extensions,
      "figma_grid_child_vertical_align",
      toJson(gridChildVerticalAlign),
    );
  }

  const overflowDirection = enumValue(
    field(record, "overflowDirection"),
    ["NONE", "HORIZONTAL", "VERTICAL", "BOTH"],
    "NONE",
    nodeId,
    "layout.scroll",
    context,
  );

  const grid: RawGrid = {
    columns: extractGridTracks(field(record, "gridColumnSizes"), nodeId, "layout.grid.columns", context),
    rows: extractGridTracks(field(record, "gridRowSizes"), nodeId, "layout.grid.rows", context),
    column_gap: numberValue(field(record, "gridColumnGap"), 0, nodeId, "layout.grid.column_gap", context),
    row_gap: numberValue(field(record, "gridRowGap"), 0, nodeId, "layout.grid.row_gap", context),
  };

  const strokesIncludedInLayout = field(record, "strokesIncludedInLayout");
  if (strokesIncludedInLayout === true) {
    addLossDiagnostic(
      context,
      nodeId,
      "layout.strokes_included_in_layout",
      "Stroke inclusion in auto-layout calculations is not represented by the raw model.",
      extensions,
      "figma_strokes_included_in_layout",
      true,
    );
  }

  return {
    mode: layoutMode,
    wrap: layoutWrap === "WRAP",
    primary_alignment: primaryAlignment,
    counter_alignment: counterAlignment,
    gap: boundNumberFromRecord(record, "itemSpacing", itemSpacing, nodeId, "layout.gap", context),
    padding: {
      top: boundNumberFromRecord(record, "paddingTop", numberValue(field(record, "paddingTop"), 0, nodeId, "layout.padding.top", context), nodeId, "layout.padding.top", context),
      right: boundNumberFromRecord(record, "paddingRight", numberValue(field(record, "paddingRight"), 0, nodeId, "layout.padding.right", context), nodeId, "layout.padding.right", context),
      bottom: boundNumberFromRecord(record, "paddingBottom", numberValue(field(record, "paddingBottom"), 0, nodeId, "layout.padding.bottom", context), nodeId, "layout.padding.bottom", context),
      left: boundNumberFromRecord(record, "paddingLeft", numberValue(field(record, "paddingLeft"), 0, nodeId, "layout.padding.left", context), nodeId, "layout.padding.left", context),
    },
    grid,
    clips_content: booleanValue(field(record, "clipsContent"), false, nodeId, "layout.clips_content", context),
    scroll: {
      horizontal: overflowDirection === "HORIZONTAL" || overflowDirection === "BOTH",
      vertical: overflowDirection === "VERTICAL" || overflowDirection === "BOTH",
    },
  };
}

function extractSize(record: UnknownRecord, nodeId: string, context: ExtractionContext): RawSize {
  const targetAspectRatio = field(record, "targetAspectRatio");
  const width = optionalNumber(field(record, "width"), nodeId, "size.width", context);
  const height = optionalNumber(field(record, "height"), nodeId, "size.height", context);
  const horizontal = axisSizing(field(record, "layoutSizingHorizontal"), nodeId, "size.horizontal", context);
  const vertical = axisSizing(field(record, "layoutSizingVertical"), nodeId, "size.vertical", context);
  const minWidth = optionalNumber(field(record, "minWidth"), nodeId, "size.min_width", context);
  const maxWidth = optionalNumber(field(record, "maxWidth"), nodeId, "size.max_width", context);
  const minHeight = optionalNumber(field(record, "minHeight"), nodeId, "size.min_height", context);
  const maxHeight = optionalNumber(field(record, "maxHeight"), nodeId, "size.max_height", context);
  let aspectRatio: number | undefined;
  if (targetAspectRatio !== undefined && targetAspectRatio !== null) {
    const ratio = isRecord(targetAspectRatio)
      ? [optionalNumber(field(targetAspectRatio, "x"), nodeId, "size.aspect_ratio.x", context), optionalNumber(field(targetAspectRatio, "y"), nodeId, "size.aspect_ratio.y", context)]
      : [undefined, undefined];
    if (ratio[0] !== undefined && ratio[1] !== undefined && ratio[0] > 0 && ratio[1] > 0) {
      aspectRatio = ratio[0] / ratio[1];
    } else {
      addDiagnostic(
        context,
        "ERROR",
        "FR-SIZE-EXTRACT-001",
        "The target aspect ratio is not a positive Figma vector.",
        nodeId,
        "size.aspect_ratio",
      );
    }
  }

  return {
    ...(width === undefined ? {} : { width }),
    ...(height === undefined ? {} : { height }),
    ...(horizontal === undefined ? {} : { horizontal }),
    ...(vertical === undefined ? {} : { vertical }),
    ...(minWidth === undefined ? {} : { min_width: minWidth }),
    ...(maxWidth === undefined ? {} : { max_width: maxWidth }),
    ...(minHeight === undefined ? {} : { min_height: minHeight }),
    ...(maxHeight === undefined ? {} : { max_height: maxHeight }),
    ...(aspectRatio === undefined ? {} : { aspect_ratio: aspectRatio }),
  };
}

function extractPosition(
  record: UnknownRecord,
  nodeId: string,
  context: ExtractionContext,
  coordinateParentTransform?: FigmaTransform,
): ExtractedPosition {
  const positioningValue = field(record, "layoutPositioning");
  const positioning: RawPositioning = enumValue(
    positioningValue,
    ["AUTO", "ABSOLUTE"],
    "AUTO",
    nodeId,
    "position.positioning",
    context,
  );
  const constraints = field(record, "constraints");
  const horizontalConstraint = enumValue(
    isRecord(constraints) ? field(constraints, "horizontal") : undefined,
    ["MIN", "CENTER", "MAX", "STRETCH", "SCALE"],
    "MIN",
    nodeId,
    "position.horizontal_constraint",
    context,
  );
  const verticalConstraint = enumValue(
    isRecord(constraints) ? field(constraints, "vertical") : undefined,
    ["MIN", "CENTER", "MAX", "STRETCH", "SCALE"],
    "MIN",
    nodeId,
    "position.vertical_constraint",
    context,
  );

  const rowValue = field(record, "gridRowAnchorIndex");
  const columnValue = field(record, "gridColumnAnchorIndex");
  const hasGridPlacement = (rowValue !== undefined && rowValue !== -1)
    || (columnValue !== undefined && columnValue !== -1);
  const row = hasGridPlacement
    ? optionalUnsignedInteger(rowValue, 4_294_967_295, nodeId, "position.grid.row", context)
    : undefined;
  const column = hasGridPlacement
    ? optionalUnsignedInteger(columnValue, 4_294_967_295, nodeId, "position.grid.column", context)
    : undefined;
  const rowSpan = hasGridPlacement
    ? optionalUnsignedInteger(field(record, "gridRowSpan"), 4_294_967_295, nodeId, "position.grid.row_span", context)
    : undefined;
  const columnSpan = hasGridPlacement
    ? optionalUnsignedInteger(field(record, "gridColumnSpan"), 4_294_967_295, nodeId, "position.grid.column_span", context)
    : undefined;
  let grid: RawGridPlacement | undefined;
  if (row !== undefined || column !== undefined || rowSpan !== undefined || columnSpan !== undefined) {
    if (row === undefined || column === undefined || row < 0 || column < 0) {
      addDiagnostic(
        context,
        "ERROR",
        "FR-GRID-EXTRACT-001",
        "Grid placement is missing a non-negative row or column index.",
        nodeId,
        "position.grid",
      );
    } else if (rowSpan !== undefined && rowSpan < 1 || columnSpan !== undefined && columnSpan < 1) {
      addDiagnostic(
        context,
        "ERROR",
        "FR-GRID-EXTRACT-002",
        "Grid placement spans must be positive integers.",
        nodeId,
        "position.grid",
      );
    } else {
      grid = {
        row,
        column,
        row_span: rowSpan ?? 1,
        column_span: columnSpan ?? 1,
      };
    }
  }

  const x = numberValue(field(record, "x"), 0, nodeId, "position.x", context);
  const y = numberValue(field(record, "y"), 0, nodeId, "position.y", context);
  const sourceTransform = extractTransform(
    field(record, "relativeTransform"),
    x,
    y,
    nodeId,
    context,
  );
  const localTransform = coordinateParentTransform === undefined
    ? sourceTransform
    : relativeToDirectParent(sourceTransform, coordinateParentTransform, nodeId, context);

  return {
    sourceTransform,
    position: {
      positioning,
      x: localTransform[4],
      y: localTransform[5],
      horizontal_constraint: horizontalConstraint,
      vertical_constraint: verticalConstraint,
      ...(grid === undefined ? {} : { grid }),
      transform: {
        matrix: [
          localTransform[0],
          localTransform[1],
          localTransform[2],
          localTransform[3],
          0,
          0,
        ],
      },
    },
  };
}

function extractTransform(
  value: unknown,
  x: number,
  y: number,
  nodeId: string,
  context: ExtractionContext,
): FigmaTransform {
  if (value === undefined || value === null) return [1, 0, 0, 1, x, y];
  if (!Array.isArray(value) || value.length !== 2 || !Array.isArray(value[0]) || !Array.isArray(value[1])) {
    addDiagnostic(context, "ERROR", "FR-POSITION-EXTRACT-001", "relativeTransform is not a 2x3 Figma transform.", nodeId, "position.transform");
    return [1, 0, 0, 1, x, y];
  }
  const first = value[0];
  const second = value[1];
  if (first.length !== 3 || second.length !== 3 || [...first, ...second].some((item) => typeof item !== "number" || !Number.isFinite(item))) {
    addDiagnostic(context, "ERROR", "FR-POSITION-EXTRACT-001", "relativeTransform contains invalid numeric values.", nodeId, "position.transform");
    return [1, 0, 0, 1, x, y];
  }
  return [
    first[0] as number,
    second[0] as number,
    first[1] as number,
    second[1] as number,
    first[2] as number,
    second[2] as number,
  ];
}

function relativeToDirectParent(
  child: FigmaTransform,
  parent: FigmaTransform,
  nodeId: string,
  context: ExtractionContext,
): FigmaTransform {
  const [a, b, c, d, tx, ty] = parent;
  const determinant = a * d - b * c;
  if (!Number.isFinite(determinant) || Math.abs(determinant) <= TRANSFORM_EPSILON) {
    addTransformRebaseDiagnostic(context, nodeId);
    return child;
  }

  const inverse: FigmaTransform = [
    d / determinant,
    -b / determinant,
    -c / determinant,
    a / determinant,
    (c * ty - d * tx) / determinant,
    (b * tx - a * ty) / determinant,
  ];
  const local = multiplyTransforms(inverse, child).map(canonicalTransformValue) as FigmaTransform;
  if (local.some((value) => !Number.isFinite(value))) {
    addTransformRebaseDiagnostic(context, nodeId);
    return child;
  }
  return local;
}

function multiplyTransforms(left: FigmaTransform, right: FigmaTransform): FigmaTransform {
  return [
    left[0] * right[0] + left[2] * right[1],
    left[1] * right[0] + left[3] * right[1],
    left[0] * right[2] + left[2] * right[3],
    left[1] * right[2] + left[3] * right[3],
    left[0] * right[4] + left[2] * right[5] + left[4],
    left[1] * right[4] + left[3] * right[5] + left[5],
  ];
}

function canonicalTransformValue(value: number): number {
  if (Math.abs(value) <= TRANSFORM_EPSILON) return 0;
  if (Math.abs(value - 1) <= TRANSFORM_EPSILON) return 1;
  if (Math.abs(value + 1) <= TRANSFORM_EPSILON) return -1;
  return value;
}

function addTransformRebaseDiagnostic(context: ExtractionContext, nodeId: string): void {
  addDiagnostic(
    context,
    "ERROR",
    "FR-POSITION-EXTRACT-002",
    "The coordinate parent transform is singular or ill-conditioned; child coordinates were not rebased.",
    nodeId,
    "position.transform",
  );
}

function isNonContainerGroup(nodeType: string): boolean {
  return nodeType === "GROUP" || nodeType === "BOOLEAN_OPERATION";
}

function extractStyle(
  record: UnknownRecord,
  nodeId: string,
  context: ExtractionContext,
  extensions: RawExtensions,
): RawStyle {
  const strokeWeightValue = field(record, "strokeWeight");
  const strokeWeight = isMixed(strokeWeightValue)
    ? undefined
    : optionalNumber(strokeWeightValue, nodeId, "style.stroke_widths", context);
  const strokeToken = registerAliasFromRecord(record, "strokeWeight", nodeId, "style.stroke_widths", context);
  const strokeWidths = {
    top: boundNumberFromRecord(record, "strokeTopWeight", optionalNumber(field(record, "strokeTopWeight"), nodeId, "style.stroke_widths.top", context) ?? strokeWeight ?? 0, nodeId, "style.stroke_widths.top", context, strokeToken),
    right: boundNumberFromRecord(record, "strokeRightWeight", optionalNumber(field(record, "strokeRightWeight"), nodeId, "style.stroke_widths.right", context) ?? strokeWeight ?? 0, nodeId, "style.stroke_widths.right", context, strokeToken),
    bottom: boundNumberFromRecord(record, "strokeBottomWeight", optionalNumber(field(record, "strokeBottomWeight"), nodeId, "style.stroke_widths.bottom", context) ?? strokeWeight ?? 0, nodeId, "style.stroke_widths.bottom", context, strokeToken),
    left: boundNumberFromRecord(record, "strokeLeftWeight", optionalNumber(field(record, "strokeLeftWeight"), nodeId, "style.stroke_widths.left", context) ?? strokeWeight ?? 0, nodeId, "style.stroke_widths.left", context, strokeToken),
  };

  const cornerRadiusValue = field(record, "cornerRadius");
  const cornerRadius = isMixed(cornerRadiusValue)
    ? undefined
    : optionalNumber(cornerRadiusValue, nodeId, "style.radii", context);
  const cornerToken = registerAliasFromRecord(record, "cornerRadius", nodeId, "style.radii", context);
  const radii: RawRadii = {
    top_left: boundNumberFromRecord(record, "topLeftRadius", optionalNumber(field(record, "topLeftRadius"), nodeId, "style.radii.top_left", context) ?? cornerRadius ?? 0, nodeId, "style.radii.top_left", context, cornerToken),
    top_right: boundNumberFromRecord(record, "topRightRadius", optionalNumber(field(record, "topRightRadius"), nodeId, "style.radii.top_right", context) ?? cornerRadius ?? 0, nodeId, "style.radii.top_right", context, cornerToken),
    bottom_right: boundNumberFromRecord(record, "bottomRightRadius", optionalNumber(field(record, "bottomRightRadius"), nodeId, "style.radii.bottom_right", context) ?? cornerRadius ?? 0, nodeId, "style.radii.bottom_right", context, cornerToken),
    bottom_left: boundNumberFromRecord(record, "bottomLeftRadius", optionalNumber(field(record, "bottomLeftRadius"), nodeId, "style.radii.bottom_left", context) ?? cornerRadius ?? 0, nodeId, "style.radii.bottom_left", context, cornerToken),
    smoothing: numberValue(field(record, "cornerSmoothing"), 0, nodeId, "style.radii.smoothing", context),
  };

  const paints = field(record, "fills");
  const strokes = field(record, "strokes");
  const styleFills = extractPaints(paints, nodeId, "style.fills", context, extensions);
  const styleStrokes = extractPaints(strokes, nodeId, "style.strokes", context, extensions);

  const strokeCap = field(record, "strokeCap");
  if (styleStrokes.length > 0 && strokeCap !== undefined && strokeCap !== "NONE") {
    addLossDiagnostic(context, nodeId, "style.stroke_cap", "Stroke caps are not represented by the raw style model.", extensions, "figma_stroke_cap", toJson(strokeCap));
  }
  const strokeJoin = field(record, "strokeJoin");
  if (styleStrokes.length > 0 && strokeJoin !== undefined && strokeJoin !== "MITER") {
    addLossDiagnostic(context, nodeId, "style.stroke_join", "Stroke joins are not represented by the raw style model.", extensions, "figma_stroke_join", toJson(strokeJoin));
  }
  const dashPattern = field(record, "dashPattern");
  if (Array.isArray(dashPattern) && dashPattern.length > 0) {
    addLossDiagnostic(context, nodeId, "style.dash_pattern", "Dashed strokes are not represented by the raw style model.", extensions, "figma_dash_pattern", toJson(dashPattern));
  }

  const maskType = field(record, "maskType");
  if (maskType !== undefined && maskType !== "ALPHA") {
    addLossDiagnostic(context, nodeId, "style.mask_type", "Non-alpha mask semantics are not represented by the raw style model.", extensions, "figma_mask_type", toJson(maskType));
  }

  return {
    fills: styleFills,
    strokes: styleStrokes,
    stroke_widths: strokeWidths,
    stroke_align: enumValue<RawStrokeAlign>(field(record, "strokeAlign"), ["INSIDE", "CENTER", "OUTSIDE"], "INSIDE", nodeId, "style.stroke_align", context),
    radii,
    effects: extractEffects(field(record, "effects"), nodeId, context, extensions),
    blend_mode: mapBlendMode(field(record, "blendMode"), nodeId, context, extensions),
    is_mask: booleanValue(field(record, "isMask"), false, nodeId, "style.is_mask", context),
  };
}

function extractPaints(
  value: unknown,
  nodeId: string,
  propertyPath: string,
  context: ExtractionContext,
  extensions: RawExtensions,
): RawPaint[] {
  if (value === undefined || value === null) return [];
  if (isMixed(value)) {
    addDiagnostic(context, "ERROR", "FR-PAINT-EXTRACT-001", "A mixed paint value cannot be represented as a paint list.", nodeId, propertyPath);
    return [];
  }
  if (!Array.isArray(value)) {
    addDiagnostic(context, "ERROR", "FR-PAINT-EXTRACT-002", "The Figma paint property is not an array.", nodeId, propertyPath);
    return [];
  }

  const paints: RawPaint[] = [];
  for (let index = 0; index < value.length; index += 1) {
    const paint = value[index];
    const result = extractPaint(paint, nodeId, `${propertyPath}[${index}]`, index, context, extensions);
    if (result !== undefined) paints.push(result);
  }
  return paints;
}

function extractPaint(
  value: unknown,
  nodeId: string,
  propertyPath: string,
  index: number,
  context: ExtractionContext,
  extensions: RawExtensions,
): RawPaint | undefined {
  if (!isRecord(value)) {
    addDiagnostic(context, "ERROR", "FR-PAINT-EXTRACT-003", "A paint entry is not an object.", nodeId, propertyPath);
    return undefined;
  }
  const type = stringValue(field(value, "type"));
  if (type === undefined) {
    addDiagnostic(context, "ERROR", "FR-PAINT-EXTRACT-004", "A paint entry has no type.", nodeId, propertyPath);
    appendExtension(extensions, "figma_unsupported_paints", toJson(value));
    return undefined;
  }

  registerBoundVariableAliases(
    field(value, "boundVariables"),
    nodeId,
    `${propertyPath}.boundVariables`,
    new Set(type === "SOLID" ? ["color"] : []),
    context,
    extensions,
    "figma_paint_bound_variables",
  );

  const visible = field(value, "visible");
  if (visible === false) {
    addLossDiagnostic(context, nodeId, propertyPath, "Invisible paints are omitted because raw paints have no visibility flag.", extensions, "figma_invisible_paints", toJson(value));
    return undefined;
  }
  const opacity = optionalNumber(field(value, "opacity"), nodeId, `${propertyPath}.opacity`, context) ?? 1;
  const blendMode = field(value, "blendMode");
  if (blendMode !== undefined && blendMode !== "NORMAL") {
    addLossDiagnostic(context, nodeId, `${propertyPath}.blendMode`, "Paint blend modes are not represented by the raw paint model.", extensions, "figma_paint_blend_modes", toJson(value));
  }

  switch (type) {
    case "SOLID": {
      const literal = extractColor(field(value, "color"), opacity, nodeId, `${propertyPath}.color`, context);
      if (literal === undefined) return undefined;
      const tokenId = registerPaintVariable(value, "color", nodeId, `${propertyPath}.color`, context);
      return { kind: "SOLID", color: boundValue(literal, tokenId, nodeId, context) };
    }
    case "GRADIENT_LINEAR":
    case "GRADIENT_RADIAL":
    case "GRADIENT_ANGULAR":
    case "GRADIENT_DIAMOND": {
      const gradientKind = gradientKindFor(type);
      const transform = field(value, "gradientTransform");
      if (transform !== undefined) {
        addLossDiagnostic(context, nodeId, `${propertyPath}.gradientTransform`, "Gradient transforms are not represented by the raw paint model.", extensions, "figma_gradient_transforms", toJson(transform));
      }
      const stopsValue = field(value, "gradientStops");
      if (!Array.isArray(stopsValue)) {
        addDiagnostic(context, "ERROR", "FR-PAINT-GRADIENT-001", "Gradient paint has no gradient stop array.", nodeId, `${propertyPath}.gradientStops`);
        return undefined;
      }
      const stops = [];
      for (let stopIndex = 0; stopIndex < stopsValue.length; stopIndex += 1) {
        const stop = stopsValue[stopIndex];
        if (!isRecord(stop)) {
          addDiagnostic(context, "ERROR", "FR-PAINT-GRADIENT-002", "Gradient stop is not an object.", nodeId, `${propertyPath}.gradientStops[${stopIndex}]`);
          continue;
        }
        registerBoundVariableAliases(
          field(stop, "boundVariables"),
          nodeId,
          `${propertyPath}.gradientStops[${stopIndex}].boundVariables`,
          new Set(["color"]),
          context,
          extensions,
          "figma_gradient_stop_bound_variables",
        );
        const color = extractColor(field(stop, "color"), opacity, nodeId, `${propertyPath}.gradientStops[${stopIndex}].color`, context);
        const position = optionalNumber(field(stop, "position"), nodeId, `${propertyPath}.gradientStops[${stopIndex}].position`, context);
        if (color === undefined || position === undefined) continue;
        const tokenId = registerPaintVariable(stop, "color", nodeId, `${propertyPath}.gradientStops[${stopIndex}].color`, context);
        stops.push({ position, color: boundValue(color, tokenId, nodeId, context) });
      }
      return { kind: "GRADIENT", gradient_kind: gradientKind, stops };
    }
    case "IMAGE": {
      const imageHash = stringValue(field(value, "imageHash"));
      if (imageHash === undefined || imageHash.length === 0) {
        addDiagnostic(context, "ERROR", "FR-ASSET-EXTRACT-001", "Image paint has no image hash; no asset was added.", nodeId, `${propertyPath}.imageHash`);
        appendExtension(extensions, "figma_invalid_image_paints", toJson(value));
        return undefined;
      } else {
        registerImageAsset(imageHash, nodeId, propertyPath, context);
      }
      const scaleMode = enumValue<RawImageScaleMode>(field(value, "scaleMode"), ["FIT", "FILL", "CROP", "TILE"], "FIT", nodeId, `${propertyPath}.scaleMode`, context);
      if (field(value, "imageTransform") !== undefined || field(value, "scalingFactor") !== undefined || field(value, "rotation") !== undefined || field(value, "filters") !== undefined || opacity !== 1) {
        addLossDiagnostic(context, nodeId, propertyPath, "Image paint transform, filters, rotation, or opacity are not fully represented by the raw paint model.", extensions, "figma_image_paint_details", toJson(value));
      }
      return { kind: "IMAGE", asset_id: imageHash, scale_mode: scaleMode };
    }
    case "VIDEO":
      addLossDiagnostic(context, nodeId, propertyPath, "Video paint hash and scale semantics are not represented by the raw paint model.", extensions, "figma_video_paints", toJson(value));
      return { kind: "VIDEO" };
    case "PATTERN":
      addLossDiagnostic(context, nodeId, propertyPath, "Pattern paint geometry is not represented by the raw paint model.", extensions, "figma_pattern_paints", toJson(value));
      return { kind: "PATTERN" };
    case "SHADER":
      addLossDiagnostic(context, nodeId, propertyPath, "Shader paint properties are not represented by the raw paint model.", extensions, "figma_shader_paints", toJson(value));
      return { kind: "SHADER" };
    default:
      addDiagnostic(context, "ERROR", "FR-PAINT-EXTRACT-005", `Paint type ${type} is not representable by the raw model.`, nodeId, propertyPath);
      appendExtension(extensions, "figma_unsupported_paints", toJson(value));
      return undefined;
  }
}

function extractEffects(
  value: unknown,
  nodeId: string,
  context: ExtractionContext,
  extensions: RawExtensions,
): RawEffect[] {
  if (value === undefined || value === null) return [];
  if (isMixed(value) || !Array.isArray(value)) {
    addDiagnostic(context, "ERROR", "FR-EFFECT-EXTRACT-001", "The Figma effects property is not an array.", nodeId, "style.effects");
    return [];
  }
  const effects: RawEffect[] = [];
  for (let index = 0; index < value.length; index += 1) {
    const effect = value[index];
    if (!isRecord(effect)) {
      addDiagnostic(context, "ERROR", "FR-EFFECT-EXTRACT-002", "An effect entry is not an object.", nodeId, `style.effects[${index}]`);
      continue;
    }
    const path = `style.effects[${index}]`;
    const type = stringValue(field(effect, "type"));
    if (type === undefined) {
      addDiagnostic(context, "ERROR", "FR-EFFECT-EXTRACT-003", "An effect entry has no type.", nodeId, path);
      continue;
    }
    if (field(effect, "visible") === false) {
      addLossDiagnostic(context, nodeId, path, "Invisible effects are omitted because raw effects have no visibility flag.", extensions, "figma_invisible_effects", toJson(effect));
      continue;
    }
    if (field(effect, "blendMode") !== undefined && field(effect, "blendMode") !== "NORMAL") {
      addLossDiagnostic(context, nodeId, path, "Effect visibility or blend mode is not represented by the raw effect model.", extensions, "figma_effect_details", toJson(effect));
    }

    registerBoundVariableAliases(
      field(effect, "boundVariables"),
      nodeId,
      `${path}.boundVariables`,
      new Set(type === "DROP_SHADOW" || type === "INNER_SHADOW" ? ["color"] : []),
      context,
      extensions,
      "figma_effect_bound_variables",
    );

    if (type === "DROP_SHADOW" || type === "INNER_SHADOW") {
      const color = extractColor(field(effect, "color"), 1, nodeId, `${path}.color`, context);
      const offset = field(effect, "offset");
      if (color === undefined || !isRecord(offset)) {
        addDiagnostic(context, "ERROR", "FR-EFFECT-EXTRACT-004", "Shadow effect is missing a color or offset.", nodeId, path);
        continue;
      }
      const tokenId = registerPaintVariable(effect, "color", nodeId, `${path}.color`, context);
      const shadow = {
        color: boundValue(color, tokenId, nodeId, context),
        offset_x: numberValue(field(offset, "x"), 0, nodeId, `${path}.offset.x`, context),
        offset_y: numberValue(field(offset, "y"), 0, nodeId, `${path}.offset.y`, context),
        blur: numberValue(field(effect, "radius"), 0, nodeId, `${path}.radius`, context),
        spread: numberValue(field(effect, "spread"), 0, nodeId, `${path}.spread`, context),
      };
      effects.push(type === "DROP_SHADOW" ? { kind: "DROP_SHADOW", ...shadow } : { kind: "INNER_SHADOW", ...shadow });
      continue;
    }
    if (type === "LAYER_BLUR" || type === "BACKGROUND_BLUR") {
      const radius = numberValue(field(effect, "radius"), 0, nodeId, `${path}.radius`, context);
      effects.push(type === "LAYER_BLUR" ? { kind: "LAYER_BLUR", radius } : { kind: "BACKGROUND_BLUR", radius });
      continue;
    }
    const knownUnsupported: Record<string, RawEffect["kind"]> = {
      PROGRESSIVE: "PROGRESSIVE_BLUR",
      NOISE: "NOISE",
      TEXTURE: "TEXTURE",
      GLASS: "GLASS",
      SHADER: "SHADER",
    };
    const kind = knownUnsupported[type];
    if (kind !== undefined) {
      addLossDiagnostic(context, nodeId, path, `Effect ${type} has fields that are not represented by the raw effect model.`, extensions, "figma_unsupported_effects", toJson(effect));
      effects.push({ kind } as RawEffect);
      continue;
    }
    addDiagnostic(context, "ERROR", "FR-EFFECT-EXTRACT-005", `Effect type ${type} is not representable by the raw model.`, nodeId, path);
    appendExtension(extensions, "figma_unsupported_effects", toJson(effect));
  }
  return effects;
}

function extractText(node: TextNode, context: ExtractionContext, extensions: RawExtensions): RawText {
  const characters = node.characters;
  const textMetadata = {
    has_missing_font: node.hasMissingFont,
    text_auto_resize: node.textAutoResize,
    text_align_horizontal: node.textAlignHorizontal,
    text_align_vertical: node.textAlignVertical,
    text_truncation: node.textTruncation,
    max_lines: node.maxLines,
  };
  addLossDiagnostic(context, node.id, "text", "Text layout metadata outside characters and styled runs is retained only as a source extension.", extensions, "figma_text_metadata", toJson(textMetadata));
  if (node.hasMissingFont) {
    addDiagnostic(context, "ERROR", "FR-TEXT-FONT-001", "The text node uses a font unavailable to Figma.", node.id, "text.fontName");
  }

  let segments: Array<UnknownRecord> = [];
  try {
    segments = node.getStyledTextSegments([...TEXT_SEGMENT_FIELDS]) as Array<UnknownRecord>;
  } catch (error) {
    addDiagnostic(context, "ERROR", "FR-TEXT-EXTRACT-001", `Styled text extraction failed: ${errorMessage(error)}`, node.id, "text.runs");
  }

  const runs: RawTextRun[] = [];
  for (let index = 0; index < segments.length; index += 1) {
    const segment = segments[index];
    const start = optionalUnsignedInteger(field(segment, "start"), 4_294_967_295, node.id, `text.runs[${index}].start_utf16`, context);
    const end = optionalUnsignedInteger(field(segment, "end"), 4_294_967_295, node.id, `text.runs[${index}].end_utf16`, context);
    if (start === undefined || end === undefined || start < 0 || end < start || end > characters.length) {
      addDiagnostic(context, "ERROR", "FR-TEXT-EXTRACT-002", "Styled text range is outside the UTF-16 text bounds.", node.id, `text.runs[${index}]`);
      continue;
    }
    runs.push({
      start_utf16: start,
      end_utf16: end,
      style: extractTextStyle(segment, node.id, `text.runs[${index}].style`, context, extensions),
    });
  }

  return { characters, runs };
}

function extractTextStyle(
  segment: UnknownRecord,
  nodeId: string,
  propertyPath: string,
  context: ExtractionContext,
  extensions: RawExtensions,
): RawTextStyle {
  const fontName = field(segment, "fontName");
  const fontSize = optionalNumber(field(segment, "fontSize"), nodeId, `${propertyPath}.font_size`, context);
  const fontSizeToken = registerAliasFromRecord(segment, "fontSize", nodeId, `${propertyPath}.font_size`, context);
  registerBoundVariableAliases(
    field(segment, "boundVariables"),
    nodeId,
    `${propertyPath}.boundVariables`,
    new Set(["fontSize"]),
    context,
    extensions,
    "figma_text_bound_variables",
  );
  const lineHeight = textMetric(field(segment, "lineHeight"), fontSize, nodeId, `${propertyPath}.line_height`, context);
  const letterSpacing = textMetric(field(segment, "letterSpacing"), fontSize, nodeId, `${propertyPath}.letter_spacing`, context);
  const textColor = extractTextColor(field(segment, "fills"), nodeId, `${propertyPath}.color`, context);
  const style: RawTextStyle = {};

  if (isRecord(fontName)) {
    const family = stringValue(field(fontName, "family"));
    const fontStyle = stringValue(field(fontName, "style"));
    if (family !== undefined) style.font_family = family;
    if (fontStyle !== undefined) style.font_style = fontStyle;
  } else if (fontName !== undefined && !isMixed(fontName)) {
    addDiagnostic(context, "ERROR", "FR-TEXT-EXTRACT-003", "Text font name is not a Figma font object.", nodeId, `${propertyPath}.font_family`);
  }
  if (fontSize !== undefined) style.font_size = boundValue(fontSize, fontSizeToken, nodeId, context);
  const fontWeight = optionalUnsignedInteger(field(segment, "fontWeight"), 65_535, nodeId, `${propertyPath}.font_weight`, context);
  if (fontWeight !== undefined) style.font_weight = fontWeight;
  if (lineHeight !== undefined) style.line_height = lineHeight;
  if (letterSpacing !== undefined) style.letter_spacing = letterSpacing;
  if (textColor !== undefined) style.color = textColor;
  return style;
}

function extractTextColor(
  value: unknown,
  nodeId: string,
  propertyPath: string,
  context: ExtractionContext,
): RawBoundValue<RawColor> | undefined {
  if (value === undefined || value === null) return undefined;
  if (isMixed(value) || !Array.isArray(value)) {
    addDiagnostic(context, "ERROR", "FR-TEXT-COLOR-001", "Text fill color is mixed or not an array.", nodeId, propertyPath);
    return undefined;
  }
  let result: RawBoundValue<RawColor> | undefined;
  let visiblePaintCount = 0;
  for (let index = 0; index < value.length; index += 1) {
    const paint = value[index];
    if (!isRecord(paint)) continue;
    if (field(paint, "visible") === false) continue;
    visiblePaintCount += 1;
    const type = stringValue(field(paint, "type"));
    if (type !== "SOLID") {
      addDiagnostic(context, "WARNING", "FR-TEXT-COLOR-002", `Text paint ${type ?? "UNKNOWN"} cannot be represented as a text color.`, nodeId, `${propertyPath}[${index}]`);
      continue;
    }
    const opacity = optionalNumber(field(paint, "opacity"), nodeId, `${propertyPath}[${index}].opacity`, context) ?? 1;
    const color = extractColor(field(paint, "color"), opacity, nodeId, `${propertyPath}[${index}].color`, context);
    if (color === undefined) continue;
    const tokenId = registerPaintVariable(paint, "color", nodeId, `${propertyPath}[${index}].color`, context);
    if (result === undefined) result = boundValue(color, tokenId, nodeId, context);
  }
  if (visiblePaintCount > 1) {
    addDiagnostic(context, "WARNING", "FR-TEXT-COLOR-003", "Multiple visible text paints were reduced to the first solid color.", nodeId, propertyPath);
  }
  return result;
}

function textMetric(
  value: unknown,
  fontSize: number | undefined,
  nodeId: string,
  propertyPath: string,
  context: ExtractionContext,
): number | undefined {
  if (value === undefined || value === null || isMixed(value)) return undefined;
  if (!isRecord(value)) {
    addDiagnostic(context, "ERROR", "FR-TEXT-METRIC-001", "Text metric is not a Figma metric object.", nodeId, propertyPath);
    return undefined;
  }
  const unit = stringValue(field(value, "unit"));
  if (unit === "AUTO") return undefined;
  const metric = optionalNumber(field(value, "value"), nodeId, propertyPath, context);
  if (metric === undefined) return undefined;
  if (unit === "PIXELS") return metric;
  if (unit === "PERCENT") {
    if (fontSize === undefined) {
      addDiagnostic(context, "ERROR", "FR-TEXT-METRIC-002", "Percent text metric cannot be converted without a font size.", nodeId, propertyPath);
      return undefined;
    }
    return fontSize * metric / 100;
  }
  addDiagnostic(context, "ERROR", "FR-TEXT-METRIC-003", `Text metric unit ${unit ?? "UNKNOWN"} is not representable.`, nodeId, propertyPath);
  return undefined;
}

async function extractComponentMetadata(node: SceneNode, context: ExtractionContext): Promise<RawComponentMetadata | undefined> {
  const record = node as unknown as UnknownRecord;
  const type = stringValue(field(record, "type"));
  if (type === "COMPONENT_SET") {
    const key = stringValue(field(record, "key"));
    if (key !== undefined) registerComponentDefinition(record, key, undefined, context);
    return undefined;
  }
  if (type !== "COMPONENT" && type !== "INSTANCE") return undefined;

  if (type === "INSTANCE") {
    const instance = node as InstanceNode;
    let mainComponent: ComponentNode | null = null;
    try {
      mainComponent = await awaitWithOptionalDeadline(instance.getMainComponentAsync(), context.deadline);
    } catch (error) {
      if (isDeadlineError(error)) throw error;
      addDiagnostic(context, "ERROR", "FR-COMPONENT-MAIN-001", `Main component lookup failed: ${errorMessage(error)}`, node.id, "component.component_key");
    }
    if (mainComponent === null) {
      addDiagnostic(context, "ERROR", "FR-COMPONENT-MAIN-002", "Instance has no resolvable main component; component metadata was omitted.", node.id, "component.component_key");
      return undefined;
    }
    const mainRecord = mainComponent as unknown as UnknownRecord;
    const key = stringValue(field(mainRecord, "key"));
    if (key === undefined) {
      addDiagnostic(context, "ERROR", "FR-COMPONENT-MAIN-003", "Main component has no key.", node.id, "component.component_key");
      return undefined;
    }
    const setKey = componentSetKey(mainRecord);
    registerComponentDefinition(mainRecord, key, setKey, context);
    const parent = field(mainRecord, "parent");
    if (isRecord(parent) && stringValue(field(parent, "type")) === "COMPONENT_SET") {
      const parentKey = stringValue(field(parent, "key"));
      if (parentKey !== undefined) registerComponentDefinition(parent, parentKey, undefined, context);
    }
    return {
      role: "INSTANCE",
      component_key: key,
      ...(setKey === undefined ? {} : { component_set_key: setKey }),
      variants: stringMap(field(record, "variantProperties"), node.id, "component.variants", context),
      properties: extractComponentProperties(field(record, "componentProperties"), node.id, "component.properties", context),
      overrides: extractOverrides(field(record, "overrides"), node.id, context),
    };
  }

  const key = stringValue(field(record, "key"));
  if (key === undefined) {
    addDiagnostic(context, "ERROR", "FR-COMPONENT-EXTRACT-001", "Component has no key; component metadata was omitted.", node.id, "component.component_key");
    return undefined;
  }
  const setKey = componentSetKey(record);
  registerComponentDefinition(record, key, setKey, context);
  const definitions = field(record, "componentPropertyDefinitions");
  return {
    role: "COMPONENT",
    component_key: key,
    ...(setKey === undefined ? {} : { component_set_key: setKey }),
    variants: stringMap(field(record, "variantProperties"), node.id, "component.variants", context),
    properties: extractComponentProperties(definitions, node.id, "component.properties", context, true),
    overrides: [],
  };
}

function registerComponentDefinition(
  record: UnknownRecord,
  key: string,
  setKey: string | undefined,
  context: ExtractionContext,
): void {
  const definitions = field(record, "componentPropertyDefinitions");
  const propertyDefinitions: Record<string, string> = {};
  if (isRecord(definitions)) {
    for (const name of Object.keys(definitions).sort(compareStrings)) {
      const definition = definitions[name];
      const type = isRecord(definition) ? stringValue(field(definition, "type")) : undefined;
      if (type !== undefined) propertyDefinitions[name] = type;
    }
  }
  const name = stringValue(field(record, "name")) ?? key;
  const component: RawComponent = {
    key,
    name,
    ...(setKey === undefined ? {} : { set_key: setKey }),
    property_definitions: propertyDefinitions,
  };
  const previous = context.components.get(key);
  if (previous === undefined || componentSortKey(component) < componentSortKey(previous)) {
    context.components.set(key, component);
  }
}

function extractComponentProperties(
  value: unknown,
  nodeId: string,
  propertyPath: string,
  context: ExtractionContext,
  definitions = false,
): Record<string, RawComponentValue> {
  if (value === undefined || value === null) return {};
  if (!isRecord(value)) {
    addDiagnostic(context, "ERROR", "FR-COMPONENT-EXTRACT-002", "Component properties are not an object.", nodeId, propertyPath);
    return {};
  }
  const properties: Record<string, RawComponentValue> = {};
  for (const name of Object.keys(value).sort(compareStrings)) {
    const property = value[name];
    if (!isRecord(property)) {
      addDiagnostic(context, "ERROR", "FR-COMPONENT-EXTRACT-003", "Component property entry is not an object.", nodeId, `${propertyPath}.${name}`);
      continue;
    }
    const type = stringValue(field(property, "type"));
    const rawValue = field(property, definitions ? "defaultValue" : "value");
    const converted = componentValue(type, rawValue, nodeId, `${propertyPath}.${name}`, context);
    if (converted !== undefined) properties[name] = converted;
    const boundVariables = field(property, "boundVariables");
    if (hasAnyAlias(boundVariables)) {
      addDiagnostic(context, "ERROR", "FR-TOKEN-LOSS-002", "Component property variable bindings are not represented by the raw component model.", nodeId, `${propertyPath}.${name}.boundVariables`);
    }
  }
  return properties;
}

function componentValue(
  type: string | undefined,
  value: unknown,
  nodeId: string,
  propertyPath: string,
  context: ExtractionContext,
): RawComponentValue | undefined {
  if (type === "VARIANT" && typeof value === "string") return { kind: "VARIANT", value };
  if (type === "TEXT" && typeof value === "string") return { kind: "TEXT", value };
  if (type === "BOOLEAN" && typeof value === "boolean") return { kind: "BOOLEAN", value };
  if (type === "INSTANCE_SWAP" && typeof value === "string") return { kind: "INSTANCE_SWAP", value };
  if (type === "SLOT") {
    addDiagnostic(context, "ERROR", "FR-COMPONENT-EXTRACT-004", "SLOT component properties are not represented by the raw component model.", nodeId, propertyPath);
    return undefined;
  }
  addDiagnostic(context, "ERROR", "FR-COMPONENT-EXTRACT-005", `Component property type ${type ?? "UNKNOWN"} or value is not representable.`, nodeId, propertyPath);
  return undefined;
}

function extractOverrides(value: unknown, nodeId: string, context: ExtractionContext): RawComponentMetadata["overrides"] {
  if (value === undefined || value === null) return [];
  if (!Array.isArray(value)) {
    addDiagnostic(context, "ERROR", "FR-COMPONENT-EXTRACT-006", "Instance overrides are not an array.", nodeId, "component.overrides");
    return [];
  }
  const overrides = [];
  for (const entry of value) {
    if (!isRecord(entry)) continue;
    const childId = stringValue(field(entry, "id"));
    const fields = field(entry, "overriddenFields");
    if (childId === undefined || !Array.isArray(fields)) {
      addDiagnostic(context, "ERROR", "FR-COMPONENT-EXTRACT-007", "Instance override is missing an id or field list.", nodeId, "component.overrides");
      continue;
    }
    overrides.push({ node_id: childId, fields: fields.filter((fieldValue): fieldValue is string => typeof fieldValue === "string") });
  }
  return overrides.sort((left, right) => compareStrings(left.node_id, right.node_id));
}

function extractReactions(
  record: UnknownRecord,
  nodeId: string,
  context: ExtractionContext,
  extensions: RawExtensions,
): RawReaction[] {
  const value = field(record, "reactions");
  if (value === undefined || value === null) return [];
  if (!Array.isArray(value)) {
    addDiagnostic(context, "ERROR", "FR-REACTION-EXTRACT-001", "Node reactions are not an array.", nodeId, "reactions");
    return [];
  }
  const reactions: RawReaction[] = [];
  let lossy = false;
  for (let index = 0; index < value.length; index += 1) {
    const reaction = value[index];
    if (!isRecord(reaction)) {
      addDiagnostic(context, "ERROR", "FR-REACTION-EXTRACT-002", "Reaction entry is not an object.", nodeId, `reactions[${index}]`);
      lossy = true;
      continue;
    }
    const trigger = extractTrigger(field(reaction, "trigger"), nodeId, `reactions[${index}].trigger`, context);
    const actionValue = field(reaction, "actions");
    const legacyAction = field(reaction, "action");
    let actions: unknown[];
    if (actionValue === undefined) actions = legacyAction === undefined ? [] : [legacyAction];
    else if (Array.isArray(actionValue)) actions = actionValue;
    else {
      addDiagnostic(context, "ERROR", "FR-REACTION-EXTRACT-003", "Reaction actions are not an array.", nodeId, `reactions[${index}].actions`);
      lossy = true;
      continue;
    }
    if (actions.length > 1) {
      addDiagnostic(context, "WARNING", "FR-REACTION-EXTRACT-004", "Multiple Figma actions were reduced to the first raw action.", nodeId, `reactions[${index}].actions`);
      lossy = true;
    }
    const action = extractAction(actions[0], nodeId, `reactions[${index}].action`, context);
    if (trigger !== undefined && action !== undefined) reactions.push({ trigger, action });
    else lossy = true;
  }
  if (lossy) extensions.figma_reactions = toJson(value);
  return reactions;
}

function extractTrigger(value: unknown, nodeId: string, propertyPath: string, context: ExtractionContext): RawTrigger | undefined {
  if (!isRecord(value)) {
    addDiagnostic(context, "ERROR", "FR-REACTION-EXTRACT-005", "Reaction trigger is null or not an object.", nodeId, propertyPath);
    return undefined;
  }
  const type = stringValue(field(value, "type"));
  switch (type) {
    case "ON_CLICK":
      return { kind: "CLICK" };
    case "ON_HOVER":
      return { kind: "HOVER" };
    case "ON_PRESS":
      return { kind: "PRESS" };
    case "ON_KEY_DOWN": {
      const keyCodes = field(value, "keyCodes");
      if (!Array.isArray(keyCodes) || keyCodes.some((key) => typeof key !== "number" || !Number.isInteger(key))) {
        addDiagnostic(context, "ERROR", "FR-REACTION-EXTRACT-006", "ON_KEY_DOWN trigger has no integer key code list.", nodeId, propertyPath);
        return undefined;
      }
      addDiagnostic(context, "WARNING", "FR-REACTION-EXTRACT-007", "Key codes are encoded as a comma-separated raw key string because the raw model has no numeric key-code field.", nodeId, propertyPath);
      return { kind: "KEY", key: keyCodes.join(",") };
    }
    default:
      addDiagnostic(context, "ERROR", "FR-REACTION-EXTRACT-008", `Trigger type ${type ?? "UNKNOWN"} is not representable by the raw model.`, nodeId, propertyPath);
      return undefined;
  }
}

function extractAction(value: unknown, nodeId: string, propertyPath: string, context: ExtractionContext): RawAction | undefined {
  if (!isRecord(value)) {
    addDiagnostic(context, "ERROR", "FR-REACTION-EXTRACT-009", "Reaction action is missing or not an object.", nodeId, propertyPath);
    return undefined;
  }
  const type = stringValue(field(value, "type"));
  if (type !== "NODE") {
    addDiagnostic(context, "ERROR", "FR-REACTION-EXTRACT-010", `Action type ${type ?? "UNKNOWN"} is not representable by the raw model.`, nodeId, propertyPath);
    return undefined;
  }
  const destinationId = stringValue(field(value, "destinationId"));
  if (destinationId === undefined || destinationId.length === 0) {
    addDiagnostic(context, "ERROR", "FR-REACTION-EXTRACT-011", "Node action has no destination id.", nodeId, `${propertyPath}.destination_id`);
    return undefined;
  }
  const navigation = stringValue(field(value, "navigation"));
  if (navigation === "OVERLAY") return { kind: "OPEN_OVERLAY", destination_id: destinationId };
  if (navigation === "NAVIGATE") {
    const transition = field(value, "transition");
    if (isRecord(transition) && field(transition, "type") === "SMART_ANIMATE") {
      return { kind: "SMART_ANIMATE", destination_id: destinationId };
    }
    return { kind: "NAVIGATE", destination_id: destinationId };
  }
  addDiagnostic(context, "ERROR", "FR-REACTION-EXTRACT-012", `Navigation ${navigation ?? "UNKNOWN"} is not representable by the raw model.`, nodeId, `${propertyPath}.navigation`);
  return undefined;
}

function diagnoseNodeVariableBindings(record: UnknownRecord, node: SceneNode, context: ExtractionContext): void {
  const bindings = field(record, "boundVariables");
  if (!isRecord(bindings)) return;
  const supportedNodeFields = new Set([
    "fills",
    "strokes",
    "effects",
    "textRangeFills",
    "fontSize",
    "itemSpacing",
    "paddingLeft",
    "paddingRight",
    "paddingTop",
    "paddingBottom",
    "cornerRadius",
    "topLeftRadius",
    "topRightRadius",
    "bottomLeftRadius",
    "bottomRightRadius",
    "strokeWeight",
    "strokeTopWeight",
    "strokeRightWeight",
    "strokeBottomWeight",
    "strokeLeftWeight",
  ]);
  for (const [fieldName, value] of Object.entries(bindings)) {
    const aliases = collectAliasPaths(value, `bound_variables.${fieldName}`);
    for (const alias of aliases) {
      if (!hasReferenceForNode(context, node.id, alias.id)) {
        registerVariableReference(alias.id, node, alias.path, context);
      }
      if (!supportedNodeFields.has(fieldName)) {
        addDiagnostic(context, "ERROR", "FR-TOKEN-LOSS-003", `Variable binding for ${fieldName} is not represented by the raw model.`, node.id, alias.path);
      }
    }
  }
}

async function resolveVariables(context: ExtractionContext): Promise<RawVariable[]> {
  const variables: RawVariable[] = [];
  for (const id of [...context.variableReferences.keys()].sort(compareStrings)) {
    assertWithinDeadline(context.deadline);
    const references = [...(context.variableReferences.get(id) ?? [])].sort((left, right) => compareStrings(left.node.id, right.node.id) || compareStrings(left.propertyPath, right.propertyPath));
    if (references.length === 0) continue;
    let variable: Variable | null = null;
    try {
      variable = await awaitWithOptionalDeadline(figma.variables.getVariableByIdAsync(id), context.deadline);
    } catch (error) {
      if (isDeadlineError(error)) throw error;
      addDiagnostic(context, "ERROR", "FR-TOKEN-001", `Variable ${id} could not be resolved: ${errorMessage(error)}`);
    }
    if (variable === null) {
      addDiagnostic(context, "ERROR", "FR-TOKEN-001", `Variable ${id} could not be resolved.`);
      continue;
    }

    let collection: VariableCollection | null = null;
    try {
      collection = await awaitWithOptionalDeadline(figma.variables.getVariableCollectionByIdAsync(variable.variableCollectionId), context.deadline);
    } catch (error) {
      if (isDeadlineError(error)) throw error;
      addDiagnostic(context, "ERROR", "FR-TOKEN-MODE-002", `Variable collection ${variable.variableCollectionId} could not be resolved: ${errorMessage(error)}`, references[0].node.id, references[0].propertyPath);
    }
    if (collection === null) {
      addDiagnostic(context, "ERROR", "FR-TOKEN-MODE-002", `Variable collection ${variable.variableCollectionId} could not be resolved.`, references[0].node.id, references[0].propertyPath);
      continue;
    }
    const valuesByContext = new Map<string, {
      modeContext: RawModeContext;
      literal: RawLiteral;
      reference: VariableReference;
    }>();
    for (const reference of references) {
      const modeContext = extractVariableModes(reference.node);
      const literal = await resolveVariableLiteral(
        variable,
        reference.node,
        reference.node.id,
        reference.propertyPath,
        context,
        modeContext,
        collection,
      );
      if (literal === undefined) continue;
      const resolvedModeContext = canonicalModeContext(modeContext);
      const modeId = resolvedModeContext[variable.variableCollectionId];
      if (modeId === undefined) continue;
      assignBoundModeContext(context, id, reference.node.id, resolvedModeContext);
      const contextKey = JSON.stringify(resolvedModeContext);
      const previous = valuesByContext.get(contextKey);
      if (previous === undefined) {
        valuesByContext.set(contextKey, { modeContext: resolvedModeContext, literal, reference });
      } else if (JSON.stringify(previous.literal) !== JSON.stringify(literal)) {
        addDiagnostic(context, "ERROR", "FR-TOKEN-MODE-004", `Variable ${id} resolves to conflicting values for consumers in the same mode context.`, reference.node.id, reference.propertyPath);
      }
    }

    for (const contextKey of [...valuesByContext.keys()].sort(compareStrings)) {
      const resolved = valuesByContext.get(contextKey);
      if (resolved === undefined) continue;
      const modeId = resolved.modeContext[variable.variableCollectionId];
      if (modeId === undefined) continue;
      variables.push({
        id: variable.id,
        name: variable.name,
        collection_id: variable.variableCollectionId,
        mode_id: modeId,
        mode_context: resolved.modeContext,
        source_node_id: resolved.reference.node.id,
        value: resolved.literal,
      });
    }
  }
  return variables;
}

async function resolveVariableLiteral(
  variable: Variable,
  consumer: SceneNode,
  nodeId: string,
  propertyPath: string,
  context: ExtractionContext,
  modeContext: RawModeContext,
  collection: VariableCollection,
): Promise<RawLiteral | undefined> {
  assertWithinDeadline(context.deadline);
  const hasCompleteContext = await collectVariableModeContext(
    variable,
    nodeId,
    propertyPath,
    context,
    new Set(),
    modeContext,
    collection,
  );
  if (!hasCompleteContext) return undefined;
  let resolved: { value: VariableValue; resolvedType: VariableResolvedDataType };
  try {
    resolved = variable.resolveForConsumer(consumer);
  } catch (error) {
    addDiagnostic(context, "ERROR", "FR-TOKEN-CHAIN-002", `Variable ${variable.id} could not be resolved for the selected node: ${errorMessage(error)}`, nodeId, propertyPath);
    return undefined;
  }
  return rawLiteral(resolved.value, resolved.resolvedType, nodeId, propertyPath, context);
}

async function collectVariableModeContext(
  variable: Variable,
  nodeId: string,
  propertyPath: string,
  context: ExtractionContext,
  seen: Set<string>,
  modeContext: RawModeContext,
  collection?: VariableCollection,
): Promise<boolean> {
  assertWithinDeadline(context.deadline);
  if (seen.has(variable.id)) {
    addDiagnostic(context, "ERROR", "FR-TOKEN-CHAIN-001", `Variable alias cycle detected at ${variable.id}.`, nodeId, propertyPath);
    return false;
  }
  seen.add(variable.id);
  let resolvedCollection = collection;
  if (resolvedCollection === undefined) {
    try {
      resolvedCollection = await awaitWithOptionalDeadline(
        figma.variables.getVariableCollectionByIdAsync(variable.variableCollectionId),
        context.deadline,
      ) ?? undefined;
    } catch (error) {
      if (isDeadlineError(error)) throw error;
      addDiagnostic(context, "ERROR", "FR-TOKEN-MODE-002", `Variable collection ${variable.variableCollectionId} could not be resolved: ${errorMessage(error)}`, nodeId, propertyPath);
      return false;
    }
  }
  if (resolvedCollection === undefined) {
    addDiagnostic(context, "ERROR", "FR-TOKEN-MODE-002", `Variable collection ${variable.variableCollectionId} could not be resolved.`, nodeId, propertyPath);
    return false;
  }
  const modeId = modeContext[variable.variableCollectionId] ?? resolvedCollection.defaultModeId;
  if (!resolvedCollection.modes.some((mode) => mode.modeId === modeId)) {
    addDiagnostic(context, "ERROR", "FR-TOKEN-MODE-003", `No resolved mode is available for variable ${variable.id}.`, nodeId, propertyPath);
    return false;
  }
  modeContext[variable.variableCollectionId] = modeId;
  const unresolvedValue = variable.valuesByMode[modeId];
  if (unresolvedValue === undefined) {
    addDiagnostic(context, "ERROR", "FR-TOKEN-MODE-003", `Variable ${variable.id} has no value for resolved mode ${modeId}.`, nodeId, propertyPath);
    return false;
  }
  const alias = variableAlias(unresolvedValue);
  if (alias === undefined) return true;

  let target: Variable | null = null;
  try {
    target = await awaitWithOptionalDeadline(figma.variables.getVariableByIdAsync(alias), context.deadline);
  } catch (error) {
    if (isDeadlineError(error)) throw error;
    addDiagnostic(context, "ERROR", "FR-TOKEN-CHAIN-003", `Aliased variable ${alias} could not be loaded: ${errorMessage(error)}`, nodeId, propertyPath);
  }
  if (target === null) {
    addDiagnostic(context, "ERROR", "FR-TOKEN-CHAIN-003", `Aliased variable ${alias} could not be loaded.`, nodeId, propertyPath);
    return false;
  }
  return collectVariableModeContext(
    target,
    nodeId,
    propertyPath,
    context,
    seen,
    modeContext,
  );
}

function rawLiteral(
  value: VariableValue,
  resolvedType: VariableResolvedDataType,
  nodeId: string,
  propertyPath: string,
  context: ExtractionContext,
): RawLiteral | undefined {
  switch (resolvedType) {
    case "BOOLEAN":
      if (typeof value === "boolean") return { kind: "BOOLEAN", value };
      break;
    case "FLOAT":
      if (typeof value === "number" && Number.isFinite(value)) return { kind: "NUMBER", value };
      break;
    case "STRING":
      if (typeof value === "string") return { kind: "STRING", value };
      break;
    case "COLOR": {
      const color = colorValue(value);
      if (color !== undefined) return { kind: "COLOR", value: color };
      break;
    }
    default:
      addDiagnostic(context, "ERROR", "FR-TOKEN-VALUE-001", `Variable type ${resolvedType} is not representable by RawLiteral.`, nodeId, propertyPath);
      return undefined;
  }
  addDiagnostic(context, "ERROR", "FR-TOKEN-VALUE-002", `Variable value does not match resolved type ${resolvedType}.`, nodeId, propertyPath);
  return undefined;
}

function extractGridTracks(value: unknown, nodeId: string, propertyPath: string, context: ExtractionContext): RawGridTrack[] {
  if (value === undefined || value === null) return [];
  if (!Array.isArray(value)) {
    addDiagnostic(context, "ERROR", "FR-GRID-EXTRACT-003", "Grid track collection is not an array.", nodeId, propertyPath);
    return [];
  }
  const tracks: RawGridTrack[] = [];
  for (let index = 0; index < value.length; index += 1) {
    const track = value[index];
    if (!isRecord(track)) {
      addDiagnostic(context, "ERROR", "FR-GRID-EXTRACT-004", "Grid track is not an object.", nodeId, `${propertyPath}[${index}]`);
      continue;
    }
    const type = stringValue(field(track, "type"));
    if (type !== "FIXED" && type !== "FLEX") {
      addDiagnostic(context, "ERROR", "FR-GRID-EXTRACT-005", `Grid track type ${type ?? "UNKNOWN"} is not provided by the supported Figma typings.`, nodeId, `${propertyPath}[${index}]`);
      continue;
    }
    const trackValue = numberValue(field(track, "value"), 0, nodeId, `${propertyPath}[${index}].value`, context);
    tracks.push(type === "FIXED" ? { kind: "FIXED", value: trackValue } : { kind: "FLEX", value: trackValue });
  }
  return tracks;
}

function extractColor(value: unknown, opacity: number, nodeId: string, propertyPath: string, context: ExtractionContext): RawColor | undefined {
  if (!isRecord(value)) {
    addDiagnostic(context, "ERROR", "FR-COLOR-EXTRACT-001", "Color is not an RGB/RGBA object.", nodeId, propertyPath);
    return undefined;
  }
  const r = optionalNumber(field(value, "r"), nodeId, `${propertyPath}.r`, context);
  const g = optionalNumber(field(value, "g"), nodeId, `${propertyPath}.g`, context);
  const b = optionalNumber(field(value, "b"), nodeId, `${propertyPath}.b`, context);
  const a = optionalNumber(field(value, "a"), nodeId, `${propertyPath}.a`, context) ?? 1;
  if (r === undefined || g === undefined || b === undefined || !Number.isFinite(opacity)) {
    addDiagnostic(context, "ERROR", "FR-COLOR-EXTRACT-002", "Color contains an invalid channel.", nodeId, propertyPath);
    return undefined;
  }
  return { r, g, b, a: a * opacity };
}

function registerImageAsset(imageHash: string, nodeId: string, propertyPath: string, context: ExtractionContext): void {
  if (context.assets.has(imageHash) || context.assetChecks.has(imageHash)) return;
  if (context.assetChecks.size >= MAX_IMAGE_ASSETS) {
    addDiagnostic(
      context,
      "ERROR",
      "FR-ASSET-LIMIT-001",
      `Image asset resolution stopped after ${MAX_IMAGE_ASSETS} unique assets.`,
      nodeId,
      propertyPath,
    );
    return;
  }

  let image: Image | null = null;
  try {
    image = figma.getImageByHash(imageHash);
  } catch (error) {
    addDiagnostic(context, "ERROR", "FR-ASSET-EXTRACT-002", `Image asset lookup failed: ${errorMessage(error)}`, nodeId, propertyPath);
    return;
  }
  if (image === null) {
    addDiagnostic(context, "ERROR", "FR-ASSET-EXTRACT-003", `Image asset ${imageHash} is not available in the file; no asset was added.`, nodeId, propertyPath);
    return;
  }

  const asset = image;
  const check = asset.getBytesAsync().then(
    (bytes) => {
      if (bytes.byteLength === 0) {
        addDiagnostic(context, "ERROR", "FR-ASSET-EXTRACT-004", `Image asset ${imageHash} returned no bytes; no asset was added.`, nodeId, propertyPath);
        return;
      }
      context.assets.set(imageHash, {
        id: imageHash,
        source_node_id: nodeId,
        media_type: detectImageMediaType(bytes),
        content_hash: asset.hash,
        export_settings: {},
        ...(context.includeAssetPayloads
          ? { payload_base64: encodeAssetPayload(bytes, nodeId, propertyPath, context) }
          : {}),
      });
    },
    (error) => {
      addDiagnostic(context, "ERROR", "FR-ASSET-EXTRACT-005", `Image asset ${imageHash} bytes could not be loaded: ${errorMessage(error)}; no asset was added.`, nodeId, propertyPath);
    },
  );
  context.assetChecks.set(imageHash, check);
}

function encodeAssetPayload(
  bytes: Uint8Array,
  nodeId: string,
  propertyPath: string,
  context: ExtractionContext,
): string | undefined {
  try {
    return figma.base64Encode(bytes);
  } catch (error) {
    addDiagnostic(
      context,
      "ERROR",
      "FR-ASSET-EXTRACT-008",
      `Asset bytes could not be base64 encoded: ${errorMessage(error)}; payload was omitted.`,
      nodeId,
      propertyPath,
    );
    return undefined;
  }
}

function detectImageMediaType(bytes: Uint8Array): string {
  if (bytes.length >= 8
    && bytes[0] === 0x89
    && bytes[1] === 0x50
    && bytes[2] === 0x4e
    && bytes[3] === 0x47) return "image/png";
  if (bytes.length >= 3 && bytes[0] === 0xff && bytes[1] === 0xd8 && bytes[2] === 0xff) {
    return "image/jpeg";
  }
  if (bytes.length >= 6) {
    const signature = String.fromCharCode(...bytes.slice(0, 6));
    if (signature === "GIF87a" || signature === "GIF89a") return "image/gif";
  }
  if (bytes.length >= 12) {
    const riff = String.fromCharCode(...bytes.slice(0, 4));
    const webp = String.fromCharCode(...bytes.slice(8, 12));
    if (riff === "RIFF" && webp === "WEBP") return "image/webp";
  }
  return "application/octet-stream";
}

function boundValue<T>(
  literal: T,
  tokenId: string | undefined,
  nodeId: string,
  context: ExtractionContext,
): RawBoundValue<T> {
  if (tokenId === undefined) return { literal };
  const value: RawBoundValue<T> = {
    literal,
    token_id: tokenId,
  };
  const key = boundValueKey(nodeId, tokenId);
  const values = context.boundValues.get(key) ?? [];
  values.push(value);
  context.boundValues.set(key, values);
  return value;
}

function boundNumberFromRecord(
  record: UnknownRecord,
  fieldName: string,
  literal: number,
  nodeId: string,
  propertyPath: string,
  context: ExtractionContext,
  fallbackTokenId?: string,
): RawBoundValue<number> {
  const tokenId = registerAliasFromRecord(record, fieldName, nodeId, propertyPath, context)
    ?? fallbackTokenId;
  return boundValue(literal, tokenId, nodeId, context);
}

function registerPaintVariable(record: UnknownRecord, fieldName: string, nodeId: string, propertyPath: string, context: ExtractionContext): string | undefined {
  return registerAliasFromRecord(record, fieldName, nodeId, propertyPath, context);
}

function registerAliasFromRecord(record: UnknownRecord, fieldName: string, nodeId: string, propertyPath: string, context: ExtractionContext): string | undefined {
  const alias = aliasIdFromRecord(field(record, "boundVariables"), fieldName);
  if (alias === undefined) return undefined;
  const node = context.nodes.get(nodeId);
  if (node !== undefined) registerVariableReference(alias, node, propertyPath, context);
  return alias;
}

function registerBoundVariableAliases(
  value: unknown,
  nodeId: string,
  propertyPath: string,
  supportedFields: ReadonlySet<string>,
  context: ExtractionContext,
  extensions: RawExtensions,
  extensionKey: string,
): void {
  if (!isRecord(value)) return;
  const node = context.nodes.get(nodeId);
  if (node === undefined) return;

  let hasAlias = false;
  for (const [fieldName, fieldValue] of Object.entries(value)) {
    for (const alias of collectAliasPaths(fieldValue, `${propertyPath}.${fieldName}`)) {
      hasAlias = true;
      registerVariableReference(alias.id, node, alias.path, context);
      if (!supportedFields.has(fieldName)) {
        addDiagnostic(
          context,
          "ERROR",
          "FR-TOKEN-LOSS-004",
          `Variable binding for ${fieldName} is not represented by the raw model.`,
          nodeId,
          alias.path,
        );
      }
    }
  }
  if (hasAlias) appendExtension(extensions, extensionKey, toJson(value));
}

function registerVariableReference(id: string, node: SceneNode, propertyPath: string, context: ExtractionContext): void {
  const references = context.variableReferences.get(id) ?? [];
  if (!references.some((reference) => reference.node.id === node.id && reference.propertyPath === propertyPath)) {
    references.push({ id, node, propertyPath });
    context.variableReferences.set(id, references);
  }
}

function hasReferenceForNode(context: ExtractionContext, nodeId: string, variableId: string): boolean {
  return [...context.variableReferences.values()].some((references) => references.some((reference) => reference.node.id === nodeId && reference.id === variableId));
}

function collectAliasPaths(value: unknown, propertyPath: string, seen = new Set<object>()): Array<{ id: string; path: string }> {
  const alias = variableAlias(value);
  if (alias !== undefined) return [{ id: alias, path: propertyPath }];
  if (Array.isArray(value)) return value.flatMap((child, index) => collectAliasPaths(child, `${propertyPath}[${index}]`, seen));
  if (!isRecord(value) || seen.has(value)) return [];
  seen.add(value);
  const result = Object.entries(value).flatMap(([key, child]) => collectAliasPaths(child, `${propertyPath}.${key}`, seen));
  seen.delete(value);
  return result;
}

function hasAnyAlias(value: unknown): boolean {
  return collectAliasPaths(value, "value").length > 0;
}

function hasAlias(value: unknown, fieldName: string): boolean {
  return aliasIdFromRecord(value, fieldName) !== undefined;
}

function aliasIdFromRecord(value: unknown, fieldName: string): string | undefined {
  return isRecord(value) ? variableAlias(field(value, fieldName)) : undefined;
}

function variableAlias(value: unknown): string | undefined {
  return isRecord(value) && field(value, "type") === "VARIABLE_ALIAS" && typeof field(value, "id") === "string"
    ? field(value, "id") as string
    : undefined;
}

function colorValue(value: unknown): RawColor | undefined {
  if (!isRecord(value)) return undefined;
  const r = field(value, "r");
  const g = field(value, "g");
  const b = field(value, "b");
  const a = field(value, "a");
  if ([r, g, b].some((channel) => typeof channel !== "number" || !Number.isFinite(channel))) return undefined;
  return { r: r as number, g: g as number, b: b as number, a: typeof a === "number" && Number.isFinite(a) ? a : 1 };
}

function boundValueKey(nodeId: string, tokenId: string): string {
  return `${nodeId}\0${tokenId}`;
}

function assignBoundModeContext(
  context: ExtractionContext,
  tokenId: string,
  nodeId: string,
  modeContext: RawModeContext,
): void {
  for (const value of context.boundValues.get(boundValueKey(nodeId, tokenId)) ?? []) {
    value.mode_context = { ...modeContext };
  }
}

function canonicalModeContext(modeContext: RawModeContext): RawModeContext {
  return Object.fromEntries(
    Object.keys(modeContext)
      .sort(compareStrings)
      .map((collectionId) => [collectionId, modeContext[collectionId]]),
  );
}

function extractVariableModes(node: SceneNode): RawModeContext {
  const modes = node.resolvedVariableModes ?? {};
  return Object.fromEntries(
    Object.keys(modes)
      .sort(compareStrings)
      .flatMap((collectionId) => typeof modes[collectionId] === "string" ? [[collectionId, modes[collectionId]]] : []),
  );
}

function componentSetKey(record: UnknownRecord): string | undefined {
  const parent = field(record, "parent");
  return isRecord(parent) && field(parent, "type") === "COMPONENT_SET" ? stringValue(field(parent, "key")) : undefined;
}

function stringMap(value: unknown, nodeId: string, propertyPath: string, context: ExtractionContext): Record<string, string> {
  if (value === undefined || value === null) return {};
  if (!isRecord(value)) {
    addDiagnostic(context, "ERROR", "FR-COMPONENT-EXTRACT-008", "Component variants are not a string map.", nodeId, propertyPath);
    return {};
  }
  const result: Record<string, string> = {};
  for (const key of Object.keys(value).sort(compareStrings)) {
    const item = value[key];
    if (typeof item === "string") result[key] = item;
    else addDiagnostic(context, "ERROR", "FR-COMPONENT-EXTRACT-009", "Component variant value is not a string.", nodeId, `${propertyPath}.${key}`);
  }
  return result;
}

function mapAlignment(value: unknown, nodeId: string, propertyPath: string, context: ExtractionContext): RawAlignment {
  switch (value) {
    case "MIN":
      return "START";
    case "CENTER":
      return "CENTER";
    case "MAX":
      return "END";
    case "SPACE_BETWEEN":
      return "SPACE_BETWEEN";
    case "BASELINE":
      return "BASELINE";
    case "STRETCH":
      return "STRETCH";
    case undefined:
      return "START";
    default:
      addDiagnostic(context, "ERROR", "FR-LAYOUT-EXTRACT-001", `Alignment ${String(value)} is not representable.`, nodeId, propertyPath);
      return "START";
  }
}

function mapBlendMode(value: unknown, nodeId: string, context: ExtractionContext, extensions: RawExtensions): RawBlendMode {
  switch (value) {
    case undefined:
    case "NORMAL":
      return "NORMAL";
    case "PASS_THROUGH":
      return "PASS_THROUGH";
    case "MULTIPLY":
      return "MULTIPLY";
    case "SCREEN":
      return "SCREEN";
    case "OVERLAY":
      return "OVERLAY";
    default:
      addLossDiagnostic(context, nodeId, "style.blend_mode", `Blend mode ${String(value)} is reduced to OTHER by the raw model.`, extensions, "figma_blend_mode", toJson(value));
      return "OTHER";
  }
}

function gradientKindFor(type: string): RawGradientKind {
  switch (type) {
    case "GRADIENT_LINEAR":
      return "LINEAR";
    case "GRADIENT_RADIAL":
      return "RADIAL";
    case "GRADIENT_ANGULAR":
      return "ANGULAR";
    default:
      return "DIAMOND";
  }
}

function nodeKind(type: string): RawNodeKind | undefined {
  switch (type) {
    case "FRAME":
      return "FRAME";
    case "GROUP":
      return "GROUP";
    case "RECTANGLE":
      return "RECTANGLE";
    case "ELLIPSE":
      return "ELLIPSE";
    case "TEXT":
      return "TEXT";
    case "VECTOR":
      return "VECTOR";
    case "IMAGE":
      return "IMAGE";
    case "COMPONENT":
      return "COMPONENT";
    case "INSTANCE":
      return "INSTANCE";
    case "SCROLL":
      return "SCROLL";
    default:
      return undefined;
  }
}

function axisSizing(value: unknown, nodeId: string, propertyPath: string, context: ExtractionContext): RawAxisSizing | undefined {
  if (value === undefined || value === null) return undefined;
  return enumValue<RawAxisSizing>(value, ["HUG", "FILL", "FIXED"], "FIXED", nodeId, propertyPath, context);
}

function enumValue<T extends string>(
  value: unknown,
  allowed: readonly T[],
  fallback: T,
  nodeId: string,
  propertyPath: string,
  context: ExtractionContext,
): T {
  if (value === undefined || value === null) return fallback;
  if (typeof value === "string" && allowed.includes(value as T)) return value as T;
  addDiagnostic(context, "ERROR", "FR-VALUE-EXTRACT-001", `Value ${String(value)} is not one of the supported enum values.`, nodeId, propertyPath);
  return fallback;
}

function booleanValue(value: unknown, fallback: boolean, nodeId: string, propertyPath: string, context: ExtractionContext): boolean {
  if (value === undefined || value === null) return fallback;
  if (typeof value === "boolean") return value;
  addDiagnostic(context, "ERROR", "FR-VALUE-EXTRACT-002", `Value ${String(value)} is not boolean.`, nodeId, propertyPath);
  return fallback;
}

function numberValue(value: unknown, fallback: number, nodeId: string, propertyPath: string, context: ExtractionContext): number {
  return optionalNumber(value, nodeId, propertyPath, context) ?? fallback;
}

function optionalNumber(value: unknown, nodeId: string, propertyPath: string, context: ExtractionContext): number | undefined {
  if (value === undefined || value === null) return undefined;
  if (typeof value === "number" && Number.isFinite(value)) return value;
  addDiagnostic(context, "ERROR", "FR-VALUE-EXTRACT-003", `Value ${String(value)} is not a finite number.`, nodeId, propertyPath);
  return undefined;
}

function optionalInteger(value: unknown, nodeId: string, propertyPath: string, context: ExtractionContext): number | undefined {
  const number = optionalNumber(value, nodeId, propertyPath, context);
  if (number === undefined) return undefined;
  if (Number.isInteger(number)) return number;
  addDiagnostic(context, "ERROR", "FR-VALUE-EXTRACT-004", `Value ${number} is not an integer.`, nodeId, propertyPath);
  return undefined;
}

function optionalUnsignedInteger(
  value: unknown,
  maximum: number,
  nodeId: string,
  propertyPath: string,
  context: ExtractionContext,
): number | undefined {
  const integer = optionalInteger(value, nodeId, propertyPath, context);
  if (integer === undefined) return undefined;
  if (integer >= 0 && integer <= maximum) return integer;
  addDiagnostic(context, "ERROR", "FR-VALUE-EXTRACT-005", `Value ${integer} is outside the supported unsigned integer range.`, nodeId, propertyPath);
  return undefined;
}

function addLossDiagnostic(
  context: ExtractionContext,
  nodeId: string,
  propertyPath: string,
  message: string,
  extensions: RawExtensions,
  extensionKey: string,
  sourceValue: JsonValue,
  diagnose = true,
): void {
  if (diagnose) addDiagnostic(context, "WARNING", "FR-EXTRACT-LOSS-001", message, nodeId, propertyPath);
  appendExtension(extensions, extensionKey, sourceValue);
}

function addDiagnostic(
  context: ExtractionContext,
  severity: ExtractionDiagnostic["severity"],
  code: string,
  message: string,
  nodeId?: string,
  propertyPath?: string,
): void {
  context.diagnostics.push({
    severity,
    code,
    message,
    ...(nodeId === undefined ? {} : { node_id: nodeId }),
    ...(propertyPath === undefined ? {} : { property_path: propertyPath }),
  });
}

function appendExtension(extensions: RawExtensions, key: string, value: JsonValue): void {
  const previous = extensions[key];
  if (previous === undefined) {
    extensions[key] = [value];
  } else if (Array.isArray(previous)) {
    previous.push(value);
  } else {
    extensions[key] = [previous, value];
  }
}

function field(record: UnknownRecord, key: string): unknown {
  try {
    return record[key];
  } catch {
    return undefined;
  }
}

function isRecord(value: unknown): value is UnknownRecord {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function stringValue(value: unknown): string | undefined {
  return typeof value === "string" ? value : undefined;
}

function isMixed(value: unknown): boolean {
  return typeof figma !== "undefined" && value === figma.mixed;
}

function toJson(value: unknown, seen = new Set<object>()): JsonValue {
  if (value === undefined || value === null) return null;
  if (isMixed(value)) return { mixed: true };
  if (typeof value === "string" || typeof value === "boolean") return value;
  if (typeof value === "number") return Number.isFinite(value) ? value : String(value);
  if (typeof value === "symbol" || typeof value === "function" || typeof value === "bigint") return String(value);
  if (Array.isArray(value)) return value.map((item) => toJson(item, seen));
  if (typeof value === "object") {
    if (seen.has(value)) return "[circular]";
    seen.add(value);
    const output: Record<string, JsonValue> = {};
    for (const [key, child] of Object.entries(value)) output[key] = toJson(child, seen);
    seen.delete(value);
    return output;
  }
  return String(value);
}

function awaitWithOptionalDeadline<T>(operation: Promise<T>, deadline?: ExtractionDeadline): Promise<T> {
  return deadline === undefined ? operation : withExtractionDeadline(operation, deadline);
}

function assertWithinDeadline(deadline?: ExtractionDeadline): void {
  if (deadline !== undefined && deadline.expiresAt <= Date.now()) throw deadlineError();
}

function deadlineError(): Error {
  return new Error("figma-rust extraction and compiler deadline exceeded");
}

function isDeadlineError(error: unknown): boolean {
  return error instanceof Error && error.message === "figma-rust extraction and compiler deadline exceeded";
}

export function isExtractionDeadlineError(error: unknown): boolean {
  return isDeadlineError(error);
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

function compareStrings(left: string, right: string): number {
  return left < right ? -1 : left > right ? 1 : 0;
}

function componentSortKey(component: RawComponent): string {
  return `${component.name}\u0000${component.set_key ?? ""}\u0000${JSON.stringify(component.property_definitions)}`;
}

function compareComponents(left: RawComponent, right: RawComponent): number {
  return compareStrings(left.key, right.key) || compareStrings(componentSortKey(left), componentSortKey(right));
}

function compareAssets(left: RawAsset, right: RawAsset): number {
  return compareStrings(left.id, right.id) || compareStrings(left.source_node_id, right.source_node_id);
}

function compareDiagnostics(left: ExtractionDiagnostic, right: ExtractionDiagnostic): number {
  return compareStrings(left.node_id ?? "", right.node_id ?? "")
    || compareStrings(left.property_path ?? "", right.property_path ?? "")
    || compareStrings(left.code, right.code)
    || compareStrings(left.severity, right.severity)
    || compareStrings(left.message, right.message);
}
