<script lang="ts">
  /**
   * The two errors of a 3D shell result side by side (docs/verification/DISCRETIZATION.md):
   * the algebraic error, CERTIFIED (`certificate.svelte.ts`), and the discretization error,
   * ESTIMATED and never certified (`discretization.svelte.ts`). Below them, the element error map
   * toggle and the on-demand convergence check.
   */
  import { t } from '../lib/i18n';
  import { uiStore } from '../lib/store';
  import { certificateStore, formatBound } from '../lib/store/certificate.svelte';
  import { discretizationStore, type QuantityConvergence } from '../lib/store/discretization.svelte';
  import { shellContourColor } from '../lib/three/stress-heatmap';

  const is3D = $derived(uiStore.is3DWorkspace);
  const est = $derived(discretizationStore.estimate);
  const study = $derived(discretizationStore.study);
  const visible = $derived(is3D && est.status !== 'idle' && discretizationStore.applies());
  const cert = $derived(certificateStore.shown(true));

  const algebraic = $derived.by(() => {
    if (cert.status === 'pending') return t('discretization.algebraicPending');
    if (cert.status !== 'done') return t('discretization.algebraicNone');
    if (cert.certificate.status === 'certified') return t('discretization.algebraicCertified').replace('{abs}', formatBound(cert.certificate.absBound));
    return t('discretization.algebraicNotCertified');
  });

  function pct(x: number | null | undefined): string {
    if (x === null || x === undefined || !Number.isFinite(x)) return '—';
    const p = 100 * x;
    return p < 0.1 ? p.toFixed(2) : p < 10 ? p.toFixed(1) : p.toFixed(0);
  }

  function num(x: number | null | undefined): string {
    if (x === null || x === undefined || !Number.isFinite(x)) return '—';
    return Math.abs(x) >= 1e4 || (Math.abs(x) < 1e-2 && x !== 0) ? x.toExponential(3) : x.toPrecision(4);
  }

  function quantityLabel(q: QuantityConvergence): string {
    const base = t(`discretization.q.${q.quantity}`);
    return q.nodeId !== undefined ? base.replace('{node}', String(q.nodeId)) : base;
  }

  const discretization = $derived(est.status === 'done' ? est.estimate : null);
  const warn = $derived(
    (discretization?.exceedsThreshold ?? false) || (study.status === 'done' && study.study.exceedsThreshold),
  );
  const threshold = $derived(discretization ? pct(discretization.threshold) : '5');

  const legend = $derived.by(() => {
    const r = discretizationStore.mapRange();
    const stops: string[] = [];
    for (let i = 0; i <= 10; i++) {
      const v = r.min + (i / 10) * (r.max - r.min);
      stops.push('#' + shellContourColor(v, r.min, r.max).toString(16).padStart(6, '0'));
    }
    return { max: r.max, gradient: `linear-gradient(to right, ${stops.join(', ')})` };
  });
</script>

