import assert from "node:assert/strict";

import {
  createExtractionDeadline,
  extractNodes,
  fallbackExportFormat,
} from "../src/extract";
import {
  BRIDGE_MAX_REQUEST_BYTES,
  LARGE_BUNDLE_BYTES,
  classifyBundleBytes,
  includeRestSnapshotForExport,
  summarizeBundle,
  validateBridgeExportResponse,
} from "../src/export";
import {
  cleanupTemporaryTransferNodes,
  findTemporaryTransferNodes,
  isTemporaryTransferNodeName,
} from "../src/transfer";
import type { ExtractionBundle } from "../src/schema";
import realGroupFixture from "./fixtures/real-group.plugin-api.json";
import multiModeFixture from "./fixtures/multi-mode-variables.json";
import realExtractionFixture from "../../fixtures/real-figma/extraction.json";
import dataTableExtractionFixture from "../../fixtures/orbitline-figma-mcp/extraction.table.json";

type Transform = [[number, number, number], [number, number, number]];

interface TestNode {
  id: string;
  name: string;
  type: "GROUP" | "RECTANGLE" | "VECTOR";
  x: number;
  y: number;
  width: number;
  height: number;
  relativeTransform: Transform;
  gridRowAnchorIndex?: number;
  gridColumnAnchorIndex?: number;
  gridRowSpan?: number;
  gridColumnSpan?: number;
  resolvedVariableModes?: Record<string, string>;
  layoutAlign?: "MIN" | "CENTER" | "MAX" | "STRETCH" | "INHERIT";
  fills?: unknown[];
  children?: TestNode[];
  exportAsync?: (settings: { format: "SVG" | "PNG" }) => Promise<Uint8Array>;
}

const testVariables = new Map<string, unknown>();
const testCollections = new Map<string, unknown>();

Object.assign(globalThis, {
  figma: {
    fileKey: realGroupFixture.file_key,
    currentPage: { id: "0:1", selection: [] },
    mixed: Symbol("mixed"),
    base64Encode: (bytes: Uint8Array) => Buffer.from(bytes).toString("base64"),
    variables: {
      getVariableByIdAsync: async (id: string) => testVariables.get(id) ?? null,
      getVariableCollectionByIdAsync: async (id: string) => testCollections.get(id) ?? null,
    },
  },
});

function rectangle(id: string, transform: Transform): TestNode {
  return {
    id,
    name: id,
    type: "RECTANGLE",
    x: transform[0][2],
    y: transform[1][2],
    width: 20,
    height: 10,
    relativeTransform: transform,
  };
}

async function extractsGroupLocalCoordinates(): Promise<void> {
  const bundle = await extractNodes([
    realGroupFixture.group as unknown as SceneNode,
  ]);
  assert.equal(bundle.source.file_key, realGroupFixture.file_key);
  const positions = bundle.roots[0].children.map((child) => [
    child.id,
    child.position.x,
    child.position.y,
  ]);
  assert.deepEqual(positions, [["1:2", 0, 0], ["1:3", 70, 40]]);
  assert.deepEqual(
    positions,
    realExtractionFixture.roots[0].children.map((child) => [
      child.id,
      child.position.x,
      child.position.y,
    ]),
  );
  assert.deepEqual(bundle.extraction_diagnostics, []);
  for (const child of bundle.roots[0].children) {
    assert.deepEqual(child.position.transform.matrix, [1, 0, 0, 1, 0, 0]);
  }
}

async function extractsNestedGroupLocalCoordinates(): Promise<void> {
  const child = rectangle("3:3", [[1, 0, 150], [0, 1, 260]]);
  const inner: TestNode = {
    id: "3:2",
    name: "Inner group",
    type: "GROUP",
    x: 130,
    y: 240,
    width: 40,
    height: 40,
    relativeTransform: [[1, 0, 130], [0, 1, 240]],
    children: [child],
  };
  const outer: TestNode = {
    id: "3:1",
    name: "Outer group",
    type: "GROUP",
    x: 100,
    y: 200,
    width: 80,
    height: 80,
    relativeTransform: [[1, 0, 100], [0, 1, 200]],
    children: [inner],
  };

  const bundle = await extractNodes([outer as unknown as SceneNode]);
  const extractedInner = bundle.roots[0].children[0];
  assert.equal(extractedInner.position.x, 30);
  assert.equal(extractedInner.position.y, 40);
  assert.equal(extractedInner.children[0].position.x, 20);
  assert.equal(extractedInner.children[0].position.y, 20);
  assert.deepEqual(bundle.extraction_diagnostics, []);
}

async function extractsRotatedGroupLocalCoordinates(): Promise<void> {
  const angle = Math.PI / 6;
  const cosine = Math.cos(angle);
  const sine = Math.sin(angle);
  const child = rectangle("2:2", [
    [cosine, -sine, 100 + cosine * 10 - sine * 20],
    [sine, cosine, 200 + sine * 10 + cosine * 20],
  ]);
  const group: TestNode = {
    id: "2:1",
    name: "Rotated group",
    type: "GROUP",
    x: 100,
    y: 200,
    width: 50,
    height: 50,
    relativeTransform: [[cosine, -sine, 100], [sine, cosine, 200]],
    children: [child],
  };

  const bundle = await extractNodes([group as unknown as SceneNode]);
  const position = bundle.roots[0].children[0].position;
  assert.deepEqual(position.transform.matrix, [1, 0, 0, 1, 0, 0]);
  assert.ok(Math.abs(position.x - 10) <= 1e-12);
  assert.ok(Math.abs(position.y - 20) <= 1e-12);
  assert.deepEqual(bundle.extraction_diagnostics, []);
}

