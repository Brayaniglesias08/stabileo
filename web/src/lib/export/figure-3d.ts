/**
 * Figures of a 3D model, drawn as vectors: the same figures the 2D view makes, for a structure in
 * space.
 *
 * A capture of the 3D view was a screenshot: a dark picture with the model small in it, labels
 * a few pixels high once the picture fits a page, a deformed shape at true scale (invisible) and
 * diagrams without their values. Here the model is projected (orthographic, from where the
 * reader's camera looks) and drawn on a 2D context like the 2D figures, so it goes through the
 * same paper ink, the same framing and the same SVG and PNG output:
 *
 * - structure: members, nodes, supports, the loads with their values, optional numbering;
 * - reactions: the structure, its loads, and the reaction at every support with its components;
 * - despiece: `drawFreeBody`, as on screen;
 * - M, V, N: each member's diagram in its own plane (the side the 3D view draws it on), the
 *   largest value of each member written; both bending planes when both carry load;
 * - deformed: the deflected members, scaled so the largest movement reads at about a twentieth of
 *   the structure, with the largest displacements written;
 * - shellMap (PRO): the shells painted by one of their results, each element its own value, with
 *   the scale beside them.
 *
 * Shells are drawn in every figure, as faces under the members, the nearest last; the deformed
 * figure moves their corners with the nodes. A figure can be drawn from a fixed view (plan, the
 * two elevations, isometric) and from results other than the ones on screen (`FigureOptions`).
 */
import { modelStore, resultsStore, uiStore } from '../store';
import { computeLocalAxes3D } from '../engine/local-axes-3d';
import { projectNodeToScene, shouldProjectModelToXZ } from '../geometry/coordinate-system';
import { computeDiagram3D, computeGlobalMax3D, getDiagramLocalDirection, type Diagram3DKind } from '../engine/diagrams-3d';
import { computeDiagramDisplayDirection } from '../three/diagram-render-3d';
import { computeDeformedShape3D, type ElementEI } from '../three/deformed-shape-3d';
import { eiOf } from '../engine/member-deflection';
import { drawFreeBody, type P3 } from '../canvas/free-body';
import { createLabelCollector } from '../canvas/label-layout';
import { freeBodyActions3D } from '../viewport3d/free-body-3d';
import { fixedQuantity, displacementText } from '../utils/unit-format';
import type { ExportFrame, Figure2DRenderer, FigureView, FrameView } from './view-figure';
import type { AnalysisResults3D } from '../engine/types-3d';
import { shellComponentMeta, shellComponentValue, type ShellContourComponent } from '../engine/shell-stress';
import { shellContourColor } from '../three/stress-heatmap';
import { RAW_INK } from './paper-ink';

type V = [number, number, number];

/** The direction a fixed view looks along. */
const FIXED_VIEW: Record<Exclude<FigureView, 'screen'>, V> = {
  top: [0, 0, -1], front: [0, 1, 0], side: [-1, 0, 0], iso: [-1, 1, -1],
};

/** The view the figure is drawn from: the reader's camera direction or a fixed one, Z up, orthographic. */
function viewBasis(view: FigureView = 'screen'): { right: V; up: V; dir: V } {
  const p = uiStore.cameraPosition3D, t = uiStore.cameraTarget3D;
  let d: V = view === 'screen' ? [t.x - p.x, t.y - p.y, t.z - p.z] : [...FIXED_VIEW[view]];
  const l = Math.hypot(...d) || 1;
  d = [d[0] / l, d[1] / l, d[2] / l];
  // right = d × Z; if looking straight down, along X.
  let r: V = [d[1], -d[0], 0];
  const rl = Math.hypot(...r);
  r = rl > 1e-6 ? [r[0] / rl, r[1] / rl, 0] : [1, 0, 0];
  const u: V = [r[1] * d[2] - r[2] * d[1], r[2] * d[0] - r[0] * d[2], r[0] * d[1] - r[1] * d[0]];
  return { right: r, up: u, dir: d };
}

