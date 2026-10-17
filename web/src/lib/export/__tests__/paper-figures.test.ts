import { describe, it, expect } from 'vitest';
import { inkColor, parseColor } from '../paper-ink';
import { createSvgContext, parseFont } from '../svg-context';
import { inkContext } from '../ink-context';

const lightness = (css: string) => {
  const c = parseColor(css)!;
  return (Math.max(c.r, c.g, c.b) + Math.min(c.r, c.g, c.b)) / 510;
};

describe('the viewport colours on paper', () => {
  it('light neutrals become ink, dark grounds become paper', () => {
    expect(lightness(inkColor('#c7d3dd', 'color'))).toBeLessThan(0.2);   // a member
    expect(lightness(inkColor('#f4f7fa', 'color'))).toBeLessThan(0.2);   // text
    expect(inkColor('#0c1620', 'color')).toBe('rgb(255,255,255)');       // the canvas
    expect(inkColor('rgba(10,14,24,0.85)', 'color')).toBe('rgba(255,255,255,0.85)'); // a label halo
  });

  it('a light accent is darkened until it reads on white, keeping its hue', () => {
    const c = parseColor(inkColor('#f07a62', 'color'))!;
    expect(lightness(inkColor('#f07a62', 'color'))).toBeLessThanOrEqual(0.39);
    expect(c.r).toBeGreaterThan(c.g);
    expect(c.r).toBeGreaterThan(c.b);
  });

  it('a pale translucent fill is left as it is', () => {
    expect(inkColor('rgba(229, 72, 42, 0.22)', 'color')).toBe('rgba(229,72,42,0.22)');
  });

  it('in black and white every colour is a grey', () => {
    for (const css of ['#e5482a', '#4a8fd4', '#d9a441', 'rgba(229, 72, 42, 0.22)', '#c7d3dd']) {
      const c = parseColor(inkColor(css, 'bw'))!;
      expect(c.r).toBe(c.g);
      expect(c.g).toBe(c.b);
    }
  });

  it('on screen nothing changes', () => {
    expect(inkColor('#c7d3dd', 'screen')).toBe('#c7d3dd');
  });
});

describe('the SVG drawing context', () => {
  it('writes paths with the transform applied and frames what it drew', () => {
    const svg = createSvgContext(100, 100);
    const ctx = svg.ctx;
    ctx.translate(10, 20);
    ctx.strokeStyle = '#ff0000';
    ctx.lineWidth = 2;
    ctx.beginPath(); ctx.moveTo(0, 0); ctx.lineTo(30, 0); ctx.stroke();
    expect(svg.body()).toContain('d="M10 20L40 20"');
    expect(svg.body()).toContain('stroke="rgb(255,0,0)"');
    const b = svg.bounds();
    expect(b.x0).toBeCloseTo(9); expect(b.x1).toBeCloseTo(41);
  });

  it('keeps text as text, with the anchor and the alpha as attributes', () => {
    const svg = createSvgContext(100, 100);
    svg.ctx.font = 'bold 12px sans-serif';
    svg.ctx.textAlign = 'center';
    svg.ctx.fillStyle = 'rgba(0, 0, 255, 0.5)';
    svg.ctx.fillText('M = 12 <kN·m>', 50, 50);
    const body = svg.body();
    expect(body).toContain('text-anchor="middle"');
    expect(body).toContain('font-weight="bold"');
    expect(body).toContain('fill-opacity="0.5"');
    expect(body).toContain('M = 12 &lt;kN·m&gt;');
  });

  it('save and restore bring the state back', () => {
    const svg = createSvgContext(10, 10);
    svg.ctx.fillStyle = '#123456';
    svg.ctx.save(); svg.ctx.fillStyle = '#ffffff'; svg.ctx.translate(5, 5); svg.ctx.restore();
    expect(svg.ctx.fillStyle).toBe('#123456');
    svg.ctx.fillRect(0, 0, 1, 1);
    expect(svg.body()).toContain('M0 0L1 0L1 1L0 1Z');
  });

  it('reads the parts of a font shorthand', () => {
    expect(parseFont('bold 11px sans-serif')).toMatchObject({ size: 11, weight: 'bold' });
    expect(parseFont('10px monospace')).toMatchObject({ size: 10, weight: 'normal', family: 'monospace' });
  });

  it('drops the control characters XML forbids, so the file still opens', () => {
    const svg = createSvgContext(100, 100);
    svg.ctx.fillText(`Viga${String.fromCharCode(11)}1${String.fromCharCode(0)}\tA`, 10, 10);
    const body = svg.body();
    for (const c of body) {
      const k = c.codePointAt(0)!;
      expect(k < 0x20 && k !== 0x09 && k !== 0x0a && k !== 0x0d).toBe(false);
    }
    expect(body).toContain('Viga1');
  });
});

describe('the ink context', () => {
  it('maps what is set and gives back what was set, so a colour is never mapped twice', () => {
    const svg = createSvgContext(10, 10);
    const ink = inkContext(svg.ctx, 'color', 2);
    ink.fillStyle = '#c7d3dd';
    expect(ink.fillStyle).toBe('#c7d3dd');
    expect(lightness(svg.ctx.fillStyle as string)).toBeLessThan(0.2);
    const keep = ink.fillStyle;
    ink.fillStyle = keep;
    expect(lightness(svg.ctx.fillStyle as string)).toBeLessThan(0.2);
    ink.font = '10px sans-serif';
    expect(svg.ctx.font).toBe('20px sans-serif');
  });
});