async function rejectsIllConditionedGroupTransform(): Promise<void> {
  const child = rectangle("4:2", [[1, 0, 10], [0, 1, 20]]);
  const group: TestNode = {
    id: "4:1",
    name: "Ill-conditioned group",
    type: "GROUP",
    x: 0,
    y: 0,
    width: 40,
    height: 40,
    relativeTransform: [[1, 1, 0], [0, 1e-13, 0]],
    children: [child],
  };

  const bundle = await extractNodes([group as unknown as SceneNode]);
  assert.ok(
    bundle.extraction_diagnostics.some(
      (diagnostic) => diagnostic.code === "FR-POSITION-EXTRACT-002",
    ),
  );
  assert.ok(
    bundle.roots[0].children[0].position.transform.matrix.every(Number.isFinite),
  );
}

async function ignoresNonGridPlacementSentinels(): Promise<void> {
  const node = rectangle("5:1", [[1, 0, 0], [0, 1, 0]]);
  node.gridRowAnchorIndex = -1;
  node.gridColumnAnchorIndex = -1;
  node.gridRowSpan = 1;
  node.gridColumnSpan = 1;

  const bundle = await extractNodes([node as unknown as SceneNode]);
  assert.equal(bundle.roots[0].position.grid, undefined);
  assert.deepEqual(bundle.extraction_diagnostics, []);
}

async function preservesAndValidatesGridPlacements(): Promise<void> {
  const valid = rectangle("6:1", [[1, 0, 0], [0, 1, 0]]);
  valid.gridRowAnchorIndex = 0;
  valid.gridColumnAnchorIndex = 1;
  valid.gridRowSpan = 2;
  valid.gridColumnSpan = 3;

  const missingAnchor = rectangle("6:2", [[1, 0, 0], [0, 1, 0]]);
  missingAnchor.gridRowAnchorIndex = 0;
  missingAnchor.gridColumnAnchorIndex = -1;
  missingAnchor.gridRowSpan = 1;
  missingAnchor.gridColumnSpan = 1;

  const zeroSpan = rectangle("6:3", [[1, 0, 0], [0, 1, 0]]);
  zeroSpan.gridRowAnchorIndex = 0;
  zeroSpan.gridColumnAnchorIndex = 0;
  zeroSpan.gridRowSpan = 0;
  zeroSpan.gridColumnSpan = 1;

  const bundle = await extractNodes([
    valid as unknown as SceneNode,
    missingAnchor as unknown as SceneNode,
    zeroSpan as unknown as SceneNode,
  ]);
  assert.deepEqual(bundle.roots[0].position.grid, {
    row: 0,
    column: 1,
    row_span: 2,
    column_span: 3,
  });
  assert.equal(bundle.roots[1].position.grid, undefined);
  assert.equal(bundle.roots[2].position.grid, undefined);
  assert.ok(
    bundle.extraction_diagnostics.some(
      (diagnostic) => diagnostic.node_id === "6:2" && diagnostic.code === "FR-GRID-EXTRACT-001",
    ),
  );
  assert.ok(
    bundle.extraction_diagnostics.some(
      (diagnostic) => diagnostic.node_id === "6:3" && diagnostic.code === "FR-GRID-EXTRACT-002",
    ),
  );
}

