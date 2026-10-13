/**
 * "Exportar vista": the chosen figures as SVG or PNG files, or a picture on the clipboard.
 *
 * Figures that need results solve the model first (`ensureResults`); when it cannot be solved,
 * the figures that do not need results still come out and the reason is told. In 2D the figures
 * are drawn as vectors (`view-figure.ts`); in 3D they are the 3D view, captured, so only PNG.
 */
import { zipSync } from 'fflate';
import { modelStore, uiStore } from '../store';
import { t, tp } from '../i18n';
import { ensureResults } from '../actions/solve';
import { downloadBlob } from '../store/file';
import {
  NEEDS_RESULTS, canDrawVector, capture3D, canvasToBlob, plan2D, sheetOf, stackPngs, toCanvas, toSvg,
  type FigureContent, type FigureOptions, type FigurePlan,
} from './view-figure';

export type ViewFormat = 'svg' | 'png';

export interface ViewExportRequest {
  format: ViewFormat;
  contents: FigureContent[];
  options: FigureOptions;
  /** All the figures on one sheet, rather than one file each. */
  sheet: boolean;
}

/** The project's name as a file name. */
export function fileBase(): string {
  return (modelStore.model.name || t('file.defaultStructure')).replace(/[^a-zA-Z0-9áéíóúñÁÉÍÓÚÑ _-]/g, '').trim() || 'stabileo';
}

const SLUG: Record<FigureContent, string> = {
  model: 'estructura', reactions: 'reacciones', despiece: 'dcl', moment: 'M', shear: 'V', axial: 'N', deformed: 'deformada',
};

/**
 * Solve if a chosen figure needs results, and keep the figures that can be drawn. A failed solve
 * leaves out the result figures and says why.
 */
async function drawable(contents: FigureContent[]): Promise<FigureContent[]> {
  if (!contents.some((c) => NEEDS_RESULTS.has(c))) return contents;
  const solved = await ensureResults();
  if (solved.ok) return contents;
  const kept = contents.filter((c) => !NEEDS_RESULTS.has(c));
  uiStore.toast(tp(kept.length ? 'viewExport.partial' : 'viewExport.unsolved', { error: solved.error }), 'error');
  return kept;
}

/** The 2D plans for the request: one per figure, or one sheet. */
function plans(contents: FigureContent[], req: ViewExportRequest): FigurePlan[] {
  const list = contents.map((c) => plan2D(c, req.options)).filter((p): p is FigurePlan => p !== null);
  if (req.sheet && list.length > 1) return [sheetOf(list, req.options.style)];
  return list;
}

async function dataUrlBlob(url: string): Promise<Blob> {
  return (await fetch(url)).blob();
}

/** The files the request makes: name and content. */
async function files(req: ViewExportRequest): Promise<Array<{ name: string; blob: Blob }>> {
  const contents = await drawable(req.contents);
  if (contents.length === 0) return [];
  const base = fileBase();
  if (!canDrawVector()) {
    // 3D: the view, captured.
    const shots = await capture3D(contents, req.options);
    if (req.sheet && shots.length > 1) {
      const blob = await stackPngs(shots.map((s) => s.dataUrl));
      return blob ? [{ name: `${base}_${t('viewExport.sheetFile')}.png`, blob }] : [];
    }
    return Promise.all(shots.map(async (s) => ({ name: `${base}_${SLUG[s.content]}.png`, blob: await dataUrlBlob(s.dataUrl) })));
  }
  const ps = plans(contents, req);
  const out: Array<{ name: string; blob: Blob }> = [];
  for (const p of ps) {
    const name = ps.length === 1 && (req.sheet || contents.length > 1) ? `${base}_${t('viewExport.sheetFile')}` : `${base}_${SLUG[p.content]}`;
    if (req.format === 'svg') out.push({ name: `${name}.svg`, blob: new Blob([toSvg(p)], { type: 'image/svg+xml' }) });
    else {
      const blob = await canvasToBlob(toCanvas(p));
      if (blob) out.push({ name: `${name}.png`, blob });
    }
  }
  return out;
}

/** Download what the request makes: one file, or a zip of several. */
export async function exportView(req: ViewExportRequest): Promise<boolean> {
  const fs = await files(req);
  if (fs.length === 0) return false;
  if (fs.length === 1) { downloadBlob(fs[0]!.blob, fs[0]!.name); return true; }
  const entries: Record<string, Uint8Array> = {};
  for (const f of fs) entries[f.name] = new Uint8Array(await f.blob.arrayBuffer());
  downloadBlob(new Blob([zipSync(entries, { level: 0 }) as Uint8Array<ArrayBuffer>], { type: 'application/zip' }), `${fileBase()}_${t('viewExport.figuresFile')}.zip`);
  return true;
}

/**
 * Put the figures on the clipboard as one picture (several go on one sheet), to paste into a
 * document. A PNG: the format every editor accepts from the clipboard.
 */
export async function copyView(req: ViewExportRequest): Promise<boolean> {
  if (typeof ClipboardItem === 'undefined' || !navigator.clipboard?.write) {
    uiStore.toast(t('viewExport.noClipboard'), 'error');
    return false;
  }
  // The clipboard write has to start inside the click, so it is handed a promise of the picture.
  const picture = (async () => {
    const contents = await drawable(req.contents);
    if (contents.length === 0) throw new Error('empty');
    if (!canDrawVector()) {
      const shots = await capture3D(contents, req.options);
      if (!shots[0]) throw new Error('empty');
      const blob = shots.length > 1 ? await stackPngs(shots.map((s) => s.dataUrl)) : await dataUrlBlob(shots[0].dataUrl);
      if (!blob) throw new Error('empty');
      return blob;
    }
    const ps = plans(contents, { ...req, sheet: true });
    const blob = ps[0] ? await canvasToBlob(toCanvas(ps[0])) : null;
    if (!blob) throw new Error('empty');
    return blob;
  })();
  try {
    await navigator.clipboard.write([new ClipboardItem({ 'image/png': picture })]);
    return true;
  } catch {
    uiStore.toast(t('viewExport.copyFailed'), 'error');
    return false;
  }
}