{#if visible}
  <div class="disc" class:warn data-testid="discretization-status" title={t('discretization.tooltip')}>
    <div class="line">
      <span class="algebraic">{algebraic}</span>
      <span class="sep">·</span>
      {#if est.status === 'pending'}
        <span>{t('discretization.pending')}</span>
      {:else if est.status === 'failed'}
        <span>{t('discretization.failed').replace('{reason}', est.reason)}</span>
      {:else if discretization && discretization.relativeError !== null}
        <span data-testid="discretization-estimate">{t('discretization.estimated').replace('{pct}', pct(discretization.relativeError))}</span>
      {/if}
    </div>

    {#if warn}
      <div class="warning" data-testid="discretization-warning">{t('discretization.warning').replace('{threshold}', threshold)}</div>
    {/if}

    {#if discretization && discretization.frameMembers > 0}
      <div class="note">{t('discretization.framesExact')}</div>
    {/if}

    {#if discretization}
      <label class="toggle">
        <input type="checkbox" bind:checked={discretizationStore.showMap} data-testid="discretization-map-toggle" />
        {t('discretization.showMap')}
      </label>
      {#if discretizationStore.showMap}
        <div class="legend">
          <div class="bar" style:background={legend.gradient}></div>
          <div class="ticks"><span>0 %</span><span>{threshold} %</span><span>{legend.max.toFixed(0)} %</span></div>
          <div class="note">{t('discretization.mapExplain')}</div>
        </div>
      {/if}
    {/if}

    <div class="study">
      {#if study.status === 'running'}
        <span>{t('discretization.studyRunning')}</span>
      {:else}
        <button class="link" disabled={!discretizationStore.canRunStudy()} onclick={() => discretizationStore.runStudy()} data-testid="discretization-run-study">
          {t('discretization.runStudy')}
        </button>
      {/if}
      {#if study.status === 'failed'}
        <div class="note">{t('discretization.studyFailed').replace('{reason}', t(`discretization.refuse.${study.reason}`) === `discretization.refuse.${study.reason}` ? study.reason : t(`discretization.refuse.${study.reason}`))}</div>
      {:else if study.status === 'done'}
        <table data-testid="discretization-study">
          <thead>
            <tr>
              <th>{t('discretization.col.quantity')}</th>
              {#each study.study.levels as _l, i}<th>{i === 0 ? t('discretization.col.model') : `h/${2 ** i}`}</th>{/each}
              <th>{t('discretization.col.extrapolated')}</th>
              <th>{t('discretization.col.order')}</th>
              <th>{t('discretization.col.error')}</th>
            </tr>
          </thead>
          <tbody>
            {#each study.study.quantities as q}
              <tr class:bad={q.status === 'notConverging' || (q.estimatedError ?? q.relativeChange) > study.study.threshold}>
                <td>{quantityLabel(q)}</td>
                {#each q.values as v}<td>{num(v)}</td>{/each}
                <td>{num(q.extrapolated)}</td>
                <td>{q.observedOrder === null ? '—' : q.observedOrder.toFixed(2)}</td>
                <td title={t(`discretization.status.${q.status}`)}>
                  {q.estimatedError !== null ? `~${pct(q.estimatedError)} %` : t(`discretization.status.${q.status}`)}
                </td>
              </tr>
            {/each}
          </tbody>
        </table>
        <div class="note">{t('discretization.studyNote').replace('{n}', study.study.levels.map((l) => l.nodes).join(' → '))}</div>
        {#each study.study.caveats as c}
          <div class="note">{t(`discretization.caveat.${c}`)}</div>
        {/each}
      {/if}
    </div>
  </div>
{/if}

<style>
  .disc { margin: 4px 0; padding: 3px 7px; border: 1px solid var(--st-hair); border-radius: 4px; font-size: 0.66rem; line-height: 1.35; color: var(--st-text-2); }
  .disc.warn { border-color: var(--st-warn); }
  .line { cursor: help; }
  .sep { margin: 0 4px; opacity: 0.6; }
  .warning { margin-top: 3px; color: var(--st-warn); font-weight: 600; }
  .note { margin-top: 2px; opacity: 0.8; }
  .toggle { display: flex; gap: 4px; align-items: center; margin-top: 3px; cursor: pointer; }
  .legend { margin-top: 3px; }
  .bar { height: 8px; border-radius: 2px; }
  .ticks { display: flex; justify-content: space-between; font-size: 0.6rem; }
  .study { margin-top: 3px; }
  .link { padding: 0; border: none; background: none; color: var(--st-accent); font: inherit; text-decoration: underline; cursor: pointer; }
  .link:disabled { opacity: 0.5; cursor: default; }
  table { margin-top: 3px; border-collapse: collapse; width: 100%; }
  th, td { padding: 1px 3px; text-align: right; border-bottom: 1px solid var(--st-hair); white-space: nowrap; }
  th:first-child, td:first-child { text-align: left; white-space: normal; }
  tr.bad td { color: var(--st-warn); }
</style>
