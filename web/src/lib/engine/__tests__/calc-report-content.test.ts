import { describe, it, expect } from 'vitest';
import { generateCalcReportHtml, DEFAULT_REPORT_CONTENT, figuresWanted, type CalcReportData, type CalcReportContent } from '../calc-report';
import type { KinematicReport } from '../kinematic-report';

/** A 4 m simply supported beam under 10 kN/m. */
function beam(overrides: Partial<CalcReportData> = {}): CalcReportData {
  return {
    config: { projectName: 'Viga', engineerName: '', companyName: '', date: '1 Jan 2026', notes: '' },
    is3D: false, analysisMode: '2D', provenance: { kind: 'single' }, hasDesignChecks: false,
    nodes: [{ id: 1, x: 0, y: 0 }, { id: 2, x: 4, y: 0 }] as never,
    elements: [], materials: [], sections: [], supports: [], loads: [], loadCases: [], combinations: [],
    results2D: {
      reactions: [{ nodeId: 1, rx: 0, rz: 20, my: 0 }, { nodeId: 2, rx: 0, rz: 20, my: 0 }],
      displacements: [{ nodeId: 1, ux: 0, uz: 0 }, { nodeId: 2, ux: 0, uz: 0 }],
      elementForces: [{
        elementId: 1, length: 4, nStart: 0, nEnd: 0, vStart: 20, vEnd: -20, mStart: 0, mEnd: 0, qI: -10, qJ: -10, pointLoads: [],
      }],
    } as never,
    ...overrides,
  };
}

const content = (patch: Partial<CalcReportContent> = {}): CalcReportContent => ({
  ...structuredClone(DEFAULT_REPORT_CONTENT), ...patch,
});

const h1s = (html: string) => [...html.matchAll(/<h1[^>]*>(.*?)<\/h1>/g)].map((m) => m[1]);

describe('the report the reader chose', () => {
  it('numbers only the sections that go in', () => {
    const c = content({ modelData: false, loads: false, kinematics: false, despiece: false });
    c.rows.reactions = { table: false, diagram: false };
    c.rows.displacements = { table: false, diagram: false };
    c.rows.shear = { table: false, diagram: false };
    c.rows.axial = { table: false, diagram: false };
    const html = generateCalcReportHtml(beam({ content: c }));
    expect(h1s(html).filter((s) => /^\d\./.test(s!))).toEqual(['1. Internal Forces']);
  });

  it('gives the moment along the member: the ends and the largest, where it is', () => {
    const c = content();
    c.rows.moment = { table: true, diagram: false };
    const html = generateCalcReportHtml(beam({ content: c }));
    // wL²/8 = 20 kN·m at mid-span, sagging.
    expect(html).toMatch(/<td>1<\/td><td class="num">0<\/td><td class="num">0<\/td><td class="num">20\.00<\/td><td class="num">2\.00<\/td>/);
  });

  it('puts each diagram in its place, numbered after the model figure', () => {
    const c = content();
    const html = generateCalcReportHtml(beam({
      content: c,
      figures: [{ content: 'moment', title: 'Bending moment M', svg: '<?xml version="1.0"?><svg id="m"></svg>' }],
    }));
    expect(html).toContain('<svg id="m"></svg>');
    expect(html).not.toContain('<?xml');
    expect(html).toContain('Figure 2. Bending moment M');
  });

  it('asks for the figures of what was chosen', () => {
    const c = content({ despiece: true });
    expect(figuresWanted(c)).toEqual(['model', 'despiece', 'deformed', 'moment', 'shear', 'axial']);
  });

  it('writes the kinematic analysis: the count, the supports and the classification', () => {
    const k = {
      formula: 'g = 3·m + r − 3·n', substitution: 'g = 3×1 + 3 − 3×2 = 0', classificationText: 'Isostatic',
      totalR: 3, totalC: 0, supportDetails: [{ nodeId: 1, type: 'Pinned', dofs: 2, restrainedDofs: 'ux, uz' }],
      hingeDetails: [], slideDetails: [], hasHiddenMechanism: false, mechanismModes: 0, unconstrainedDofs: [],
      suggestions: [], rankChecked: true,
    } as unknown as KinematicReport;
    const html = generateCalcReportHtml(beam({ content: content(), kinematics: k }));
    expect(html).toContain('g = 3×1 + 3 − 3×2 = 0');
    expect(html).toContain('Isostatic');
    expect(html).toContain('<td>1</td><td>Pinned</td><td>ux, uz</td><td class="num">2</td>');
  });

  it('without results says why, and still gives the model and the kinematics', () => {
    const html = generateCalcReportHtml(beam({ content: content(), results2D: undefined, solveError: 'At least 1 support is needed', kinematics: null }));
    expect(html).toContain('At least 1 support is needed');
    expect(h1s(html)).toContain('3. Kinematic analysis');
    expect(html).not.toContain('Support reactions</h1>');
  });
});
