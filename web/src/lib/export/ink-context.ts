/**
 * A drawing context that puts the viewport's colours on paper and sizes its text for the figure.
 *
 * It stands in front of a real canvas context or an SVG one (`svg-context.ts`): every colour set
 * on it goes through `inkColor` for the chosen style, every font through the figure's text scale,
 * and everything else reaches the context behind it untouched.
 */
import { inkColor, type InkStyle } from './paper-ink';
import { scaleFont } from '../canvas/text-scale';

export function inkContext(target: CanvasRenderingContext2D, style: InkStyle, textScale = 1): CanvasRenderingContext2D {
  const map = (v: unknown) => (typeof v === 'string' ? inkColor(v, style) : v);
  // What the drawing set, given back when it reads a colour or a font: code that saves a colour
  // and sets it again must not have it mapped twice.
  const asSet = new Map<PropertyKey, unknown>();
  const stack: Array<Map<PropertyKey, unknown>> = [];
  return new Proxy(target, {
    get(t, key) {
      if (asSet.has(key)) return asSet.get(key);
      if (key === 'save') return () => { stack.push(new Map(asSet)); t.save(); };
      if (key === 'restore') return () => { const m = stack.pop(); if (m) { asSet.clear(); for (const [k, v] of m) asSet.set(k, v); } t.restore(); };
      const v = Reflect.get(t, key, t);
      return typeof v === 'function' ? v.bind(t) : v;
    },
    set(t, key, value) {
      if (key === 'fillStyle' || key === 'strokeStyle' || key === 'shadowColor') { asSet.set(key, value); value = map(value); }
      else if (key === 'font' && typeof value === 'string') { asSet.set(key, value); value = scaleFont(value, textScale); }
      return Reflect.set(t, key, value, t);
    },
  });
}
