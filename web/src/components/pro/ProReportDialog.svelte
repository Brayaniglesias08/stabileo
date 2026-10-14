<script lang="ts">
  /**
   * The calculation report, as a PDF: who it is from, what it carries and how its figures look.
   *
   * The same frame and folded sections as Basic's report (`components/export/`): each section
   * closed, its title saying in one line what is chosen inside. Each result is printed as its
   * table, its figures or both; the figures are drawn from the model (`pro/report-export.ts`),
   * and figures added from the view still go at the end.
   *
   * Only the PDF: the same data as a spreadsheet is Project › Export › Excel, with its own
   * choice of sheets.
   */
  import { proNav } from '../../lib/store/pro-nav.svelte';
  import { modelStore } from '../../lib/store';
  import { t, tp } from '../../lib/i18n';
  import ExportDialog from '../export/ExportDialog.svelte';
  import ExportSection from '../export/ExportSection.svelte';
  import type { ReportConfig, ReportPart, ReportResult, ResultMode } from '../../lib/engine/pro-report';
  import type { ReportFigureChoices } from '../../lib/pro/report-export';
  import type { FigureView } from '../../lib/export/view-figure';
  import type { InkStyle } from '../../lib/export/paper-ink';
  import { SHELL_CONTOUR_COMPONENTS, shellComponentLabelKey } from '../../lib/engine/shell-stress';
  import { reportFigures } from '../../lib/store/report-figures.svelte';
  import { captureFigure } from '../../lib/export/figure';

  interface Props {
    open: boolean;
    hasResults: boolean;
    hasAdvanced: boolean;
    /** WHICH advanced analyses actually ran — the keys of their results. */
    advancedRan: string[];
    hasDiagnostics: boolean;
    hasQuantities: boolean;
    ongenerate: (config: ReportConfig, figures: ReportFigureChoices) => void;
  }

  let { open = $bindable(false), hasResults, hasAdvanced, advancedRan, hasDiagnostics, hasQuantities, ongenerate }: Props = $props();

  // ─── Remembered between sessions (localStorage) ─────────────
  // Migrate old storage key
  if (localStorage.getItem('dedaliano-report-config') !== null && localStorage.getItem('stabileo-report-config') === null) {
    localStorage.setItem('stabileo-report-config', localStorage.getItem('dedaliano-report-config')!);
    localStorage.removeItem('dedaliano-report-config');
  }
  const STORAGE_KEY = 'stabileo-report-config';
  type Sections = Omit<ReportConfig['sections'], 'parts'>;
  interface Saved {
    companyName?: string; companyLogo?: string | null;
    sections?: Partial<Sections>; parts?: Partial<Record<ReportPart, boolean>>;
    modes?: Partial<Record<ReportResult, ResultMode>>; figures?: Partial<ReportFigureChoices>;
  }
  function loadSaved(): Saved {
    try { const raw = localStorage.getItem(STORAGE_KEY); return raw ? JSON.parse(raw) : {}; } catch { return {}; }
  }
  const saved = loadSaved();

  let companyName = $state(saved.companyName ?? '');
  let companyLogo = $state<string | null>(saved.companyLogo ?? null);
  let sec = $state<Sections>({
    modelData: true, results: true, verification: false, advancedAnalysis: true, storyDrift: false,
    diagnostics: true, quantities: true, loads: true, envelope: true, statics: true, deflections: true, figures: true,
    ...(saved.sections ?? {}),
    // Never offered here: a code check is reported from the design, and story drift was never implemented.
  });
  let parts = $state<Record<ReportPart, boolean>>({
    nodes: true, materials: true, sections: true, members: true, supports: true, shells: true, combinations: true, ...(saved.parts ?? {}),
  });
  let modes = $state<Record<ReportResult, ResultMode>>({ reactions: 'both', displacements: 'both', forces: 'both', shells: 'both', ...(saved.modes ?? {}) });
  let figs = $state<ReportFigureChoices>({ style: 'color', values: true, view: 'screen', shellComponent: 'vonMises', ...(saved.figures ?? {}) });
  /** Which advanced analyses to print, of the ones that ran. */
  let advancedPicked = $state<Record<string, boolean>>({});

  function persist() {
    try {
      localStorage.setItem(STORAGE_KEY, JSON.stringify({
        companyName, companyLogo, sections: $state.snapshot(sec), parts: $state.snapshot(parts),
        modes: $state.snapshot(modes), figures: $state.snapshot(figs),
      }));
    } catch { /* this session only */ }
  }

  /*
   * A letterhead nobody filled in is not a letterhead: `hasProjectInfo` decides whether the
   * report prints one at all, so leaving it blank is a decision the document honours.
   */
  const hasProjectInfo = $derived(!!(companyName.trim() || companyLogo));
  let logoError = $state<string | null>(null);
  function handleLogoUpload(e: Event) {
    const input = e.target as HTMLInputElement;
    const file = input.files?.[0];
    if (!file) return;
    logoError = null;
    if (file.size > 500_000) { logoError = t('report.logoTooLarge'); input.value = ''; return; }
    const reader = new FileReader();
    reader.onload = () => { companyLogo = reader.result as string; persist(); };
    reader.readAsDataURL(file);
  }

  let figureError = $state(false);
  function addFigure() {
    const f = captureFigure();
    figureError = !f;
    if (f) reportFigures.add(f);
  }

  const shells = $derived(open && modelStore.plates.size + modelStore.quads.size > 0);
  const PARTS: Array<{ id: ReportPart; key: string }> = [
    { id: 'nodes', key: 'report.nodes' }, { id: 'materials', key: 'report.materials' }, { id: 'sections', key: 'report.sections' },
    { id: 'members', key: 'report.elements' }, { id: 'supports', key: 'report.supports' },
    { id: 'shells', key: 'report.slabsAndWalls' }, { id: 'combinations', key: 'report.loadCombinations' },
  ];
  const partsShown = $derived(PARTS.filter((p) => p.id !== 'shells' || shells));
  const RESULTS: Array<{ id: ReportResult; key: string; figures: string }> = [
    { id: 'reactions', key: 'report.reactions', figures: 'proReport.figReactions' },
    { id: 'displacements', key: 'report.displacements', figures: 'proReport.figDeformed' },
    { id: 'forces', key: 'report.forces', figures: 'proReport.figForces' },
    { id: 'shells', key: 'report.shellResults', figures: 'proReport.figShells' },
  ];
  const resultsShown = $derived(RESULTS.filter((r) => r.id !== 'shells' || shells));
  const MODES: Array<{ id: ResultMode; key: string }> = [
    { id: 'table', key: 'proReport.modeTable' }, { id: 'figure', key: 'proReport.modeFigure' }, { id: 'both', key: 'proReport.modeBoth' },
  ];
  const VIEWS: Array<{ id: FigureView; key: string }> = [
    { id: 'screen', key: 'proExport.viewScreen' }, { id: 'top', key: 'proExport.viewTop' },
    { id: 'front', key: 'proExport.viewFront' }, { id: 'side', key: 'proExport.viewSide' }, { id: 'iso', key: 'proExport.viewIso' },
  ];
  const STYLES: Array<{ id: InkStyle; key: string }> = [
    { id: 'color', key: 'viewExport.styleColor' }, { id: 'bw', key: 'viewExport.styleBw' },
  ];

  const projectSummary = $derived(hasProjectInfo ? [companyName.trim(), companyLogo ? t('report.companyLogo') : ''].filter(Boolean).join(' · ') : t('report.projectInfoEmpty'));
  const contentSummary = $derived.by(() => {
    const out: string[] = [];
    if (sec.modelData) out.push(t('proReport.partModel'));
    if (sec.loads) out.push(t('proReport.partLoads'));
    if (sec.results && hasResults) out.push(t('proReport.partResults'));
    if (sec.advancedAnalysis && hasAdvanced) out.push(t('proReport.partAdvanced'));
    if (sec.quantities && hasQuantities) out.push(t('proReport.partQuantities'));
    if (sec.diagnostics && hasDiagnostics) out.push(t('proReport.partDiagnostics'));
    return out.join(' · ') || '—';
  });
  const figuresSummary = $derived([
    t(STYLES.find((s) => s.id === figs.style)?.key ?? 'viewExport.styleColor'),
    t(VIEWS.find((v) => v.id === figs.view)!.key),
    figs.values ? t('viewExport.optValues') : null,
    reportFigures.list.length ? tp('proReport.addedN', { n: reportFigures.list.length }) : null,
  ].filter(Boolean).join(' · '));

  function handleGenerate() {
    persist();
    ongenerate({
      companyName,
      companyLogo,
      figures: sec.figures ? reportFigures.list.map(({ dataUrl, caption }) => ({ dataUrl, caption })) : [],
      hasProjectInfo,
      advancedPicked: { ...advancedPicked },
      sections: { ...$state.snapshot(sec), verification: false, storyDrift: false, parts: $state.snapshot(parts) },
      resultModes: $state.snapshot(modes),
    }, $state.snapshot(figs));
  }
