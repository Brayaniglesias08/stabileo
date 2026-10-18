/**
 * A member's number and its length (the dimension) beside its midpoint, through the frame's label
 * collector, so they keep off the members and off every other label: the 2D view, its exports and
 * the 3D figures place them the same way.
 *
 * On a member drawn across the screen they are one block under it (the number first), where the
 * loads, drawn above, are not. On an upright one a block stacks along the escape direction, which
 * is across the member, and the two texts landed on each other: there the number goes to the
 * left and the length to the right, each pushed outward. A member seen end-on keeps them too,
 * beside its point.
 */
import type { LabelCollector } from './label-layout';

type P = { x: number; y: number };
export type MemberTag = { text: string; colour: string };

const FONT = '10px sans-serif';

export function addMemberTags(labels: LabelCollector, a: P, b: P, id: number, number: MemberTag | null, length: MemberTag | null): void {
  const dx = b.x - a.x, dy = b.y - a.y, l = Math.hypot(dx, dy);
  if (!number && !length) return;
  const mid = { x: (a.x + b.x) / 2, y: (a.y + b.y) / 2 };
  if (l < 1e-6) {
    // Seen end-on (a member along the line of sight in a 3D figure): no direction to stand off
    // from, so the block goes under and right of the point, clear of the node's own number.
    const box = (priority: number) => ({
      x: mid.x + 8, y: mid.y + 16, width: 0, height: 12,
      dirX: 0, dirY: 1, priority, anchorX: 'left' as const, sweep: 'any' as const, group: `member-${id}`,
    });
    if (number) labels.add({ text: number.text, colour: number.colour, font: FONT, box: box(1) });
    if (length) labels.add({ text: length.text, colour: length.colour, font: FONT, box: box(0) });
    return;
  }
  if (Math.abs(dy) > Math.abs(dx)) {
    // Upright: the normal pointing right, and its opposite.
    let nx = -dy / l, ny = dx / l;
    if (nx < 0) { nx = -nx; ny = -ny; }
    if (number) labels.add({ text: number.text, colour: number.colour, font: FONT, box: { x: mid.x - 7, y: mid.y + 4, width: 0, height: 12, dirX: -nx, dirY: -ny, priority: 1, anchorX: 'right', sweep: 'any' } });
    if (length) labels.add({ text: length.text, colour: length.colour, font: FONT, box: { x: mid.x + 7, y: mid.y + 4, width: 0, height: 12, dirX: nx, dirY: ny, priority: 0, anchorX: 'left', sweep: 'any' } });
    return;
  }
  // Across: the normal pointing down the screen, the block under the member.
  let nx = -dy / l, ny = dx / l;
  if (ny < 0) { nx = -nx; ny = -ny; }
  const box = (priority: number) => ({
    x: mid.x + nx * 8, y: mid.y + ny * 8 + 10, width: 0, height: 12,
    dirX: nx, dirY: ny, priority, anchorX: 'center' as const, sweep: 'any' as const, group: `member-${id}`,
  });
  if (number) labels.add({ text: number.text, colour: number.colour, font: FONT, box: box(1) });
  if (length) labels.add({ text: length.text, colour: length.colour, font: FONT, box: box(0) });
}