async function preservesVariableValuesAcrossConsumerModes(): Promise<void> {
  const collectionId = multiModeFixture.collection_id;
  const valueCollectionId = `${collectionId}:values`;
  const lightMode = multiModeFixture.modes.light;
  const darkMode = multiModeFixture.modes.dark;
  const aliasId = multiModeFixture.variable_id;
  const valueId = multiModeFixture.aliased_variable_id;
  testCollections.set(collectionId, {
    id: collectionId,
    defaultModeId: lightMode,
    modes: [
      { modeId: lightMode, name: "Light" },
      { modeId: darkMode, name: "Dark" },
    ],
  });
  testCollections.set(valueCollectionId, {
    id: valueCollectionId,
    defaultModeId: lightMode,
    modes: [
      { modeId: lightMode, name: "Light" },
      { modeId: darkMode, name: "Dark" },
    ],
  });
  testVariables.set(aliasId, {
    id: aliasId,
    name: "color/alias",
    variableCollectionId: collectionId,
    valuesByMode: {
      [lightMode]: { type: "VARIABLE_ALIAS", id: valueId },
      [darkMode]: { type: "VARIABLE_ALIAS", id: valueId },
    },
    resolveForConsumer: (consumer: TestNode) => ({
      value: consumer.resolvedVariableModes?.[valueCollectionId] === darkMode
        ? multiModeFixture.values.dark
        : multiModeFixture.values.light,
      resolvedType: "COLOR",
    }),
  });
  testVariables.set(valueId, {
    id: valueId,
    name: "color/value",
    variableCollectionId: valueCollectionId,
    valuesByMode: {
      [lightMode]: multiModeFixture.values.light,
      [darkMode]: multiModeFixture.values.dark,
    },
    resolveForConsumer: (consumer: TestNode) => ({
      value: consumer.resolvedVariableModes?.[valueCollectionId] === darkMode
        ? multiModeFixture.values.dark
        : multiModeFixture.values.light,
      resolvedType: "COLOR",
    }),
  });

  const light = rectangle("7:1", [[1, 0, 0], [0, 1, 0]]);
  light.resolvedVariableModes = {
    [collectionId]: lightMode,
    [valueCollectionId]: lightMode,
  };
  light.fills = [{
    type: "SOLID",
    visible: true,
    opacity: 1,
    color: { r: 0.1, g: 0.2, b: 0.3 },
    boundVariables: { color: { type: "VARIABLE_ALIAS", id: aliasId } },
  }];
  const dark = rectangle("7:2", [[1, 0, 30], [0, 1, 0]]);
  dark.resolvedVariableModes = {
    [collectionId]: darkMode,
    [valueCollectionId]: darkMode,
  };
  dark.fills = [{
    type: "SOLID",
    visible: true,
    opacity: 1,
    color: { r: 0.9, g: 0.8, b: 0.7 },
    boundVariables: { color: { type: "VARIABLE_ALIAS", id: aliasId } },
  }];

  const mixed = rectangle("7:3", [[1, 0, 60], [0, 1, 0]]);
  mixed.resolvedVariableModes = {
    [collectionId]: lightMode,
    [valueCollectionId]: darkMode,
  };
  mixed.fills = [{
    type: "SOLID",
    visible: true,
    opacity: 1,
    color: { r: 0.9, g: 0.8, b: 0.7 },
    boundVariables: { color: { type: "VARIABLE_ALIAS", id: aliasId } },
  }];
  const defaultModes = rectangle("7:4", [[1, 0, 90], [0, 1, 0]]);
  defaultModes.fills = [{
    type: "SOLID",
    visible: true,
    opacity: 1,
    color: { r: 0.1, g: 0.2, b: 0.3 },
    boundVariables: { color: { type: "VARIABLE_ALIAS", id: aliasId } },
  }];

  const bundle = await extractNodes([
    light as unknown as SceneNode,
    dark as unknown as SceneNode,
    mixed as unknown as SceneNode,
    defaultModes as unknown as SceneNode,
  ]);
  assert.equal(bundle.schema_version, 2);
  assert.deepEqual(bundle.roots.map((root) => root.style.fills[0]), [
    {
      kind: "SOLID",
      color: {
        literal: { r: 0.1, g: 0.2, b: 0.3, a: 1 },
        token_id: aliasId,
        mode_context: {
          [collectionId]: lightMode,
          [valueCollectionId]: lightMode,
        },
      },
    },
    {
      kind: "SOLID",
      color: {
        literal: { r: 0.9, g: 0.8, b: 0.7, a: 1 },
        token_id: aliasId,
        mode_context: {
          [collectionId]: darkMode,
          [valueCollectionId]: darkMode,
        },
      },
    },
    {
      kind: "SOLID",
      color: {
        literal: { r: 0.9, g: 0.8, b: 0.7, a: 1 },
        token_id: aliasId,
        mode_context: {
          [collectionId]: lightMode,
          [valueCollectionId]: darkMode,
        },
      },
    },
    {
      kind: "SOLID",
      color: {
        literal: { r: 0.1, g: 0.2, b: 0.3, a: 1 },
        token_id: aliasId,
        mode_context: {
          [collectionId]: lightMode,
          [valueCollectionId]: lightMode,
        },
      },
    },
  ]);
  assert.deepEqual(bundle.variables, [
    {
      id: aliasId,
      name: "color/alias",
      collection_id: collectionId,
      mode_id: darkMode,
      mode_context: {
        [collectionId]: darkMode,
        [valueCollectionId]: darkMode,
      },
      source_node_id: "7:2",
      value: { kind: "COLOR", value: { r: 0.9, g: 0.8, b: 0.7, a: 1 } },
    },
    {
      id: aliasId,
      name: "color/alias",
      collection_id: collectionId,
      mode_id: lightMode,
      mode_context: {
        [collectionId]: lightMode,
        [valueCollectionId]: darkMode,
      },
      source_node_id: "7:3",
      value: { kind: "COLOR", value: { r: 0.9, g: 0.8, b: 0.7, a: 1 } },
    },
    {
      id: aliasId,
      name: "color/alias",
      collection_id: collectionId,
      mode_id: lightMode,
      mode_context: {
        [collectionId]: lightMode,
        [valueCollectionId]: lightMode,
      },
      source_node_id: "7:1",
      value: { kind: "COLOR", value: { r: 0.1, g: 0.2, b: 0.3, a: 1 } },
    },
  ]);
  assert.ok(!bundle.extraction_diagnostics.some(
    (diagnostic) => diagnostic.code === "FR-TOKEN-MODE-001" || diagnostic.code === "FR-TOKEN-MODE-004",
  ));

  const reversed = await extractNodes([
    defaultModes as unknown as SceneNode,
    mixed as unknown as SceneNode,
    dark as unknown as SceneNode,
    light as unknown as SceneNode,
  ]);
  assert.deepEqual(reversed.variables, bundle.variables);
  testVariables.clear();
  testCollections.clear();
}

