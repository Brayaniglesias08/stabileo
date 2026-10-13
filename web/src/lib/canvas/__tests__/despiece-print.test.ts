import { describe, it, expect } from 'vitest';
import { printSpan, printSpans, valence, PRINT_GAP_PX } from '../draw-despiece-print';

describe('the despiece laid out for print', () => {
  // A portal: two columns and a beam. The bases have one member each; the knees, two.
  const nodes: Record<number, { x: number; y: number }> = { 1: { x: 0, y: 0 }, 2: { x: 0, y: 4 }, 3: { x: 6, y: 4 }, 4: { x: 6, y: 0 } };
  const elements = [{ id: 1, nodeI: 1, nodeJ: 2 }, { id: 2, nodeI: 2, nodeJ: 3 }, { id: 3, nodeI: 3, nodeJ: 4 }];

  it('cuts a member only at joints it shares, by the same distance on the page', () => {
    expect([...valence(elements)]).toEqual([[1, 1], [2, 2], [3, 2], [4, 1]]);
    const zoom = 100; // px per metre
    const spans = printSpans({ elements, getNode: (id) => nodes[id], zoom });
    const col = spans.get(1)!;
    expect(col.aI).toEqual({ x: 0, y: 0 });                       // the base stays whole
    expect(col.aJ.y).toBeCloseTo(4 - PRINT_GAP_PX / zoom);         // the knee is cut back 58 px
    const beam = spans.get(2)!;
    expect(beam.aI.x).toBeCloseTo(PRINT_GAP_PX / zoom);
    expect(beam.aJ.x).toBeCloseTo(6 - PRINT_GAP_PX / zoom);
  });

  it('a member too short for the gap keeps a third of its length', () => {
    const s = printSpan({ x: 0, y: 0 }, { x: 0.3, y: 0 }, true, true, 100);
    expect(s.lenShrunk).toBeCloseTo(0.1);
  });
});
