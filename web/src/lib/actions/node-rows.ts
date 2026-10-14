/**
 * A new row of PRO's nodes table, written to the model.
 */
import { modelStore } from '../store/model.svelte';

/**
 * A node at (x, y, z), or the one already there (`addNodeWelded`: a twin in the same place looks
 * joined and analyses as a cut). The row's name goes to a node the row created, or to one that
 * has none: a row welded onto "Apoyo A" renamed it "B2".
 */
export function addNodeRow(x: number, y: number, z: number, name: string): number {
  const before = modelStore.nodes.size;
  const id = modelStore.addNodeWelded(x, y, z);
  const created = modelStore.nodes.size > before;
  if (name.trim() && (created || !modelStore.nodes.get(id)?.name)) modelStore.renameNode(id, name);
  return id;
}
