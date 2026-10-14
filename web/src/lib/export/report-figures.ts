/**
 * The calculation report's diagrams, drawn before the report is written: inline SVG, the same
 * figures "Exportar vista" makes, in 2D and in 3D.
 */
import { figuresWanted, type CalcReportContent, type ReportFigure } from '../engine/calc-report';
import { t } from '../i18n';
import { NEEDS_RESULTS, figureTitle, plan2D, toSvg, type FigureOptions } from './view-figure';

export async function reportFigures(content: CalcReportContent, solved: boolean, options: FigureOptions): Promise<ReportFigure[]> {
  const wanted = figuresWanted(content).filter((c) => solved || !NEEDS_RESULTS.has(c));
  // The caption under each figure says what it is; a heading inside it would say it twice.
  const opts: FigureOptions = { ...options, titles: false };
  const out: ReportFigure[] = [];
  for (const c of wanted) {
    // The loads section's figure is the model with its loads, numbered as the tables are.
    const p = plan2D(c, c === 'model' ? { ...opts, loads: true, numbering: true } : opts);
    if (p) out.push({ content: c, title: c === 'model' ? t('figures.modelLoaded') : figureTitle(c), svg: toSvg(p) });
  }
  return out;
}
