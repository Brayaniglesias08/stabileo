/**
 * Basic's Excel and CSV exports: solved first when the model has no results yet.
 *
 * A workbook asked for before Calcular came out with the model and no results, and no word on
 * why. Now the export solves; if the solve fails, the file still comes out, the summary sheet
 * (or the CSV's head) gives the reason, and each results sheet points to it.
 */
import { uiStore } from '../store';
import { t } from '../i18n';
import { ensureResults } from '../actions/solve';
import { downloadExcel, downloadResultsCSV } from '../store/file';

export async function exportWorkbook(): Promise<void> {
  const solved = await ensureResults();
  await downloadExcel(solved.ok ? {} : { solveError: solved.error });
  if (!solved.ok) uiStore.toast(t('exportResults.withError'), 'info');
}

export async function exportResultsCsv(): Promise<void> {
  const solved = await ensureResults();
  downloadResultsCSV(solved.ok ? undefined : solved.error);
  if (!solved.ok) uiStore.toast(t('exportResults.withErrorCsv'), 'info');
}
