<script lang="ts">
  /**
   * PRO's "Exportar imagen": Basic's figures (`lib/export/view-figure.ts`), with what a PRO model
   * adds: its shells, as faces and as a result in colour; a fixed view (plan, the two elevations,
   * isometric) besides the camera's; the results of any case, combination or the envelope; and
   * JPG besides SVG and PNG. The frame and the folded sections are Basic's export dialog's.
   */
  import { t } from '../../../lib/i18n';
  import { uiStore } from '../../../lib/store';
  import ExportDialog from '../../export/ExportDialog.svelte';
  import ExportSection from '../../export/ExportSection.svelte';
  import {
    DEFAULT_FIGURE_OPTIONS, NEEDS_RESULTS, PRO_FIGURE_CONTENTS,
    type FigureContent, type FigureOptions, type FigureView,
  } from '../../../lib/export/view-figure';
  import { copyView, exportView, type ViewFormat } from '../../../lib/export/view-export';
  import type { InkStyle } from '../../../lib/export/paper-ink';
  import { SHELL_CONTOUR_COMPONENTS, shellComponentLabelKey, type ShellContourComponent } from '../../../lib/engine/shell-stress';
  import { ensureProResults, hasShells, resultSet, resultSetChoices } from '../../../lib/pro/export-results';

  let { open = $bindable(false), format = $bindable('svg') }: { open: boolean; format: ViewFormat } = $props();

  const KEY = 'stabileo-pro-view-export';
  type Saved = Omit<FigureOptions, 'results' | 'resultsName'>;
  interface Prefs { contents: FigureContent[]; options: Saved; sheet: boolean }
  function load(): Prefs {
    const base: Prefs = { contents: ['model'], options: { ...DEFAULT_FIGURE_OPTIONS, view: 'screen', shells: true, shellComponent: 'vonMises' }, sheet: true };
    try {
      const raw = localStorage.getItem(KEY);
      if (!raw) return base;
      const p = JSON.parse(raw) as Partial<Prefs>;
      return {
        contents: (p.contents ?? base.contents).filter((c) => PRO_FIGURE_CONTENTS.includes(c)),
        options: { ...base.options, ...(p.options ?? {}) },
        sheet: p.sheet ?? base.sheet,
      };
    } catch { return base; }
  }
  const prefs = load();
  let contents = $state<FigureContent[]>(prefs.contents);
  let options = $state<Saved>(prefs.options);
  let sheet = $state(prefs.sheet);
  let setKey = $state('screen');
  let busy = $state(false);

  function save() {
    try { localStorage.setItem(KEY, JSON.stringify({ contents, options, sheet })); } catch { /* the choices last this session */ }
  }

  const shells = $derived(open && hasShells());
  /** The figures this model can have: the shells' only with shells. */
  const offered = $derived(PRO_FIGURE_CONTENTS.filter((c) => c !== 'shellMap' || shells));
  const chosen = $derived(contents.filter((c) => offered.includes(c)));
  const needsSolve = $derived(chosen.some((c) => NEEDS_RESULTS.has(c)));
  const choices = $derived(open ? resultSetChoices() : []);

  const LABEL: Record<FigureContent, string> = {
    model: 'figures.model', reactions: 'figures.reactions', despiece: 'figures.despiece',
    moment: 'figures.moment', shear: 'figures.shear', axial: 'figures.axial', deformed: 'figures.deformed',
    shellMap: 'figures.shellMap',
  };
  const VIEWS: Array<{ id: FigureView; key: string }> = [
    { id: 'screen', key: 'proExport.viewScreen' }, { id: 'top', key: 'proExport.viewTop' },
    { id: 'front', key: 'proExport.viewFront' }, { id: 'side', key: 'proExport.viewSide' }, { id: 'iso', key: 'proExport.viewIso' },
  ];
  const STYLES: Array<{ id: InkStyle; key: string }> = [
    { id: 'color', key: 'viewExport.styleColor' }, { id: 'bw', key: 'viewExport.styleBw' }, { id: 'screen', key: 'viewExport.styleScreen' },
  ];
  const FORMATS: ViewFormat[] = ['svg', 'png', 'jpg'];
  const compName = (k: ShellContourComponent) => t(shellComponentLabelKey(k));

  function toggle(c: FigureContent) {
    contents = contents.includes(c) ? contents.filter((x) => x !== c) : PRO_FIGURE_CONTENTS.filter((x) => x === c || contents.includes(x));
    save();
  }

  const contentSummary = $derived(chosen.length ? chosen.map((c) => t(LABEL[c])).join(', ') : t('viewExport.nothing'));
  const resultsSummary = $derived(choices.find((c) => c.key === setKey)?.label ?? choices[0]?.label ?? '—');
  const drawingSummary = $derived([
    t(VIEWS.find((v) => v.id === (options.view ?? 'screen'))!.key),
    options.loads ? t('viewExport.optLoads') : null,
    options.numbering ? t('viewExport.optNumbering') : null,
    options.values ? t('viewExport.optValues') : null,
    shells && options.shells !== false ? t('proExport.optShells') : null,
  ].filter(Boolean).join(' · '));
  const outputSummary = $derived([
    format.toUpperCase(),
    t(STYLES.find((s) => s.id === options.style)!.key),
    chosen.length > 1 ? t(sheet ? 'viewExport.layoutSheet' : 'viewExport.layoutSeparate') : null,
  ].filter(Boolean).join(' · '));

  async function run(kind: 'download' | 'copy') {
    if (busy || chosen.length === 0) return;
    busy = true;
    try {
      // Solved here, with PRO's solve, so the export does not solve with another one.
      if (needsSolve) await ensureProResults();
      const set = setKey === 'screen' ? null : resultSet(setKey);
      const req = {
        format, contents: [...chosen], sheet,
        options: { ...$state.snapshot(options), ...(set ? { results: set.results, resultsName: set.name } : {}) } as FigureOptions,
      };
      if (kind === 'copy') {
        if (await copyView(req)) { uiStore.toast(t('viewExport.copied'), 'success'); open = false; }
      } else if (await exportView(req)) open = false;
    } finally {
      busy = false;
    }
  }