async function keepsAliasCyclesScopedToOneResolution(): Promise<void> {
  const collectionId = "VariableCollectionId:test:cycle";
  const modeId = "mode-default";
  const firstId = "VariableID:test:cycle-a";
  const secondId = "VariableID:test:cycle-b";
  testCollections.set(collectionId, {
    id: collectionId,
    defaultModeId: modeId,
    modes: [{ modeId, name: "Default" }],
  });
  testVariables.set(firstId, {
    id: firstId,
    name: "cycle/a",
    variableCollectionId: collectionId,
    valuesByMode: {
      [modeId]: { type: "VARIABLE_ALIAS", id: secondId },
    },
    resolveForConsumer: () => {
      throw new Error("cycle must be detected before resolved lookup");
    },
  });
  testVariables.set(secondId, {
    id: secondId,
    name: "cycle/b",
    variableCollectionId: collectionId,
    valuesByMode: {
      [modeId]: { type: "VARIABLE_ALIAS", id: firstId },
    },
    resolveForConsumer: () => {
      throw new Error("cycle must be detected before resolved lookup");
    },
  });
  const node = rectangle("7:0", [[1, 0, 0], [0, 1, 0]]);
  node.resolvedVariableModes = { [collectionId]: modeId };
  node.fills = [{
    type: "SOLID",
    color: { r: 0, g: 0, b: 0 },
    boundVariables: { color: { type: "VARIABLE_ALIAS", id: firstId } },
  }];

  const bundle = await extractNodes([node as unknown as SceneNode]);
  assert.ok(bundle.extraction_diagnostics.some(
    (diagnostic) => diagnostic.code === "FR-TOKEN-CHAIN-001",
  ));
  testVariables.clear();
  testCollections.clear();
}

async function preservesModeledNumericBindings(): Promise<void> {
  const collectionId = "VariableCollectionId:test:numbers";
  const modeId = "mode-default";
  const values = new Map([
    ["gap", 8],
    ["padding", 12],
    ["radius", 4],
    ["stroke-top", 1],
    ["stroke-right", 2],
    ["stroke-bottom", 3],
    ["stroke-left", 4],
    ["unsupported", 0.5],
  ]);
  testCollections.set(collectionId, {
    id: collectionId,
    defaultModeId: modeId,
    modes: [{ modeId, name: "Default" }],
  });
  for (const [name, value] of values) {
    const id = `VariableID:test:${name}`;
    testVariables.set(id, {
      id,
      name,
      variableCollectionId: collectionId,
      valuesByMode: { [modeId]: value },
      resolveForConsumer: () => ({ value, resolvedType: "FLOAT" }),
    });
  }
  const alias = (name: string) => ({
    type: "VARIABLE_ALIAS",
    id: `VariableID:test:${name}`,
  });
  const node = rectangle("8:1", [[1, 0, 0], [0, 1, 0]]) as TestNode & Record<string, unknown>;
  Object.assign(node, {
    resolvedVariableModes: { [collectionId]: modeId },
    layoutMode: "HORIZONTAL",
    itemSpacing: 8,
    paddingTop: 12,
    paddingRight: 12,
    paddingBottom: 12,
    paddingLeft: 12,
    cornerRadius: figma.mixed,
    topLeftRadius: 4,
    topRightRadius: 4,
    bottomRightRadius: 4,
    bottomLeftRadius: 4,
    strokeWeight: figma.mixed,
    strokeTopWeight: 1,
    strokeRightWeight: 2,
    strokeBottomWeight: 3,
    strokeLeftWeight: 4,
    boundVariables: {
      itemSpacing: alias("gap"),
      paddingTop: alias("padding"),
      paddingRight: alias("padding"),
      paddingBottom: alias("padding"),
      paddingLeft: alias("padding"),
      topLeftRadius: alias("radius"),
      topRightRadius: alias("radius"),
      bottomRightRadius: alias("radius"),
      bottomLeftRadius: alias("radius"),
      strokeTopWeight: alias("stroke-top"),
      strokeRightWeight: alias("stroke-right"),
      strokeBottomWeight: alias("stroke-bottom"),
      strokeLeftWeight: alias("stroke-left"),
      opacity: alias("unsupported"),
    },
  });

  const bundle = await extractNodes([node as unknown as SceneNode]);
  const root = bundle.roots[0];
  const expectedContext = { [collectionId]: modeId };
  assert.deepEqual(root.layout.gap, {
    literal: 8,
    token_id: "VariableID:test:gap",
    mode_context: expectedContext,
  });
  assert.ok(Object.values(root.layout.padding).every(
    (value) => value.token_id === "VariableID:test:padding" && value.literal === 12,
  ));
  assert.ok([
    root.style.radii.top_left,
    root.style.radii.top_right,
    root.style.radii.bottom_right,
    root.style.radii.bottom_left,
  ].every((value) => value.token_id === "VariableID:test:radius" && value.literal === 4));
  assert.deepEqual(Object.values(root.style.stroke_widths).map((value) => value.literal), [1, 2, 3, 4]);
  assert.equal(bundle.extraction_diagnostics.filter(
    (diagnostic) => diagnostic.code === "FR-TOKEN-LOSS-003",
  ).length, 1);
  assert.ok(bundle.extraction_diagnostics.some(
    (diagnostic) => diagnostic.code === "FR-TOKEN-LOSS-003"
      && diagnostic.property_path === "bound_variables.opacity",
  ));
  assert.ok(!bundle.extraction_diagnostics.some(
    (diagnostic) => diagnostic.code === "FR-VALUE-EXTRACT-003"
      && diagnostic.property_path === "style.stroke_widths",
  ));
  testVariables.clear();
  testCollections.clear();
}

