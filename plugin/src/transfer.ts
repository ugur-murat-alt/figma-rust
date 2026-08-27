const TEMPORARY_TRANSFER_NAME =
  /^__figma_rust_(?:extract|bundle)_ses_[A-Za-z0-9]+_[0-9]{4}$/;

export interface RemovableNamedNode {
  readonly id: string;
  readonly name: string;
  remove(): void;
}

export interface TransferCleanupResult {
  removed_count: number;
  removed_node_ids: string[];
}

export function isTemporaryTransferNodeName(name: string): boolean {
  return TEMPORARY_TRANSFER_NAME.test(name);
}

export function findTemporaryTransferNodes<T extends RemovableNamedNode>(
  nodes: readonly T[],
): T[] {
  return nodes
    .filter((node) => isTemporaryTransferNodeName(node.name))
    .sort((left, right) => left.id.localeCompare(right.id));
}

export function cleanupTemporaryTransferNodes(
  nodes: readonly RemovableNamedNode[],
  exportVerified: boolean,
): TransferCleanupResult {
  if (!exportVerified) {
    throw new Error("Verify the downloaded compiler JSON before cleanup.");
  }
  const temporaryNodes = findTemporaryTransferNodes(nodes);
  const removedNodeIds = temporaryNodes.map((node) => node.id);
  for (const node of temporaryNodes) node.remove();
  return {
    removed_count: removedNodeIds.length,
    removed_node_ids: removedNodeIds,
  };
}
