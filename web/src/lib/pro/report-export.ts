/**
 * Turning the report dialog's choices into a document.
 *
 * ── Why this is not in `ProPanel` ──────────────────────────────────
 *
 * It was, and the 600-line ceiling on that component is what said it should
 * not be. The panel is a layout shell: which tab is showing, how wide it is,
 * what the header says. Assembling a report — drawing its figures, gathering
 * verifications and advanced results — is a job, and a job in a layout shell is
 * how a layout shell stops being one.
 *
 * The report is the printable document only: the same data as a spreadsheet is
 * Project › Export › Excel, which chooses its own sheets.
 *
 * The figures are drawn from the model (`export/view-figure.ts`), the vector
 * figures "Exportar imagen" makes. The screen is captured only when they cannot
 * be drawn, and that capture is taken HERE rather than in `pro-report-inputs.ts`
 * because it is a reading of the DOM at the instant the button was pressed. A
 * tainted canvas throws on `toDataURL`; the report goes out without the picture
 * rather than not going out.
 */

import { captureProvenanceToken, provenanceForExport, provenanceLabels, ExportChangedError } from '../store/result-provenance';
import { runEngineActionAsync } from '../store/engine-action';
import { tick } from 'svelte';
import { viewportCanvas } from '../utils/viewport-canvas';
import { uiStore } from '../store/ui.svelte';
import { viewState } from '../store/view-state.svelte';
import { buildProReportData } from '../engine/pro-report-inputs';
import { openReport } from '../engine/pro-report';
import type { ReportConfig, ReportData, ReportResult, ReportResultFigures } from '../engine/pro-report';
import type { ReportFigure } from '../engine/report/project-sections';
import { DEFAULT_FIGURE_OPTIONS, figureTitle, plan2D, toSvg, type FigureContent, type FigureOptions, type FigureView } from '../export/view-figure';
import type { InkStyle } from '../export/paper-ink';
import type { ShellContourComponent } from '../engine/shell-stress';
import { t as tr } from '../i18n';
import { modelStore, resultsStore } from '../store';
import { verificationStore } from '../store/verification.svelte';
import { activePerCombo3D, activeCombinations } from '../store/active-results';
import { computeStationDemands, runUnifiedVerification } from '../engine/verification-service';
import type { ElementVerification } from '../engine/codes/argentina/cirsoc201';

export interface ReportExportInputs {
  config: ReportConfig;
  verifications: ReportData['verifications'];
  advancedResults: Record<string, unknown>;
  t: (key: string) => string;
  /** How the figures are drawn; without it the report captures the screen for the model. */
  figureOptions?: ReportFigureChoices;
}

/** The canvas as it is on screen, or nothing if the browser refuses it. */
function screenshotOfCanvas(): string | undefined {
  // The viewport's canvas, not the first canvas in the document: a panel that draws one (a chart,
  // a section preview) would otherwise put its picture in the report as "the model".
  const canvas = viewportCanvas();
  if (!canvas) return undefined;
  try { return (canvas as HTMLCanvasElement).toDataURL('image/png'); } catch { return undefined; }
}

/** The next painted frame. */
const frame = () => new Promise<void>((resolve) => requestAnimationFrame(() => resolve()));

/**
 * The canvas with every node and member number on, whatever the reader shows: the report's tables
 * name nodes and members by number, and the picture is where a reader finds them. What the reader
 * had on is put back after the picture, also when it fails.
 */
async function screenshotWithNumbers(): Promise<string | undefined> {
  const had = { nodes: uiStore.showNodeLabels3D, members: uiStore.showElementLabels3D, label: viewState.memberLabel, onSelection: viewState.labelsOnSelection };
  const changes = !had.nodes || !had.members || had.label !== 'id' || had.onSelection;
  if (!changes) return screenshotOfCanvas();
  try {
    uiStore.showNodeLabels3D = true;
    uiStore.showElementLabels3D = true;
    viewState.memberLabel = 'id';
    viewState.labelsOnSelection = false;
    await tick();
    // The viewport redraws on the frame after it is invalidated; two leave room for the labels.
    await frame(); await frame(); await frame();
    return screenshotOfCanvas();
  } finally {
    uiStore.showNodeLabels3D = had.nodes;
    uiStore.showElementLabels3D = had.members;
    viewState.memberLabel = had.label;
    viewState.labelsOnSelection = had.onSelection;
  }
}

/**
 * The concrete design as the Design panel verified it, member by member, for the report.
 *
 * The report used to re-design every member with the older checker: bars of its own choosing,
 * a linear P-M interaction, fy 420, 25 mm cover and Ø8 stirrups hard-wired, 2005 clause
 * numbers, so the document described a different design from the panel's. It now prints what
 * the panel checked: the member's own reinforcement against the adapter the project binds.
 */