async function preservesBoundDimensionsAcrossConsumerModes(): Promise<void> {
  const collectionId = "VariableCollectionId:test:dimensions";
  const lightMode = "mode-light";
  const darkMode = "mode-dark";
  testCollections.set(collectionId, {
    id: collectionId,
    defaultModeId: lightMode,
    modes: [
      { modeId: lightMode, name: "Light" },
      { modeId: darkMode, name: "Dark" },
    ],
  });
  const dimensions = [
    ["width", 120],
    ["height", 40],
    ["minWidth", 80],
    ["maxWidth", 240],
    ["minHeight", 24],
    ["maxHeight", 96],
  ] as const;
  for (const [fieldName, lightValue] of dimensions) {
    const id = `VariableID:test:${fieldName}`;
    testVariables.set(id, {
      id,
      name: `size/${fieldName}`,
      variableCollectionId: collectionId,
      valuesByMode: {
        [lightMode]: lightValue,
        [darkMode]: lightValue + 100,
      },
      resolveForConsumer: (consumer: TestNode) => ({
        value: consumer.resolvedVariableModes?.[collectionId] === darkMode
          ? lightValue + 100
          : lightValue,
        resolvedType: "FLOAT",
      }),
    });
  }
  const makeNode = (id: string, modeId: string, offset: number) => {
    const node = rectangle(id, [[1, 0, 0], [0, 1, 0]]) as TestNode & Record<string, unknown>;
    Object.assign(node, {
      resolvedVariableModes: { [collectionId]: modeId },
      layoutSizingHorizontal: "FIXED",
      layoutSizingVertical: "FIXED",
      boundVariables: Object.fromEntries(dimensions.map(([fieldName]) => [
        fieldName,
        { type: "VARIABLE_ALIAS", id: `VariableID:test:${fieldName}` },
      ])),
    });
    for (const [fieldName, value] of dimensions) node[fieldName] = value + offset;
    return node;
  };
  const light = makeNode("8:2", lightMode, 0);
  const dark = makeNode("8:3", darkMode, 100);

  const bundle = await extractNodes([
    light as unknown as SceneNode,
    dark as unknown as SceneNode,
  ]);
  const expectedContext = (modeId: string) => ({ [collectionId]: modeId });
  for (const [root, modeId, offset] of [
    [bundle.roots[0], lightMode, 0],
    [bundle.roots[1], darkMode, 100],
  ] as const) {
    for (const [fieldName, value] of dimensions) {
      const rawName = fieldName.replace(/[A-Z]/g, (letter) => `_${letter.toLowerCase()}`) as keyof typeof root.size;
      assert.deepEqual(root.size[rawName], {
        literal: value + offset,
        token_id: `VariableID:test:${fieldName}`,
        mode_context: expectedContext(modeId),
      });
    }
  }
  assert.equal(bundle.extraction_diagnostics.some((diagnostic) =>
    diagnostic.code === "FR-TOKEN-LOSS-003"
    && /bound_variables\.(?:width|height|minWidth|maxWidth|minHeight|maxHeight)/.test(
      diagnostic.property_path ?? "",
    )
  ), false);
  testVariables.clear();
  testCollections.clear();
}

async function preservesChildCounterAxisAlignmentOverrides(): Promise<void> {
  const values = ["INHERIT", "MIN", "CENTER", "MAX", "STRETCH"] as const;
  const roots = values.map((layoutAlign, index) => {
    const node = rectangle(`13:${index + 1}`, [[1, 0, 0], [0, 1, 0]]);
    node.layoutAlign = layoutAlign;
    return node as unknown as SceneNode;
  });

  const bundle = await extractNodes(roots);
  assert.deepEqual(
    bundle.roots.map((root) => root.layout.child_counter_alignment),
    values,
  );
  assert.equal(bundle.extraction_diagnostics.some((diagnostic) =>
    diagnostic.code === "FR-EXTRACT-LOSS-001"
    && diagnostic.property_path === "layout.layout_align"
  ), false);
}

