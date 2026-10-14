<script lang="ts">
  /**
   * DXF: what goes in the drawing, from which results, and how it is drawn (`lib/dxf/writer.ts`).
   * Each content goes on its own layer, so what is turned on here can still be hidden in CAD.
   */
  import { t } from '../../../lib/i18n';
  import ExportDialog from '../../export/ExportDialog.svelte';
  import ExportSection from '../../export/ExportSection.svelte';
  import { DXF_CONTENTS, DXF_NEEDS_RESULTS, type DxfContents, type DxfProjection } from '../../../lib/dxf/writer';
  import type { DxfUnit } from '../../../lib/dxf/types';
  import { downloadProDxf } from '../../../lib/pro/dxf-export';
  import { ensureProResults, hasShells, resultSetChoices } from '../../../lib/pro/export-results';
  import { uiStore } from '../../../lib/store';

  let { open = $bindable(false) }: { open: boolean } = $props();

  const KEY = 'stabileo-pro-dxf-export';
  interface Prefs { contents: DxfContents; projection: DxfProjection; units: DxfUnit; diagramFactor: number }
  const BASE: Prefs = {
    contents: { members: true, nodes: false, supports: true, shells: true, numbering: true, loads: true, deformed: false, moment: false, shear: false, axial: false, reactions: false },
    projection: 'space', units: 'm', diagramFactor: 1,
  };
  function load(): Prefs {
    try {
      const p = JSON.parse(localStorage.getItem(KEY) ?? '{}') as Partial<Prefs>;
      return { ...BASE, ...p, contents: { ...BASE.contents, ...(p.contents ?? {}) } };
    } catch { return structuredClone(BASE); }
  }
  const prefs = load();
  let contents = $state<DxfContents>(prefs.contents);
  let projection = $state<DxfProjection>(prefs.projection);
  let units = $state<DxfUnit>(prefs.units);
  let diagramFactor = $state(prefs.diagramFactor);
  let setKey = $state('screen');
  let busy = $state(false);
  function save() {
    try { localStorage.setItem(KEY, JSON.stringify({ contents: $state.snapshot(contents), projection, units, diagramFactor })); } catch { /* this session only */ }
  }

  const shells = $derived(open && hasShells());
  const offered = $derived(DXF_CONTENTS.filter((k) => k !== 'shells' || shells));
  const MODEL_PART = new Set<keyof DxfContents>(['members', 'nodes', 'supports', 'shells', 'numbering', 'loads']);
  const needsSolve = $derived(offered.some((k) => contents[k] && DXF_NEEDS_RESULTS.has(k)));
  const diagrams = $derived(contents.moment || contents.shear || contents.axial);
  const choices = $derived(open ? resultSetChoices() : []);
  const anything = $derived(offered.some((k) => contents[k]));

  const PROJECTIONS: Array<{ id: DxfProjection; key: string }> = [
    { id: 'space', key: 'proExport.dxf3d' }, { id: 'top', key: 'proExport.viewTop' },
    { id: 'front', key: 'proExport.viewFront' }, { id: 'side', key: 'proExport.viewSide' },
  ];
  const UNITS: DxfUnit[] = ['m', 'cm', 'mm'];

  const contentSummary = $derived(offered.filter((k) => contents[k]).map((k) => t(`proExport.dxf.${k}`)).join(', ') || t('viewExport.nothing'));
  const resultsSummary = $derived(choices.find((c) => c.key === setKey)?.label ?? '—');
  const drawingSummary = $derived(`${t(PROJECTIONS.find((p) => p.id === projection)!.key)} · ${units}`);

  async function run() {
    if (busy || !anything) return;
    busy = true;
    try {
      let c = $state.snapshot(contents) as DxfContents;
      if (needsSolve && !(await ensureProResults())) {
        // Without results, the drawing is the model: what needed them is left out, and said.
        c = { ...c, deformed: false, moment: false, shear: false, axial: false, reactions: false };
        uiStore.toast(t('proExport.dxfNoResults'), 'info');
      }
      downloadProDxf({ contents: c, projection, units, set: setKey, diagramFactor });
      open = false;
    } finally {
      busy = false;
    }
  }
</script>

<ExportDialog bind:open title={t('proExport.dxfTitle')} testid="pro-dxf-export-dialog">
  <ExportSection title={t('viewExport.sectionContent')} summary={contentSummary} open testid="pro-dxf-export-content">
    <span class="dx-head">{t('proExport.groupModel')}</span>
    <div class="xs-grid">
      {#each offered.filter((k) => MODEL_PART.has(k)) as k (k)}
        <label class="xs-check"><input type="checkbox" bind:checked={contents[k]} onchange={save} data-testid={`pro-dxf-${k}`} />{t(`proExport.dxf.${k}`)}</label>
      {/each}
    </div>
    <span class="dx-head">{t('proExport.groupResults')}</span>
    <div class="xs-grid">
      {#each offered.filter((k) => !MODEL_PART.has(k)) as k (k)}
        <label class="xs-check"><input type="checkbox" bind:checked={contents[k]} onchange={save} data-testid={`pro-dxf-${k}`} />{t(`proExport.dxf.${k}`)}</label>
      {/each}
    </div>
    <p class="xs-note">{t('proExport.dxfLayersNote')}</p>
  </ExportSection>

  {#if needsSolve && choices.length > 1}
    <ExportSection title={t('proExport.sectionResults')} summary={resultsSummary} testid="pro-dxf-export-results">
      <label class="xs-field">
        <span>{t('proExport.resultsOf')}</span>
        <select bind:value={setKey} data-testid="pro-dxf-set">
          {#each choices as c (c.key)}<option value={c.key}>{c.label}</option>{/each}
        </select>
      </label>
    </ExportSection>
  {/if}

  <ExportSection title={t('viewExport.sectionDrawing')} summary={drawingSummary}>
    <div class="xs-field">
      <span>{t('proExport.dxfProjection')}</span>
      <div class="xs-seg">
        {#each PROJECTIONS as p (p.id)}
          <button type="button" class:on={projection === p.id} onclick={() => { projection = p.id; save(); }} data-testid={`pro-dxf-projection-${p.id}`}>{t(p.key)}</button>
        {/each}
      </div>
    </div>
    <div class="xs-field">
      <span>{t('proExport.dxfUnits')}</span>
      <div class="xs-seg">
        {#each UNITS as u (u)}
          <button type="button" class:on={units === u} onclick={() => { units = u; save(); }} data-testid={`pro-dxf-units-${u}`}>{u}</button>
        {/each}
      </div>
    </div>
    {#if diagrams}
      <label class="xs-field">
        <span>{t('proExport.dxfDiagramSize')}</span>
        <select bind:value={diagramFactor} onchange={save}>
          <option value={0.5}>{t('proExport.sizeSmall')}</option>
          <option value={1}>{t('proExport.sizeNormal')}</option>
          <option value={2}>{t('proExport.sizeLarge')}</option>
        </select>
      </label>
    {/if}
    <p class="xs-note">{projection === 'space' ? t('proExport.dxf3dNote') : t('proExport.dxfProjectedNote')}</p>
  </ExportSection>

  {#snippet actions()}
    <button type="button" onclick={() => (open = false)}>{t('calcReport.cancel')}</button>
    <button type="button" class="primary" onclick={run} disabled={busy || !anything} data-testid="pro-dxf-export-download">{t('viewExport.download')}</button>
  {/snippet}
</ExportDialog>

<style>
  .dx-head { font-size: 0.74rem; color: var(--st-text-3); text-transform: uppercase; letter-spacing: 0.04em; margin-top: 2px; }
</style>
