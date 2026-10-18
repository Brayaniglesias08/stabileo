/**
 * A member's number and length beside it: a member seen end-on in a 3D figure (its ends on one
 * point of the page) keeps them, beside the point.
 */
import { describe, expect, it } from 'vitest';
import { addMemberTags } from '../member-tags';
import { createLabelCollector } from '../label-layout';
import { createSvgContext } from '../../export/svg-context';

function draw(a: { x: number; y: number }, b: { x: number; y: number }) {
  const svg = createSvgContext(200, 200);
  const labels = createLabelCollector();
  addMemberTags(labels, a, b, 1, { text: 'E1', colour: '#7fb0e2' }, { text: '3.00 m', colour: '#88cc88' });
  labels.flush(svg.ctx);
  return svg.body();
}

describe('member tags', () => {
  it('a member across the page has its number and length', () => {
    const body = draw({ x: 20, y: 100 }, { x: 180, y: 100 });
    expect(body).toContain('E1');
    expect(body).toContain('3.00 m');
  });

  it('a member seen end-on keeps its number and length, beside the point', () => {
    const body = draw({ x: 100, y: 100 }, { x: 100, y: 100 });
    expect(body).toContain('E1');
    expect(body).toContain('3.00 m');
  });
});