function supportsCompactExportAndLargeSelectionFeedback(): void {
  assert.equal(includeRestSnapshotForExport("COMPILER"), false);
  assert.equal(includeRestSnapshotForExport("EVIDENCE"), true);

  const bundle = {
    schema_version: 2,
    source: {
      page_id: "sayfa-çalışma",
      selected_node_ids: ["419:2"],
      plugin_api_version: "1.135.0",
    },
    roots: [{
      id: "419:2",
      children: [
        { id: "419:3", children: [] },
        { id: "419:4", children: [{ id: "419:5", children: [] }] },
      ],
    }],
    variables: [{ id: "variable:1" }, { id: "variable:2" }],
    components: [],
    assets: [{ id: "asset:1" }],
    extraction_diagnostics: [
      { severity: "INFO", code: "I" },
      { severity: "WARNING", code: "W" },
      { severity: "ERROR", code: "E" },
    ],
  } as unknown as ExtractionBundle;

  const summary = summarizeBundle(bundle);
  assert.equal(summary.root_count, 1);
  assert.equal(summary.node_count, 4);
  assert.equal(summary.variable_count, 2);
  assert.equal(summary.asset_count, 1);
  assert.deepEqual(summary.diagnostics, { info: 1, warnings: 1, errors: 1 });
  assert.equal(summary.includes_rest_snapshot, false);
  assert.equal(summary.estimated_bytes, Buffer.byteLength(JSON.stringify(bundle), "utf8"));
  assert.equal(summary.large_selection, false);
  assert.equal(summary.bridge_compatible, true);

  assert.deepEqual(classifyBundleBytes(LARGE_BUNDLE_BYTES), {
    large_selection: true,
    bridge_compatible: true,
  });
  assert.deepEqual(classifyBundleBytes(BRIDGE_MAX_REQUEST_BYTES + 1), {
    large_selection: true,
    bridge_compatible: false,
  });
}

function validatesBridgeExportCompletionBeforeTrustingTheFile(): void {
  const valid = {
    path: "/tmp/figma-rust-extraction.json",
    byte_length: 3_145_728,
    sha256: "a".repeat(64),
    complete: true,
    traversal_complete: true,
  };
  assert.deepEqual(validateBridgeExportResponse(valid), valid);
  for (const invalid of [
    { ...valid, sha256: "short" },
    { ...valid, complete: false },
    { ...valid, byte_length: -1 },
    { ...valid, traversal_complete: "yes" },
  ]) {
    assert.throws(
      () => validateBridgeExportResponse(invalid),
      /did not prove exact persisted bytes and completion/,
    );
  }
}

function cleansOnlyTemporaryTransferNodesAfterExportVerification(): void {
  const removed: string[] = [];
  const nodes = [
    {
      id: "845:451",
      name: "__figma_rust_bundle_ses_example_0111",
      remove: () => removed.push("845:451"),
    },
    {
      id: "419:2",
      name: "OL / Foundations / Canonical",
      remove: () => removed.push("419:2"),
    },
    {
      id: "836:340",
      name: "__figma_rust_extract_ses_example_0000",
      remove: () => removed.push("836:340"),
    },
    {
      id: "999:1",
      name: "__figma_rust_bundle_",
      remove: () => removed.push("999:1"),
    },
    {
      id: "999:2",
      name: "__figma_rust_bundle_user_note_0111",
      remove: () => removed.push("999:2"),
    },
    {
      id: "999:3",
      name: "__figma_rust_extract_ses_example_0000_copy",
      remove: () => removed.push("999:3"),
    },
  ];

  assert.equal(isTemporaryTransferNodeName(nodes[0].name), true);
  assert.equal(isTemporaryTransferNodeName(nodes[2].name), true);
  assert.equal(isTemporaryTransferNodeName(nodes[1].name), false);
  assert.equal(isTemporaryTransferNodeName(nodes[3].name), false);
  assert.equal(isTemporaryTransferNodeName(nodes[4].name), false);
  assert.equal(isTemporaryTransferNodeName(nodes[5].name), false);
  assert.deepEqual(
    findTemporaryTransferNodes(nodes).map((node) => node.id),
    ["836:340", "845:451"],
  );

  assert.throws(
    () => cleanupTemporaryTransferNodes(nodes, false),
    /Verify the downloaded compiler JSON before cleanup/,
  );
  assert.deepEqual(removed, []);

  const result = cleanupTemporaryTransferNodes(nodes, true);
  assert.deepEqual(result, {
    removed_count: 2,
    removed_node_ids: ["836:340", "845:451"],
  });
  assert.deepEqual(removed, ["836:340", "845:451"]);
}

async function exportsDeterministicFallbackPayloads(): Promise<void> {
  const svg = rectangle("9:1", [[1, 0, 0], [0, 1, 0]]);
  svg.type = "VECTOR";
  svg.exportAsync = async (settings) => {
    assert.equal(settings.format, "SVG");
    return Buffer.from("<svg/>");
  };
  const bundle = await extractNodes([svg as unknown as SceneNode]);
  assert.deepEqual(bundle.assets, [{
    id: "node:9:1:svg",
    source_node_id: "9:1",
    media_type: "image/svg+xml",
    export_settings: { color_policy: "authored", format: "SVG" },
    payload_base64: "PHN2Zy8+",
  }]);

  let previewExported = false;
  const previewSvg = rectangle("9:2", [[1, 0, 0], [0, 1, 0]]);
  previewSvg.type = "VECTOR";
  previewSvg.exportAsync = async () => {
    previewExported = true;
    return Buffer.from("<svg/>");
  };
  const preview = await extractNodes(
    [previewSvg as unknown as SceneNode],
    false,
    undefined,
    false,
  );
  assert.equal(previewExported, false);
  assert.equal(preview.assets[0].payload_base64, undefined);

  let childExported = false;
  const childSvg = rectangle("9:4", [[1, 0, 0], [0, 1, 0]]);
  childSvg.type = "VECTOR";
  childSvg.exportAsync = async () => {
    childExported = true;
    return Buffer.from("<svg/>");
  };
  const parentSvg = rectangle("9:3", [[1, 0, 0], [0, 1, 0]]);
  parentSvg.type = "VECTOR";
  parentSvg.children = [childSvg];
  parentSvg.exportAsync = async () => Buffer.from("<svg><path/></svg>");
  const nested = await extractNodes([parentSvg as unknown as SceneNode]);
  assert.equal(childExported, false);
  assert.deepEqual(nested.assets.map((asset) => asset.source_node_id), ["9:3"]);

  const raw = bundle.roots[0];
  raw.kind = "TEXT";
  raw.text = {
    characters: "Tracked",
    runs: [{ start_utf16: 0, end_utf16: 7, style: { letter_spacing: 1 } }],
  };
  assert.equal(fallbackExportFormat(raw), "SVG");
  raw.style.blend_mode = "MULTIPLY";
  assert.equal(fallbackExportFormat(raw), "PNG");
}

