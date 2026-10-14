<script lang="ts">
  /**
   * Excel and CSV: the project's data, chosen sheet by sheet.
   *
   * Both buttons open this dialog, with their format chosen: the content is one choice and the
   * file another. Everything is on by default; what is turned off is remembered in this browser,
   * so a sheet added later starts on.
   *
   * "Puntos a lo largo de las barras" is what the panel's "Estaciones 5 / 13 / Críticas" was:
   * how many points of each member the station sheets read. It is offered only when one of those
   * sheets is chosen, and says which they are.
   */
  import { t, tp } from '../../../lib/i18n';
  import { modelStore, resultsStore, uiStore } from '../../../lib/store';
  import ExportDialog from '../../export/ExportDialog.svelte';
  import ExportSection from '../../export/ExportSection.svelte';
  import { MODEL_SHEET_NAMES } from '../../../lib/export/workbook-model';
  import { RESULT_SHEET_NAMES } from '../../../lib/export/workbook-results';
  import { downloadProjectCsv, downloadProjectWorkbook } from '../../../lib/store/project-workbook';
  import type { StationSpec } from '../../../lib/engine/station-forces';
  import { ensureProResults, hasShells } from '../../../lib/pro/export-results';

  let { open = $bindable(false), format = $bindable('xlsx') }: { open: boolean; format: 'xlsx' | 'csv' } = $props();

  const MODEL = [...MODEL_SHEET_NAMES, 'Specifications'];
  const RESULTS = [...RESULT_SHEET_NAMES];
  const SHELL_RESULTS = new Set(['ShellCentres', 'ShellNodes', 'ShellCorners']);
  /** The sheets read at points along the members. */
  const AT_STATIONS = new Set(['Stations', 'Deflections', 'Stresses', 'Maxima']);

  const KEY = 'stabileo-pro-data-export';
  interface Prefs { off: string[]; stations: StationSpec }
  function load(): Prefs {
    try {
      const p = JSON.parse(localStorage.getItem(KEY) ?? '{}') as Partial<Prefs>;
      const st = p.stations;
      return { off: Array.isArray(p.off) ? p.off : [], stations: st === 'critical' || st === 13 || st === 5 ? st : 5 };
    } catch { return { off: [], stations: 5 }; }
  }
  const prefs = load();
  let off = $state(new Set<string>(prefs.off));
  let stations = $state<StationSpec>(prefs.stations);
  /** Cases and combinations left out; none, by default and whenever the dialog opens. */
  let casesOff = $state(new Set<number>());
  let combosOff = $state(new Set<number>());
  let busy = $state(false);

  function save() {
    try { localStorage.setItem(KEY, JSON.stringify({ off: [...off], stations })); } catch { /* this session only */ }
  }
  $effect(() => { if (open) { casesOff = new Set(); combosOff = new Set(); } });

  const shells = $derived(open && hasShells());
  const modelShown = $derived(MODEL.filter((n) => n !== 'Shells' || shells));
  const resultsShown = $derived(RESULTS.filter((n) => !SHELL_RESULTS.has(n) || shells));
  const on = (n: string) => !off.has(n);
  function flip(n: string) { const s = new Set(off); if (s.has(n)) s.delete(n); else s.add(n); off = s; save(); }
  function setAll(names: string[], value: boolean) { const s = new Set(off); for (const n of names) { if (value) s.delete(n); else s.add(n); } off = s; save(); }

  const cases = $derived(open ? modelStore.model.loadCases : []);
  const combos = $derived(open ? modelStore.combinations : []);
  function flipId(set: 'cases' | 'combos', id: number) {
    const s = new Set(set === 'cases' ? casesOff : combosOff);
    if (s.has(id)) s.delete(id); else s.add(id);
    if (set === 'cases') casesOff = s; else combosOff = s;
  }

  const modelCount = $derived(modelShown.filter(on).length);
  const resultCount = $derived(resultsShown.filter(on).length);
  const wantsResults = $derived(resultCount > 0);
  const wantsStations = $derived(resultsShown.some((n) => AT_STATIONS.has(n) && on(n)));

  const contentSummary = $derived(tp('proExport.dataContentSummary', { m: modelCount, mt: modelShown.length, r: resultCount, rt: resultsShown.length }));
  const setsSummary = $derived(casesOff.size === 0 && combosOff.size === 0
    ? t('proExport.setsAll')
    : tp('proExport.setsSome', { c: cases.length - casesOff.size, k: combos.length - combosOff.size }));
  const STATIONS: Array<{ id: StationSpec; key: string }> = [
    { id: 5, key: 'proExport.stations5' }, { id: 13, key: 'proExport.stations13' }, { id: 'critical', key: 'proExport.stationsCritical' },
  ];
  const stationsSummary = $derived(t(STATIONS.find((s) => s.id === stations)!.key));
  const formatSummary = $derived(format === 'xlsx' ? t('proExport.formatXlsx') : t('proExport.formatCsv'));

  async function run() {
    if (busy || modelCount + resultCount === 0) return;
    busy = true;
    try {
      let results = wantsResults;
      if (results && !(await ensureProResults())) {
        results = false;
        uiStore.toast(t('proExport.dataNoResults'), 'info');
      }
      const sheets = new Set([...modelShown, ...(results ? resultsShown : [])].filter(on));
      const opts = {
        results, sheets,
        sources: {
          cases: casesOff.size ? new Set(cases.map((c) => c.id).filter((id) => !casesOff.has(id))) : undefined,
          combinations: combosOff.size ? new Set(combos.map((c) => c.id).filter((id) => !combosOff.has(id))) : undefined,
        },
      };
      if (format === 'xlsx') await downloadProjectWorkbook(stations, opts);
      else downloadProjectCsv(stations, opts);
      open = false;
    } finally {
      busy = false;
    }
  }