function project2D(): boolean {
  return shouldProjectModelToXZ({
    analysisMode: uiStore.analysisMode, viewportPresentation3D: uiStore.viewportPresentation3D,
    nodes: modelStore.nodes.values(), supports: modelStore.supports.values(), loads: modelStore.loads,
    plateCount: modelStore.plates.size, quadCount: modelStore.quads.size,
  });
}

const fmtF = (v: number, q: 'force' | 'moment' | 'distributedLoad') => fixedQuantity(v, q, 1, uiStore.unitSystem);

const COLORS: Record<string, { fill: string; stroke: string }> = {
  momentZ: { fill: 'rgba(229, 72, 42, 0.22)', stroke: '#e5482a' },
  momentY: { fill: 'rgba(150, 92, 200, 0.22)', stroke: '#965cc8' },
  shearY: { fill: 'rgba(74, 143, 212, 0.22)', stroke: '#4a8fd4' },
  shearZ: { fill: 'rgba(40, 160, 150, 0.22)', stroke: '#28a096' },
  axial: { fill: 'rgba(217, 164, 65, 0.22)', stroke: '#d9a441' },
};
const SYM: Record<Diagram3DKind, string> = { momentY: 'My', momentZ: 'Mz', shearY: 'Vy', shearZ: 'Vz', axial: 'N', torsion: 'T' };

/** The diagram components a content draws: both planes when both carry something. */
export function components3D(content: 'moment' | 'shear' | 'axial', results?: AnalysisResults3D | null): Diagram3DKind[] {
  const r3d = results ?? resultsStore.results3D;
  if (content === 'axial') return ['axial'];
  const pair: Diagram3DKind[] = content === 'moment' ? ['momentZ', 'momentY'] : ['shearY', 'shearZ'];
  if (!r3d) return pair.slice(0, 1);
  const max = pair.map((k) => computeGlobalMax3D(r3d.elementForces, k));
  const top = Math.max(...max);
  return pair.filter((_, i) => max[i]! > 0.1 * top && max[i]! > 1e-9);
}

