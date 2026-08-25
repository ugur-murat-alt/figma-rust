import assert from "node:assert/strict";

import { extractNodes } from "../src/extract";
import realGroupFixture from "./fixtures/real-group.plugin-api.json";
import multiModeFixture from "./fixtures/multi-mode-variables.json";
import realExtractionFixture from "../../fixtures/real-figma/extraction.json";

type Transform = [[number, number, number], [number, number, number]];

interface TestNode {
  id: string;
  name: string;
  type: "GROUP" | "RECTANGLE";
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
  fills?: unknown[];
  children?: TestNode[];
}

const testVariables = new Map<string, unknown>();
const testCollections = new Map<string, unknown>();

Object.assign(globalThis, {
  figma: {
    fileKey: realGroupFixture.file_key,
    currentPage: { id: "0:1", selection: [] },
    mixed: Symbol("mixed"),
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

await extractsGroupLocalCoordinates();
await extractsNestedGroupLocalCoordinates();
await extractsRotatedGroupLocalCoordinates();
await rejectsIllConditionedGroupTransform();
await ignoresNonGridPlacementSentinels();
await preservesAndValidatesGridPlacements();
await keepsAliasCyclesScopedToOneResolution();
await preservesVariableValuesAcrossConsumerModes();
await preservesModeledNumericBindings();
console.log("extraction tests passed");