</script>

<ExportDialog bind:open title={format === 'xlsx' ? t('proExport.dataTitleXlsx') : t('proExport.dataTitleCsv')} testid="pro-data-export-dialog">
  <ExportSection title={t('viewExport.sectionContent')} summary={contentSummary} open testid="pro-data-export-content">
    <p class="xs-note">{t('proExport.dataAlways')}</p>
    <div class="dx-group">
      <span class="dx-head">{t('proExport.groupModel')}</span>
      <span class="dx-all">
        <button type="button" onclick={() => setAll(modelShown, true)}>{t('proExport.all')}</button>
        <button type="button" onclick={() => setAll(modelShown, false)}>{t('proExport.none')}</button>
      </span>
    </div>
    <div class="xs-grid">
      {#each modelShown as n (n)}
        <label class="xs-check"><input type="checkbox" checked={on(n)} onchange={() => flip(n)} data-testid={`pro-data-sheet-${n}`} />{t(`proExport.sheet.${n}`)}</label>
      {/each}
    </div>
    <div class="dx-group">
      <span class="dx-head">{t('proExport.groupResults')}</span>
      <span class="dx-all">
        <button type="button" onclick={() => setAll(resultsShown, true)}>{t('proExport.all')}</button>
        <button type="button" onclick={() => setAll(resultsShown, false)}>{t('proExport.none')}</button>
      </span>
    </div>
    <div class="xs-grid">
      {#each resultsShown as n (n)}
        <label class="xs-check"><input type="checkbox" checked={on(n)} onchange={() => flip(n)} data-testid={`pro-data-sheet-${n}`} />{t(`proExport.sheet.${n}`)}</label>
      {/each}
    </div>
    {#if wantsResults && !resultsStore.results3D}<p class="xs-note">{t('proExport.dataSolves')}</p>{/if}
  </ExportSection>

  {#if wantsResults && (cases.length > 1 || combos.length > 0)}
    <ExportSection title={t('proExport.sectionSets')} summary={setsSummary} testid="pro-data-export-sets">
      {#if cases.length}
        <span class="dx-head">{t('proExport.groupCases')}</span>
        <div class="xs-grid">
          {#each cases as c (c.id)}
            <label class="xs-check"><input type="checkbox" checked={!casesOff.has(c.id)} onchange={() => flipId('cases', c.id)} />{c.name}</label>
          {/each}
        </div>
      {/if}
      {#if combos.length}
        <span class="dx-head">{t('proExport.groupCombos')}</span>
        <div class="xs-grid">
          {#each combos as c (c.id)}
            <label class="xs-check"><input type="checkbox" checked={!combosOff.has(c.id)} onchange={() => flipId('combos', c.id)} />{c.name}</label>
          {/each}
        </div>
      {/if}
      <p class="xs-note">{t('proExport.setsNote')}</p>
    </ExportSection>
  {/if}

  {#if wantsStations}
    <ExportSection title={t('proExport.sectionStations')} summary={stationsSummary} testid="pro-data-export-stations">
      {#each STATIONS as s (String(s.id))}
        <label class="xs-check"><input type="radio" name="pro-data-stations" checked={stations === s.id} onchange={() => { stations = s.id; save(); }} data-testid={`pro-data-stations-${s.id}`} />{t(s.key)}</label>
      {/each}
      <p class="xs-note">{t('proExport.stationsNote')}</p>
    </ExportSection>
  {/if}

  <ExportSection title={t('proExport.format')} summary={formatSummary}>
    <div class="xs-seg">
      <button type="button" class:on={format === 'xlsx'} onclick={() => (format = 'xlsx')} data-testid="pro-data-format-xlsx">Excel</button>
      <button type="button" class:on={format === 'csv'} onclick={() => (format = 'csv')} data-testid="pro-data-format-csv">CSV</button>
    </div>
    <p class="xs-note">{format === 'xlsx' ? t('proExport.formatXlsxNote') : t('proExport.formatCsvNote')}</p>
  </ExportSection>

  {#snippet actions()}
    <button type="button" onclick={() => (open = false)}>{t('calcReport.cancel')}</button>
    <button type="button" class="primary" onclick={run} disabled={busy || modelCount + resultCount === 0} data-testid="pro-data-export-download">{t('viewExport.download')}</button>
  {/snippet}
</ExportDialog>

<style>
  .dx-group { display: flex; align-items: baseline; justify-content: space-between; gap: 8px; margin-top: 4px; }
  .dx-head { font-size: 0.74rem; color: var(--st-text-3); text-transform: uppercase; letter-spacing: 0.04em; }
  .dx-all { display: inline-flex; gap: 8px; }
  .dx-all button { background: none; border: none; padding: 0; color: var(--st-accent); font: inherit; font-size: 0.74rem; cursor: pointer; }
  .dx-all button:hover { text-decoration: underline; }
</style>