async function exportsTrackedTextFallbackPayload(): Promise<void> {
  const trackedText = {
    ...rectangle("9:5", [[1, 0, 0], [0, 1, 0]]),
    type: "TEXT",
    characters: "Symbol",
    hasMissingFont: false,
    textAutoResize: "WIDTH_AND_HEIGHT",
    textAlignHorizontal: "LEFT",
    textAlignVertical: "TOP",
    textTruncation: "DISABLED",
    maxLines: null,
    leadingTrim: "CAP_HEIGHT_TO_BASELINE",
    paragraphIndent: 4,
    paragraphSpacing: 8,
    textWrapStyle: "BALANCE",
    listSpacing: 6,
    getStyledTextSegments: () => [{
      start: 0,
      end: 6,
      fontName: { family: "JetBrains Mono", style: "Medium" },
      fontSize: 10,
      fontWeight: 500,
      lineHeight: { unit: "PIXELS", value: 12 },
      letterSpacing: { unit: "PIXELS", value: 0.4 },
      fills: [{ type: "SOLID", color: { r: 0.48, g: 0.53, b: 0.58 } }],
    }],
    exportAsync: async (settings: { format: "SVG" | "PNG" }) => {
      assert.equal(settings.format, "SVG");
      return Buffer.from("<svg/>");
    },
  } as unknown as SceneNode;

  const bundle = await extractNodes([trackedText]);
  assert.deepEqual(bundle.roots[0].text, {
    characters: "Symbol",
    auto_resize: "WIDTH_AND_HEIGHT",
    horizontal_alignment: "LEFT",
    vertical_alignment: "TOP",
    truncation: "DISABLED",
    runs: [bundle.roots[0].text?.runs[0]],
  });
  assert.equal(bundle.roots[0].text?.runs[0].style.letter_spacing, 0.4);
  assert.deepEqual(
    bundle.extraction_diagnostics
      .filter((diagnostic) => diagnostic.code === "FR-EXTRACT-LOSS-001")
      .map((diagnostic) => diagnostic.property_path)
      .filter((path) => path?.startsWith("text.")),
    [
      "text.leading_trim",
      "text.list_spacing",
      "text.paragraph_indent",
      "text.paragraph_spacing",
      "text.wrap_style",
    ],
  );
  assert.equal(bundle.extraction_diagnostics.some((diagnostic) =>
    diagnostic.property_path === "text"
  ), false);

  const truncated = await extractNodes([{
    ...trackedText,
    id: "9:6",
    textAutoResize: "HEIGHT",
    textAlignHorizontal: "JUSTIFIED",
    textAlignVertical: "BOTTOM",
    textTruncation: "ENDING",
    maxLines: 3,
  } as unknown as SceneNode]);
  assert.deepEqual(truncated.roots[0].text, {
    characters: "Symbol",
    auto_resize: "HEIGHT",
    horizontal_alignment: "JUSTIFIED",
    vertical_alignment: "BOTTOM",
    truncation: "ENDING",
    max_lines: 3,
    runs: [truncated.roots[0].text?.runs[0]],
  });
  const invalidMaxLines = await extractNodes([{
    ...trackedText,
    id: "9:7",
    textTruncation: "ENDING",
    maxLines: 0,
  } as unknown as SceneNode]);
  assert.equal(invalidMaxLines.roots[0].text?.max_lines, undefined);
  assert.equal(invalidMaxLines.extraction_diagnostics.some((diagnostic) =>
    diagnostic.code === "FR-TEXT-EXTRACT-004"
    && diagnostic.node_id === "9:7"
    && diagnostic.property_path === "text.max_lines"
  ), true);
  assert.deepEqual(bundle.assets, [{
    id: "node:9:5:svg",
    source_node_id: "9:5",
    media_type: "image/svg+xml",
    export_settings: { color_policy: "authored", format: "SVG" },
    payload_base64: "PHN2Zy8+",
  }]);
}

function checkedInDataTableExtractionCarriesRequiredFallbackPayloads(): void {
  const bundle = dataTableExtractionFixture as unknown as ExtractionBundle;
  const visit = (
    node: ExtractionBundle["roots"][number],
    capturedByAncestor = false,
  ): void => {
    const format = fallbackExportFormat(node);
    if (!capturedByAncestor && format !== undefined) {
      const mediaType = format === "SVG" ? "image/svg+xml" : "image/png";
      const matches = bundle.assets.filter((asset) =>
        asset.source_node_id === node.id
        && asset.media_type === mediaType
        && asset.export_settings.format === format
        && typeof asset.payload_base64 === "string"
        && asset.payload_base64.length > 0
      );
      assert.equal(
        matches.length,
        1,
        `node ${node.id} requires one ${format} fallback payload`,
      );
    }
    for (const child of node.children) {
      visit(child, capturedByAncestor || format !== undefined);
    }
  };

  for (const root of bundle.roots) visit(root);
}