export function createFigure3DRenderer(): Figure2DRenderer {
  const flat = () => project2D();
  const scenePos = (id: number): P3 | undefined => {
    const n = modelStore.nodes.get(id);
    return n ? projectNodeToScene(n, flat()) : undefined;
  };

  /** The shells as corner lists, keyed as the contour field keys them (`p12`, `q7`). */
  const shellFaces = () => {
    const out: Array<{ key: string; id: number; nodes: number[] }> = [];
    for (const p of modelStore.plates.values()) out.push({ key: `p${p.id}`, id: p.id, nodes: [...p.nodes] });
    for (const q of modelStore.quads.values()) out.push({ key: `q${q.id}`, id: q.id, nodes: [...q.nodes] });
    return out;
  };

  return {
    nodes(frame?: ExportFrame) {
      const { right, up } = viewBasis(frame?.options.view);
      const out: Array<{ x: number; y: number }> = [];
      for (const n of modelStore.nodes.values()) {
        const p = projectNodeToScene(n, flat());
        const v: V = [p.x, p.y, p.z ?? 0];
        out.push({ x: v[0] * right[0] + v[1] * right[1] + v[2] * right[2], y: v[0] * up[0] + v[1] * up[1] + v[2] * up[2] });
      }
      return out;
    },

    render(ctx: CanvasRenderingContext2D, _w: number, _h: number, frame: ExportFrame, view: FrameView) {
      const { right, up, dir } = viewBasis(frame.options.view);
      const toScreen = (p: P3) => {
        const v: V = [p.x, p.y, p.z ?? 0];
        const u = v[0] * right[0] + v[1] * right[1] + v[2] * right[2];
        const w = v[0] * up[0] + v[1] * up[1] + v[2] * up[2];
        return { x: u * view.zoom + view.panX, y: -w * view.zoom + view.panY };
      };
      const towardViewer = (d: P3) => -(d.x * dir[0] + d.y * dir[1] + (d.z ?? 0) * dir[2]);
      const r3d = frame.options.results ?? resultsStore.results3D;
      const content = frame.content;
      const opts = frame.options;
      const leftHand = uiStore.axisConvention3D === 'leftHand';
      const labels = createLabelCollector();
      const memberSegs: Array<{ x1: number; y1: number; x2: number; y2: number }> = [];

      const fbData = freeBodyActions3D({
        elements: modelStore.elements, nodes: modelStore.nodes, forces: r3d?.elementForces ?? [], reactions: r3d?.reactions ?? [],
        sections: modelStore.sections, project2D: flat(), leftHand, basis: uiStore.despieceBasis,
        resultant: uiStore.despieceCombineVectors, showReactions: content === 'reactions' || content === 'despiece',
        loads: modelStore.loads, loadText: (v, q) => fmtF(v, q), negligible: 0.02,
      });
      for (const m of fbData.members) {
        const a = toScreen(m.i), b = toScreen(m.j);
        memberSegs.push({ x1: a.x, y1: a.y, x2: b.x, y2: b.y });
      }

      /** The supports, small and plain: a square for a fixed one, a triangle otherwise. */
      const drawSupports = () => {
        for (const s of modelStore.supports.values()) {
          const p = scenePos(s.nodeId); if (!p) continue;
          const c = toScreen(p);
          ctx.fillStyle = '#8fa3b3';
          ctx.beginPath();
          if (String(s.type).startsWith('fixed')) ctx.rect(c.x - 7, c.y + 2, 14, 9);
          else { ctx.moveTo(c.x, c.y + 2); ctx.lineTo(c.x - 8, c.y + 13); ctx.lineTo(c.x + 8, c.y + 13); ctx.closePath(); }
          ctx.fill();
        }
      };
      const drawNumbering = () => {
        if (!opts.numbering) return;
        ctx.font = '10px sans-serif'; ctx.textAlign = 'left'; ctx.textBaseline = 'alphabetic';
        ctx.fillStyle = '#8fa3b3';
        for (const n of modelStore.nodes.values()) { const c = toScreen(projectNodeToScene(n, flat())); ctx.fillText(String(n.id), c.x + 5, c.y - 5); }
        ctx.fillStyle = '#7fb0e2';
        for (const m of fbData.members) {
          const a = toScreen(m.i), b = toScreen(m.j);
          labels.add({ text: `E${m.id}`, colour: '#7fb0e2', font: '10px sans-serif', box: { x: (a.x + b.x) / 2, y: (a.y + b.y) / 2 - 4, width: 0, height: 0, dirX: 0, dirY: -1, priority: 0, anchorX: 'center', sweep: 'any' } });
        }
      };
      /**
       * The shells, farthest first, so a nearer face covers what it hides. `fill` gives each face
       * its colour (a contour's band); without it they are a light tint under the members.
       * `move` gives a corner's position (the deformed figure moves them with the nodes).
       */
      const drawShells = (fill?: (key: string) => string | null, move?: (nodeId: number) => P3 | undefined) => {
        if (frame.options.shells === false && !fill) return;
        const faces = shellFaces().map((f) => {
          const pts = f.nodes.map((id) => move?.(id) ?? scenePos(id)).filter((p): p is P3 => !!p);
          const depth = pts.reduce((a, p) => a + towardViewer(p), 0) / Math.max(1, pts.length);
          return { ...f, pts, depth };
        }).filter((f) => f.pts.length >= 3).sort((a, b) => a.depth - b.depth);
        ctx.save();
        ctx.lineJoin = 'round';
        for (const f of faces) {
          ctx.beginPath();
          f.pts.forEach((p, k) => { const c = toScreen(p); if (k) ctx.lineTo(c.x, c.y); else ctx.moveTo(c.x, c.y); });
          ctx.closePath();
          const colour = fill?.(f.key);
          ctx.fillStyle = colour ?? 'rgba(143, 163, 179, 0.16)';
          ctx.fill();
          ctx.strokeStyle = colour ? 'rgba(60, 70, 80, 0.45)' : 'rgba(143, 163, 179, 0.7)';
          ctx.lineWidth = colour ? 0.5 : 0.8;
          ctx.stroke();
        }
        ctx.restore();
      };
      const drawMembers = (color = '#c7d3dd', width = 2.5, dash: number[] = []) => {
        ctx.save();
        ctx.strokeStyle = color; ctx.lineWidth = width; ctx.lineCap = 'round'; ctx.setLineDash(dash);
        for (const s of memberSegs) { ctx.beginPath(); ctx.moveTo(s.x1, s.y1); ctx.lineTo(s.x2, s.y2); ctx.stroke(); }
        ctx.restore();
      };

      if (content === 'shellMap') {
        drawShellMap(ctx, frame.options.shellComponent ?? 'vonMises', frame.options.style, r3d, shellFaces, drawShells, toScreen, scenePos, opts.values, labels);
        drawMembers('#8fa3b3', 1.4);
        drawSupports();
        labels.flush(ctx, memberSegs);
        return;
      }

      if (content === 'model' || content === 'reactions' || content === 'despiece') {
        // The free body draws its own pieces; the structure figures are its uncut form.
        if (content !== 'despiece') drawShells();
        drawSupports();
        drawFreeBody({
          ctx, toScreen, towardViewer,
          members: fbData.members, nodes: fbData.nodes,
          ends: content === 'despiece' ? fbData.ends : [],
          supports: content === 'model' ? [] : fbData.supports,
          loads: (content === 'despiece' || opts.loads) ? fbData.loads : [],
          mode: opts.dcl ?? 'members',
          sep: content === 'despiece' ? 1 : 0,
          planar: false,
          fmt: (v, q) => fmtF(v, q),
          labels,
        });
        if (content === 'model') drawNumbering();
        labels.flush(ctx, memberSegs);
        return;
      }

      if (content !== 'deformed') drawShells();
      drawSupports();

      if (content === 'deformed' && r3d) {
        drawMembers('#8fa3b3', 1.2, [5, 4]);
        // Scaled so the largest movement reads at about a twentieth of the structure.
        let span = 0, dmax = 0;
        const pts = [...modelStore.nodes.values()].map((n) => projectNodeToScene(n, flat()));
        for (const a of pts) for (const b of pts) span = Math.max(span, Math.hypot(a.x - b.x, a.y - b.y, (a.z ?? 0) - (b.z ?? 0)));
        const disp = new Map(r3d.displacements.map((d) => [d.nodeId, d]));
        for (const d of r3d.displacements) dmax = Math.max(dmax, Math.hypot(d.ux, d.uy, d.uz));
        const scale = dmax > 1e-12 ? (0.05 * span) / dmax : 1;
        drawShells(undefined, (id) => {
          const n = modelStore.nodes.get(id), d = disp.get(id);
          if (!n) return undefined;
          const p = projectNodeToScene(n, flat());
          return d ? { x: p.x + d.ux * scale, y: p.y + d.uy * scale, z: (p.z ?? 0) + d.uz * scale } : p;
        });
        const efMap = new Map(r3d.elementForces.map((f) => [f.elementId, f]));
        ctx.save();
        ctx.strokeStyle = '#e5482a'; ctx.lineWidth = 2.2;
        for (const e of modelStore.elements.values()) {
          const nI = modelStore.nodes.get(e.nodeI), nJ = modelStore.nodes.get(e.nodeJ);
          const dI = disp.get(e.nodeI), dJ = disp.get(e.nodeJ), ef = efMap.get(e.id);
          if (!nI || !nJ || !dI || !dJ || !ef) continue;
          const pI = projectNodeToScene(nI, flat()), pJ = projectNodeToScene(nJ, flat());
          const ei: ElementEI | undefined = eiOf(modelStore.materials.get(e.materialId), modelStore.sections.get(e.sectionId), modelStore.analysis);
          const localY = (e.localYx !== undefined && e.localYy !== undefined && e.localYz !== undefined) ? { x: e.localYx, y: e.localYy, z: e.localYz } : undefined;
          const roll = (e.rollAngle ?? 0) + (modelStore.sections.get(e.sectionId)?.rotation ?? 0);
          const curve = computeDeformedShape3D(
            { x: pI.x, y: pI.y, z: pI.z ?? 0 }, { x: pJ.x, y: pJ.y, z: pJ.z ?? 0 }, dI, dJ, ef, scale, ei, localY, roll || undefined,
          );
          if (curve.length < 2) continue;
          ctx.beginPath();
          curve.forEach((p, k) => { const s = toScreen({ x: p.x, y: p.y, z: p.z }); if (k) ctx.lineTo(s.x, s.y); else ctx.moveTo(s.x, s.y); });
          ctx.stroke();
        }
        ctx.restore();
        if (opts.values) {
          const top = [...r3d.displacements].sort((a, b) => Math.hypot(b.ux, b.uy, b.uz) - Math.hypot(a.ux, a.uy, a.uz)).slice(0, 4);
          for (const d of top) {
            const n = modelStore.nodes.get(d.nodeId); if (!n) continue;
            const p = projectNodeToScene(n, flat());
            const s = toScreen({ x: p.x + d.ux * scale, y: p.y + d.uy * scale, z: (p.z ?? 0) + d.uz * scale });
            labels.add({ text: displacementText(Math.hypot(d.ux, d.uy, d.uz), 2, uiStore.unitSystem), colour: '#7fd4cc', font: 'bold 10.5px sans-serif', box: { x: s.x + 6, y: s.y - 6, width: 0, height: 0, dirX: 0, dirY: -1, priority: Math.hypot(d.ux, d.uy, d.uz), anchorX: 'left', sweep: 'any' } });
          }
        }
        labels.flush(ctx, memberSegs);
        return;
      }

      if ((content === 'moment' || content === 'shear' || content === 'axial') && r3d) {
        drawMembers();
        const kinds = components3D(content, r3d);
        // Every member's diagram on one scale per component, standing about 45 px off the member.
        for (const kind of kinds) {
          const gmax = computeGlobalMax3D(r3d.elementForces, kind);
          if (!(gmax > 1e-9)) continue;
          const pxPerUnit = 45 / gmax;
          const col = COLORS[kind] ?? COLORS.axial!;
          const perpDir = getDiagramLocalDirection(kind);
          const isMoment = kind === 'momentY' || kind === 'momentZ';
          for (const e of modelStore.elements.values()) {
            const ef = r3d.elementForces.find((f) => f.elementId === e.id);
            const pI = scenePos(e.nodeI), pJ = scenePos(e.nodeJ);
            if (!ef || !pI || !pJ) continue;
            const localY = (e.localYx !== undefined && e.localYy !== undefined && e.localYz !== undefined) ? { x: e.localYx, y: e.localYy, z: e.localYz } : undefined;
            const roll = (e.rollAngle ?? 0) + (modelStore.sections.get(e.sectionId)?.rotation ?? 0);
            let axes;
            try { axes = computeLocalAxes3D({ id: 0, x: pI.x, y: pI.y, z: pI.z ?? 0 }, { id: 0, x: pJ.x, y: pJ.y, z: pJ.z ?? 0 }, localY, roll || undefined, leftHand); } catch { continue; }
            const { perpVec, sign } = computeDiagramDisplayDirection(axes, perpDir, resultsStore.drawPositiveTowardLocalAxes);
            const dg = computeDiagram3D(ef, kind);
            // The offset in world units that stands `pxPerUnit` px per kN on the page.
            const sI = toScreen(pI), probe = toScreen({ x: pI.x + perpVec.x, y: pI.y + perpVec.y, z: (pI.z ?? 0) + perpVec.z });
            const pxPerMetre = Math.hypot(probe.x - sI.x, probe.y - sI.y);
            if (pxPerMetre < 1e-6) continue;
            const k = pxPerUnit / pxPerMetre;
            const base = (t: number): P3 => ({ x: pI.x + (pJ.x - pI.x) * t, y: pI.y + (pJ.y - pI.y) * t, z: (pI.z ?? 0) + ((pJ.z ?? 0) - (pI.z ?? 0)) * t });
            const tip = (t: number, v: number): P3 => { const b = base(t); const o = sign * v * k; return { x: b.x + perpVec.x * o, y: b.y + perpVec.y * o, z: (b.z ?? 0) + perpVec.z * o }; };
            ctx.beginPath();
            const b0 = toScreen(base(0)); ctx.moveTo(b0.x, b0.y);
            for (const p of dg.points) { const s = toScreen(tip(p.t, p.value)); ctx.lineTo(s.x, s.y); }
            const b1 = toScreen(base(1)); ctx.lineTo(b1.x, b1.y); ctx.closePath();
            ctx.fillStyle = col.fill; ctx.fill();
            ctx.beginPath();
            dg.points.forEach((p, i) => { const s = toScreen(tip(p.t, p.value)); if (i) ctx.lineTo(s.x, s.y); else ctx.moveTo(s.x, s.y); });
            ctx.strokeStyle = col.stroke; ctx.lineWidth = 1.4; ctx.stroke();
            if (opts.values && Math.abs(dg.maxAbsValue) > 0.02 * gmax) {
              const at = dg.points.find((p) => p.value === dg.maxAbsValue) ?? dg.points[0]!;
              const s = toScreen(tip(at.t, at.value)), sb = toScreen(base(at.t));
              const dx = s.x - sb.x, dy = s.y - sb.y, l = Math.hypot(dx, dy) || 1;
              const shown = isMoment ? -dg.maxAbsValue : dg.maxAbsValue;
              labels.add({ text: `${SYM[kind]} ${fmtF(shown, isMoment ? 'moment' : 'force')}`, colour: col.stroke, font: 'bold 10.5px sans-serif', box: { x: s.x + (dx / l) * 8, y: s.y + (dy / l) * 8 + 4, width: 0, height: 0, dirX: dx / l, dirY: dy / l, priority: Math.abs(dg.maxAbsValue), anchorX: 'center', sweep: 'any' } });
            }
          }
        }
        labels.flush(ctx, memberSegs);
      }
    },
  };
}