</script>

<ExportDialog bind:open title={t('proReport.title')} testid="pro-report-dialog">
  <ExportSection title={t('report.projectInfo')} summary={projectSummary} open={hasProjectInfo} testid="rpt-project">
    <div class="rpt-logo-row">
      <span>{t('report.companyLogo')}</span>
      {#if companyLogo}
        <img src={companyLogo} alt="Logo" class="rpt-logo-preview" />
        <button type="button" class="rpt-link" onclick={() => { companyLogo = null; persist(); }}>{t('report.removeLogo')}</button>
      {:else}
        <label class="rpt-file-btn">{t('report.chooseLogo')}
          <input type="file" accept="image/png,image/jpeg,image/svg+xml" onchange={handleLogoUpload} class="rpt-file-input" data-testid="rpt-logo-file" />
        </label>
        {#if logoError}<span class="rpt-error" role="alert" data-testid="rpt-logo-error">{logoError}</span>{/if}
      {/if}
    </div>
    <label class="xs-field">
      <span>{t('report.companyName')}</span>
      <input type="text" bind:value={companyName} onchange={persist} placeholder={t('report.companyNamePh')} />
    </label>
    <p class="xs-note">{t('report.projectDataMoved')}
      <button type="button" class="rpt-link" onclick={() => { open = false; proNav.openProjectInfo(); }} data-testid="rpt-open-project-info">{t('report.openProjectInfo')}</button>
    </p>
  </ExportSection>

  <ExportSection title={t('viewExport.sectionContent')} summary={contentSummary} open testid="rpt-content">
    <label class="xs-check"><input type="checkbox" bind:checked={sec.modelData} onchange={persist} data-testid="rpt-model" />{t('proReport.modelData')}</label>
    {#if sec.modelData}
      <div class="xs-grid rpt-sub">
        {#each partsShown as p (p.id)}
          <label class="xs-check"><input type="checkbox" bind:checked={parts[p.id]} onchange={persist} data-testid={`rpt-part-${p.id}`} />{t(p.key)}</label>
        {/each}
      </div>
    {/if}
    <label class="xs-check"><input type="checkbox" bind:checked={sec.loads} onchange={persist} />{t('proReport.loads')}</label>

    <label class="xs-check" class:off={!hasResults}><input type="checkbox" bind:checked={sec.results} onchange={persist} disabled={!hasResults} data-testid="rpt-results" />{t('proReport.results')}{#if !hasResults}&nbsp;<span class="xs-note">({t('report.noData')})</span>{/if}</label>
    {#if sec.results && hasResults}
      <div class="rpt-sub">
        {#each resultsShown as r (r.id)}
          <div class="rpt-row">
            <span title={t(r.figures)}>{t(r.key)}</span>
            <div class="xs-seg">
              {#each MODES as m (m.id)}
                <button type="button" class:on={modes[r.id] === m.id} onclick={() => { modes[r.id] = m.id; persist(); }} data-testid={`rpt-mode-${r.id}-${m.id}`}>{t(m.key)}</button>
              {/each}
            </div>
          </div>
        {/each}
        <label class="xs-check"><input type="checkbox" bind:checked={sec.envelope} onchange={persist} data-testid="rpt-envelope" />{t('report.env.title')}</label>
        <label class="xs-check"><input type="checkbox" bind:checked={sec.statics} onchange={persist} data-testid="rpt-statics" />{t('pro.statics.title')}</label>
        <label class="xs-check"><input type="checkbox" bind:checked={sec.deflections} onchange={persist} data-testid="rpt-deflections" />{t('defl.title')}</label>
      </div>
    {/if}

    <label class="xs-check" class:off={!hasAdvanced}><input type="checkbox" bind:checked={sec.advancedAnalysis} onchange={persist} disabled={!hasAdvanced} />{t('report.secAdvanced')}{#if !hasAdvanced}&nbsp;<span class="xs-note">({t('report.noData')})</span>{/if}</label>
    <!-- WHICH advanced analyses, and only the ones that ran. -->
    {#if sec.advancedAnalysis && advancedRan.length > 0}
      <div class="xs-grid rpt-sub" data-testid="rpt-advanced-picks">
        {#each advancedRan as key (key)}
          <label class="xs-check">
            <input type="checkbox" checked={advancedPicked[key] ?? true} data-testid="rpt-adv-{key}"
              onchange={(e) => (advancedPicked = { ...advancedPicked, [key]: e.currentTarget.checked })} />
            {t(`pro.adv.${key}`) === `pro.adv.${key}` ? key : t(`pro.adv.${key}`)}
          </label>
        {/each}
      </div>
    {/if}
    <label class="xs-check" class:off={!hasQuantities}><input type="checkbox" bind:checked={sec.quantities} onchange={persist} disabled={!hasQuantities} />{t('report.secQuantities')}{#if !hasQuantities}&nbsp;<span class="xs-note">({t('report.noData')})</span>{/if}</label>
    <label class="xs-check" class:off={!hasDiagnostics}><input type="checkbox" bind:checked={sec.diagnostics} onchange={persist} disabled={!hasDiagnostics} />{t('report.secDiagnostics')}{#if !hasDiagnostics}&nbsp;<span class="xs-note">({t('report.noData')})</span>{/if}</label>
  </ExportSection>

  <ExportSection title={t('report.figures')} summary={figuresSummary} testid="rpt-figures">
    <div class="xs-field">
      <span>{t('viewExport.style')}</span>
      <div class="xs-seg">
        {#each STYLES as s (s.id)}
          <button type="button" class:on={figs.style === s.id} onclick={() => { figs.style = s.id; persist(); }}>{t(s.key)}</button>
        {/each}
      </div>
    </div>
    <div class="xs-field">
      <span>{t('proExport.view')}</span>
      <div class="xs-seg">
        {#each VIEWS as v (v.id)}
          <button type="button" class:on={figs.view === v.id} onclick={() => { figs.view = v.id; persist(); }} data-testid={`rpt-view-${v.id}`}>{t(v.key)}</button>
        {/each}
      </div>
    </div>
    <label class="xs-check"><input type="checkbox" bind:checked={figs.values} onchange={persist} />{t('viewExport.optValuesLong')}</label>
    {#if shells && modes.shells !== 'table'}
      <label class="xs-field">
        <span>{t('proExport.shellComponent')}</span>
        <select bind:value={figs.shellComponent} onchange={persist}>
          {#each SHELL_CONTOUR_COMPONENTS as m (m.key)}<option value={m.key}>{t(shellComponentLabelKey(m.key))}</option>{/each}
        </select>
      </label>
    {/if}
    <p class="xs-note">{t('proReport.figuresNote')}</p>

    <label class="xs-check"><input type="checkbox" bind:checked={sec.figures} onchange={persist} />{t('proReport.addedFigures')} ({reportFigures.list.length})</label>
    <div>
      <button type="button" class="rpt-small" onclick={addFigure} data-testid="rpt-add-figure">+ {t('report.addFigure')}</button>
      {#if figureError}<span class="xs-note">{t('report.figureFailed')}</span>{/if}
    </div>
    {#each reportFigures.list as f, i (f.id)}
      <div class="rpt-figure" data-testid="rpt-figure">
        <img src={f.dataUrl} alt={f.caption} />
        <input type="text" value={f.caption} onchange={(e) => reportFigures.caption(f.id, e.currentTarget.value)} aria-label={t('report.figureCaption')} />
        <button type="button" class="rpt-small" disabled={i === 0} onclick={() => reportFigures.move(f.id, -1)} aria-label="↑">↑</button>
        <button type="button" class="rpt-small" onclick={() => reportFigures.remove(f.id)} aria-label={t('report.removeFigure')}>×</button>
      </div>
    {/each}
    <p class="xs-note">{t('report.figuresHint')}</p>
  </ExportSection>

  {#snippet actions()}
    <button type="button" onclick={() => (open = false)}>{t('report.cancel')}</button>
    <button type="button" class="primary" onclick={handleGenerate} data-testid="rpt-pdf">{t('proReport.generate')}</button>
  {/snippet}
</ExportDialog>

<style>
  .rpt-sub { margin: 0 0 4px 22px; display: flex; flex-direction: column; gap: 4px; }
  .rpt-sub.xs-grid { display: grid; }
  .rpt-row { display: flex; align-items: center; justify-content: space-between; gap: 8px; color: var(--st-text-2); }
  .rpt-logo-row { display: flex; align-items: center; gap: 8px; color: var(--st-text-2); }
  .rpt-logo-preview { max-height: 32px; max-width: 120px; border-radius: 4px; border: 1px solid var(--st-hair); }
  .rpt-file-btn { position: relative; display: inline-flex; padding: 3px 10px; font-size: 0.74rem; color: var(--st-text-2); border: 1px solid var(--st-hair-strong); border-radius: var(--st-radius); cursor: pointer; }
  .rpt-file-btn:focus-within { outline: 2px solid var(--st-interactive); outline-offset: 1px; }
  .rpt-file-input { position: absolute; inset: 0; opacity: 0; cursor: pointer; }
  .rpt-error { font-size: 0.72rem; color: var(--st-danger); }
  .rpt-link { padding: 0; background: none; border: none; color: var(--st-interactive); cursor: pointer; font: inherit; text-decoration: underline; }
  .rpt-small { padding: 2px 8px; font-size: 0.72rem; border-radius: var(--st-radius); border: 1px solid var(--st-hair-strong); background: var(--st-surface-2); color: var(--st-text-2); cursor: pointer; }
  .rpt-small:disabled { opacity: 0.45; cursor: not-allowed; }
  .rpt-figure { display: flex; gap: 6px; align-items: center; }
  .rpt-figure img { width: 64px; height: auto; border: 1px solid var(--st-hair); }
  .rpt-figure input { flex: 1; min-width: 0; padding: 3px 6px; background: var(--st-surface-2); border: 1px solid var(--st-hair-strong); border-radius: var(--st-radius); color: var(--st-text); font: inherit; }
</style>