</script>

<ExportDialog bind:open title={`${t('proExport.imageTitle')} — ${format.toUpperCase()}`} testid="pro-view-export-dialog">
  <ExportSection title={t('viewExport.sectionContent')} summary={contentSummary} open testid="pro-view-export-content">
    <div class="xs-grid">
      {#each offered as c (c)}
        <label class="xs-check"><input type="checkbox" checked={contents.includes(c)} onchange={() => toggle(c)} data-testid={`pro-view-export-${c}`} />{t(LABEL[c])}</label>
      {/each}
    </div>
    {#if chosen.includes('shellMap')}
      <label class="xs-field">
        <span>{t('proExport.shellComponent')}</span>
        <select bind:value={options.shellComponent} onchange={save} data-testid="pro-view-export-shell-component">
          {#each SHELL_CONTOUR_COMPONENTS as m (m.key)}<option value={m.key}>{compName(m.key)}</option>{/each}
        </select>
      </label>
    {/if}
    {#if needsSolve}<p class="xs-note">{t('viewExport.needsSolve')}</p>{/if}
    {#if chosen.includes('despiece')}<p class="xs-note">{t('viewExport.despieceNote')}</p>{/if}
  </ExportSection>

  {#if needsSolve && choices.length > 1}
    <ExportSection title={t('proExport.sectionResults')} summary={resultsSummary} testid="pro-view-export-results">
      <label class="xs-field">
        <span>{t('proExport.resultsOf')}</span>
        <select bind:value={setKey} data-testid="pro-view-export-set">
          {#each choices as c (c.key)}<option value={c.key}>{c.label}</option>{/each}
        </select>
      </label>
      <p class="xs-note">{t('proExport.resultsNote')}</p>
    </ExportSection>
  {/if}

  <ExportSection title={t('viewExport.sectionDrawing')} summary={drawingSummary}>
    <div class="xs-field">
      <span>{t('proExport.view')}</span>
      <div class="xs-seg">
        {#each VIEWS as v (v.id)}
          <button type="button" class:on={(options.view ?? 'screen') === v.id} onclick={() => { options.view = v.id; save(); }} data-testid={`pro-view-export-view-${v.id}`}>{t(v.key)}</button>
        {/each}
      </div>
    </div>
    <label class="xs-check"><input type="checkbox" bind:checked={options.loads} onchange={save} />{t('viewExport.optLoadsLong')}</label>
    <label class="xs-check"><input type="checkbox" bind:checked={options.numbering} onchange={save} />{t('viewExport.optNumberingLong')}</label>
    <label class="xs-check"><input type="checkbox" bind:checked={options.values} onchange={save} />{t('viewExport.optValuesLong')}</label>
    <label class="xs-check"><input type="checkbox" bind:checked={options.titles} onchange={save} />{t('viewExport.optTitlesLong')}</label>
    {#if shells}
      <label class="xs-check"><input type="checkbox" checked={options.shells !== false} onchange={(e) => { options.shells = e.currentTarget.checked; save(); }} data-testid="pro-view-export-shells" />{t('proExport.optShellsLong')}</label>
    {/if}
  </ExportSection>

  <ExportSection title={t('viewExport.sectionOutput')} summary={outputSummary}>
    <div class="xs-field">
      <span>{t('proExport.format')}</span>
      <div class="xs-seg">
        {#each FORMATS as f (f)}
          <button type="button" class:on={format === f} onclick={() => (format = f)} data-testid={`pro-view-export-format-${f}`}>{f.toUpperCase()}</button>
        {/each}
      </div>
    </div>
    <div class="xs-field">
      <span>{t('viewExport.style')}</span>
      <div class="xs-seg">
        {#each STYLES as s (s.id)}
          <button type="button" class:on={options.style === s.id} onclick={() => { options.style = s.id; save(); }}>{t(s.key)}</button>
        {/each}
      </div>
    </div>
    <div class="xs-field">
      <span>{t('viewExport.layout')}</span>
      <div class="xs-seg">
        <button type="button" class:on={sheet} onclick={() => { sheet = true; save(); }}>{t('viewExport.layoutSheet')}</button>
        <button type="button" class:on={!sheet} onclick={() => { sheet = false; save(); }}>{t('viewExport.layoutSeparate')}</button>
      </div>
    </div>
  </ExportSection>

  {#snippet actions()}
    <button type="button" onclick={() => (open = false)}>{t('calcReport.cancel')}</button>
    <button type="button" onclick={() => run('copy')} disabled={busy || chosen.length === 0} data-testid="pro-view-export-copy">{t('viewExport.copy')}</button>
    <button type="button" class="primary" onclick={() => run('download')} disabled={busy || chosen.length === 0} data-testid="pro-view-export-download">{t('viewExport.download')}</button>
  {/snippet}
</ExportDialog>