async function exportsFoundationScaleFallbackAssets(): Promise<void> {
  const nodes = Array.from({ length: 353 }, (_, index) => {
    const node = rectangle(`10:${index + 1}`, [[1, 0, 0], [0, 1, 0]]);
    node.type = "VECTOR";
    node.exportAsync = async () => Buffer.from("<svg/>");
    return node as unknown as SceneNode;
  });

  const bundle = await extractNodes(nodes);
  assert.equal(bundle.assets.length, nodes.length);
  assert.equal(
    bundle.extraction_diagnostics.some(
      (diagnostic) => diagnostic.code === "FR-ASSET-LIMIT-001",
    ),
    false,
  );
}

async function extractsMoreThanTwoThousandNodesDeterministically(): Promise<void> {
  const roots = Array.from({ length: 4 }, (_, rootIndex) => {
    const root = rectangle(`20:${rootIndex + 1}`, [[1, 0, 0], [0, 1, 0]]);
    root.children = Array.from({ length: 525 }, (_, childIndex) =>
      rectangle(
        `20:${rootIndex + 1}:${childIndex + 1}`,
        [[1, 0, childIndex], [0, 1, rootIndex]],
      )
    );
    return root;
  });

  const first = await extractNodes(
    roots as unknown as SceneNode[],
    false,
    createExtractionDeadline(10_000),
    false,
  );
  const second = await extractNodes(
    roots as unknown as SceneNode[],
    false,
    createExtractionDeadline(10_000),
    false,
  );

  assert.equal(first.extraction_manifest.traversal.node_count, 2_104);
  assert.equal(first.extraction_manifest.traversal.chunk_node_limit, 2_000);
  assert.equal(first.extraction_manifest.traversal.complete, true);
  assert.equal(first.extraction_manifest.traversal.chunks.length, 2);
  assert.deepEqual(first.extraction_manifest.traversal.chunks, [
    {
      index: 0,
      start_node_index: 0,
      end_node_index: 2_000,
      node_count: 2_000,
      first_node_id: "20:1",
      last_node_id: "20:4:421",
    },
    {
      index: 1,
      start_node_index: 2_000,
      end_node_index: 2_104,
      node_count: 104,
      first_node_id: "20:4:422",
      last_node_id: "20:4:525",
    },
  ]);
  assert.deepEqual(
    first.extraction_manifest.traversal.roots.map((root) => [root.id, root.node_count, root.complete]),
    [["20:1", 526, true], ["20:2", 526, true], ["20:3", 526, true], ["20:4", 526, true]],
  );
  assert.equal(
    first.extraction_diagnostics.some((diagnostic) => diagnostic.code === "FR-EXTRACT-LIMIT-001"),
    false,
  );
  assert.equal(JSON.stringify(first), JSON.stringify(second));
}

async function marksDepthLimitedSubtreesIncomplete(): Promise<void> {
  const root = rectangle("21:0", [[1, 0, 0], [0, 1, 0]]);
  let parent = root;
  for (let depth = 1; depth <= 65; depth += 1) {
    const child = rectangle(`21:${depth}`, [[1, 0, 0], [0, 1, depth]]);
    parent.children = [child];
    parent = child;
  }

  const bundle = await extractNodes([root as unknown as SceneNode], false, undefined, false);
  assert.equal(bundle.extraction_manifest.traversal.node_count, 65);
  assert.equal(bundle.extraction_manifest.traversal.complete, false);
  assert.deepEqual(bundle.extraction_manifest.traversal.roots, [{
    id: "21:0",
    node_count: 65,
    complete: false,
  }]);
  assert.ok(bundle.extraction_diagnostics.some((diagnostic) =>
    diagnostic.code === "FR-EXTRACT-LIMIT-002"
    && diagnostic.node_id === "21:64"
    && diagnostic.property_path === "children"
  ));
}

await extractsGroupLocalCoordinates();
await extractsNestedGroupLocalCoordinates();
await extractsRotatedGroupLocalCoordinates();
await rejectsIllConditionedGroupTransform();
await ignoresNonGridPlacementSentinels();
await preservesAndValidatesGridPlacements();
await keepsAliasCyclesScopedToOneResolution();
await preservesVariableValuesAcrossConsumerModes();
await preservesModeledNumericBindings();
await preservesBoundDimensionsAcrossConsumerModes();
await preservesChildCounterAxisAlignmentOverrides();
supportsCompactExportAndLargeSelectionFeedback();
validatesBridgeExportCompletionBeforeTrustingTheFile();
cleansOnlyTemporaryTransferNodesAfterExportVerification();
await exportsDeterministicFallbackPayloads();
await exportsTrackedTextFallbackPayload();
checkedInDataTableExtractionCarriesRequiredFallbackPayloads();
await exportsFoundationScaleFallbackAssets();
await extractsMoreThanTwoThousandNodesDeterministically();
await marksDepthLimitedSubtreesIncomplete();
console.log("extraction tests passed");