export function reportDesignChecks(): NonNullable<ReportData['designChecks']> {
  const out: NonNullable<ReportData['designChecks']> = [];
  for (const [id, ctx] of verificationStore.contexts) {
    const pv = verificationStore.providedFor(id);
    if (!pv || !pv.hasProvided) continue;
    const gov = [...pv.checks].sort((a, b) => b.ratio - a.ratio)[0];
    out.push({
      elementId: id, elementType: pv.elementType, section: ctx.section.name,
      status: pv.overallStatus, worstUtilization: pv.worstUtilization, checks: pv.strengthCheckCount,
      ...(gov ? { governing: gov.category, demand: gov.demand, capacity: gov.capacity, unit: gov.unit, comboName: gov.comboName } : {}),
    });
  }
  return out.sort((a, b) => a.elementId - b.elementId);
}

/** Build and open the report; an engine error is said (an error toast), never swallowed. */
export async function exportReportAs(input: ReportExportInputs): Promise<void> {
  // What the report is taken from, checked again before its provenance is written.
  const token = captureProvenanceToken();
  await runEngineActionAsync(() => exportReportWithEngine(input, token));
}

async function exportReportWithEngine(input: ReportExportInputs, token: ReturnType<typeof captureProvenanceToken>): Promise<void> {
  const figures = input.figureOptions ? reportResultFigures(input.config, input.figureOptions) : {};
  // The model's figure is drawn; a capture of the screen only when it cannot be.
  const capture = input.config.sections?.modelData !== false && !figures.model;
  const data = buildProReportData({
    config: input.config,
    verifications: [],
    advancedResults: Object.keys(input.advancedResults).length > 0
      ? input.advancedResults as ReportData['advancedResults']
      : undefined,
    screenshot: capture ? await screenshotWithNumbers() : undefined,
    t: input.t,
  });
  if (!data) return;
  data.resultFigures = figures;
  data.designChecks = reportDesignChecks();
  try {
    data.resultProvenance = { record: await provenanceForExport(token), labels: provenanceLabels() };
  } catch (e) {
    if (e instanceof ExportChangedError) { uiStore.toast(e.message, 'error'); return; }
    throw e;
  }
  openReport(data);
}

/** How the report's figures are drawn: the dialog's choices. */
export interface ReportFigureChoices { style: InkStyle; values: boolean; view: FigureView; shellComponent: ShellContourComponent }

/** A figure as the report embeds it: an SVG in a data URL, with its caption. */
function asReportFigure(content: FigureContent, o: FigureOptions, caption: string): ReportFigure | undefined {
  const plan = plan2D(content, o);
  return plan ? { dataUrl: `data:image/svg+xml;charset=utf-8,${encodeURIComponent(toSvg(plan))}`, caption } : undefined;
}

/**
 * The report's figures, drawn from the model like "Exportar imagen" draws them (`view-figure.ts`):
 * the numbered model, the loaded model under the loads, and for each result printed as a figure
 * its figures (reactions; the deformed shape; M, V and N; the shells' result).
 */
export function reportResultFigures(config: ReportConfig, c: ReportFigureChoices): ReportResultFigures {
  const s = config.sections;
  const o: FigureOptions = { ...DEFAULT_FIGURE_OPTIONS, style: c.style, values: c.values, titles: false, view: c.view, shells: true, shellComponent: c.shellComponent };
  const out: ReportResultFigures = {};
  if (s.modelData) {
    out.model = asReportFigure('model', { ...o, loads: false, numbering: true }, tr('report.modelFigureCaption'));
    if (s.loads) out.loads = asReportFigure('model', { ...o, loads: true, numbering: false }, tr('figures.modelLoaded'));
  }
  if (!s.results || !resultsStore.results3D) return out;
  const wants = (k: ReportResult) => (config.resultModes?.[k] ?? 'both') !== 'table';
  const many = (contents: FigureContent[]) => contents.map((f) => asReportFigure(f, o, figureTitle(f, o))).filter((f): f is ReportFigure => !!f);
  if (wants('reactions')) out.reactions = many(['reactions']);
  if (wants('displacements')) out.displacements = many(['deformed']);
  if (wants('forces')) out.forces = many(['moment', 'shear', 'axial']);
  if (wants('shells') && modelStore.plates.size + modelStore.quads.size > 0) out.shells = many(['shellMap']);
  return out;
}

/**
 * The concrete verification the report prints, against the model as it is now.
 *
 * Always re-run, not taken from the verification tab: a prior run may describe a since-edited
 * model. And kept to the report: it used to be written to `verificationStore`, replacing the
 * tab's own results — the ones its colour map draws — with this pass.
 */
export function reportVerification(): ElementVerification[] {
  const results = resultsStore.results3D;
  if (!results) return [];
  const md = { elements: modelStore.elements, nodes: modelStore.nodes, sections: modelStore.sections, materials: modelStore.materials, supports: modelStore.supports };
  const stationData = resultsStore.hasCombinations3D
    ? computeStationDemands(activePerCombo3D(), activeCombinations(), md)
    : undefined;
  return runUnifiedVerification(results, md, resultsStore.governing3D.size > 0 ? resultsStore.governing3D : null, stationData?.demands);
}