/** A value's colour on the figure: the screen's contour ramp, or a grey ramp in black and white. */
function bandColour(v: number, min: number, max: number, style: string): string {
  if (style === 'bw') {
    const t = max > min ? (v - min) / (max - min) : 0.5;
    const g = Math.round(235 - 175 * Math.max(0, Math.min(1, t)));
    return `${RAW_INK}rgb(${g},${g},${g})`;
  }
  return `${RAW_INK}#${shellContourColor(v, min, max).toString(16).padStart(6, '0')}`;
}

/**
 * The shells painted by one result, element by element (each its own value, as the table gives
 * it), with the scale beside the structure: its colours, the range's ends and the component.
 * The largest absolute value is written on its element when values are on.
 */
function drawShellMap(
  ctx: CanvasRenderingContext2D,
  component: ShellContourComponent,
  style: string,
  r3d: AnalysisResults3D | null | undefined,
  shellFaces: () => Array<{ key: string; id: number; nodes: number[] }>,
  drawShells: (fill?: (key: string) => string | null) => void,
  toScreen: (p: P3) => { x: number; y: number },
  scenePos: (id: number) => P3 | undefined,
  values: boolean,
  labels: ReturnType<typeof createLabelCollector>,
): void {
  const value = new Map<string, number>();
  for (const s of r3d?.plateStresses ?? []) value.set(`p${s.elementId}`, shellComponentValue(s, component));
  for (const s of r3d?.quadStresses ?? []) value.set(`q${s.elementId}`, shellComponentValue(s, component));
  let min = Infinity, max = -Infinity;
  for (const v of value.values()) { if (v < min) min = v; if (v > max) max = v; }
  if (!Number.isFinite(min)) { drawShells(); return; }
  drawShells((key) => {
    const v = value.get(key);
    return v === undefined ? null : bandColour(v, min, max, style);
  });

  // Where the structure lands on the page, to put the scale to its right.
  let x1 = -Infinity, y0 = Infinity, y1 = -Infinity;
  for (const f of shellFaces()) for (const id of f.nodes) {
    const p = scenePos(id); if (!p) continue;
    const c = toScreen(p);
    x1 = Math.max(x1, c.x); y0 = Math.min(y0, c.y); y1 = Math.max(y1, c.y);
  }
  if (!Number.isFinite(x1)) return;
  const meta = shellComponentMeta(component);
  const fmt = (v: number) => (Math.abs(v) >= 1000 || (Math.abs(v) < 0.01 && v !== 0) ? v.toExponential(2) : v.toFixed(Math.abs(v) >= 100 ? 0 : 2));
  const h = Math.max(120, Math.min(260, y1 - y0)), w = 14, x = x1 + 36, top = (y0 + y1) / 2 - h / 2;
  const steps = 24;
  ctx.save();
  for (let k = 0; k < steps; k++) {
    const v = max - ((max - min) * (k + 0.5)) / steps;
    ctx.fillStyle = bandColour(v, min, max, style);
    ctx.fillRect(x, top + (h * k) / steps, w, h / steps + 0.5);
  }
  ctx.strokeStyle = '#8fa3b3'; ctx.lineWidth = 0.8; ctx.strokeRect(x, top, w, h);
  ctx.fillStyle = '#c7d3dd'; ctx.font = '10.5px sans-serif'; ctx.textAlign = 'left'; ctx.textBaseline = 'middle';
  if (max - min > 1e-9 * Math.max(1, Math.abs(max))) {
    ctx.fillText(fmt(max), x + w + 6, top);
    ctx.fillText(fmt((max + min) / 2), x + w + 6, top + h / 2);
    ctx.fillText(fmt(min), x + w + 6, top + h);
  } else {
    // One value over every element: the scale says so rather than spreading it over a range.
    ctx.fillText(fmt(max), x + w + 6, top + h / 2);
  }
  ctx.textBaseline = 'alphabetic';
  ctx.fillText(`${meta.label} [${meta.unit}]`, x, top - 10);
  ctx.restore();

  if (values) {
    let best: string | null = null, peak = -1;
    for (const [k, v] of value) if (Math.abs(v) > peak) { peak = Math.abs(v); best = k; }
    const f = best ? shellFaces().find((s) => s.key === best) : undefined;
    const pts = f?.nodes.map(scenePos).filter((p): p is P3 => !!p) ?? [];
    if (f && pts.length) {
      const c = toScreen({ x: pts.reduce((a, p) => a + p.x, 0) / pts.length, y: pts.reduce((a, p) => a + p.y, 0) / pts.length, z: pts.reduce((a, p) => a + (p.z ?? 0), 0) / pts.length });
      labels.add({ text: fmt(value.get(best!)!), colour: '#f4f7fa', font: 'bold 10.5px sans-serif', box: { x: c.x, y: c.y - 6, width: 0, height: 0, dirX: 0, dirY: -1, priority: peak, anchorX: 'center', sweep: 'any' } });
    }
  }
}
