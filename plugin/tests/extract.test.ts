import assert from "node:assert/strict";

import { extractNodes } from "../src/extract";
import realGroupFixture from "./fixtures/real-group.plugin-api.json";
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
  children?: TestNode[];
}

Object.assign(globalThis, {
  figma: {
    fileKey: realGroupFixture.file_key,
    currentPage: { id: "0:1", selection: [] },
    mixed: Symbol("mixed"),
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

await extractsGroupLocalCoordinates();
await extractsNestedGroupLocalCoordinates();
await extractsRotatedGroupLocalCoordinates();
await rejectsIllConditionedGroupTransform();
await ignoresNonGridPlacementSentinels();
await preservesAndValidatesGridPlacements();
console.log("group coordinate extraction tests passed");
