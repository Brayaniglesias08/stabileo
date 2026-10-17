<script lang="ts">
  /**
   * The steel members in tension, with yield (D.2.1) and rupture on Ae = An·U (D.2.2), from the
   * analysis on hand — the design result the end-connection data exists for.
   *
   * Each row is the checker's own tension result (`checkSteelMember`, the same call the optimiser
   * makes), so the number here and the one behind a profile choice cannot differ. A member whose
   * end is not described stays NOT EVALUATED, with the end named and a button that selects it and
   * brings the end-connection editor into view. A member that is described shows its rupture
   * ratio, and the calculation — connection, An, U, Ae, each with its clause — opens beneath it.
   *
   * Experimental like every CIRSOC 301 number in the app: never a pass, never green.
   */
  import { t, tp } from '../../../lib/i18n';
  import { modelStore, resultsStore, uiStore } from '../../../lib/store';
  import { membersFor } from '../../../lib/store/steel-optimise.svelte';
  import { checkSteelMember } from '../../../lib/engine/verification-service';
  import { memberNetArea, type TensionSectionInput } from '../../../lib/engine/steel/tension-connection';

  let { steelIds }: { steelIds: ReadonlySet<number> } = $props();

  const hasDemands = $derived(resultsStore.results3D !== null);

  interface Row {
    id: number;
    Nt: number;
    state: 'ok' | 'missing' | 'refused' | 'inputs';
    missing?: string;
    reason?: string;
    phiPn?: number;
    ratio?: number;
    governs?: 'yield' | 'rupture';
    An?: number; U?: number; Ae?: number;
    steps?: string[];
  }

  const rows = $derived.by((): Row[] => {
    void modelStore.modelVersion;
    if (!resultsStore.results3D) return [];
    const { members, materialOf } = membersFor([...steelIds]);
    const out: Row[] = [];
    for (const m of members) {
      if (!(m.demand.Nt > 0)) continue;
      const e = modelStore.elements.get(m.elementId);
      const sec = e && modelStore.sections.get(e.sectionId);
      const mat = materialOf.get(m.elementId);
      if (!sec || !mat) continue;
      // Tension alone: the rupture and yield limits do not depend on the moments.
      const v = checkSteelMember(m.elementId, { ...m.demand, Nc: 0, MuStrong: 0, MuWeak: 0, Vu: 0, diagram: [] }, sec as never, mat, m.lengths);
      if (!v || !v.tension) { out.push({ id: m.elementId, Nt: m.demand.Nt, state: 'inputs' }); continue; }
      const net = memberNetArea(sec as TensionSectionInput, m.lengths.tensionEnds ?? []);
      if ('missing' in net) {
        out.push({ id: m.elementId, Nt: m.demand.Nt, state: 'missing', missing: net.missing.map((x) => `#${x.elementId} ${x.end.toUpperCase()}`).join(', ') });
        continue;
      }
      if ('refused' in net) {
        out.push({ id: m.elementId, Nt: m.demand.Nt, state: 'refused', reason: `#${net.at.elementId} ${net.at.end.toUpperCase()}: ${t(`steel.tc.refuse.${net.refused.code}`)}` });
        continue;
      }
      const gov = net.perEnd.find((p) => p.at === net.governing)!.result;
      const yieldPhi = 0.9 * (mat.fy ?? 0) * (sec.a ?? 0) * 1000;
      out.push({
        id: m.elementId, Nt: m.demand.Nt, state: 'ok',
        phiPn: v.tension.phiPn, ratio: v.tension.ratio,
        governs: v.tension.phiPn < yieldPhi - 1e-9 ? 'rupture' : 'yield',
        An: gov.An, U: gov.U, Ae: gov.Ae, steps: v.tension.steps,
      });
    }
    return out.sort((a, b) => a.id - b.id);
  });

  let open = $state<number | null>(null);

  function addConnection(id: number) {
    uiStore.setSelection(new Set(), new Set([id]), true);
    // The editor sits in the geometry block of this same workflow.
    requestAnimationFrame(() => document.querySelector('[data-testid="tc-editor"]')?.scrollIntoView({ block: 'center', behavior: 'smooth' }));
  }

  const mm2 = (v: number) => (v * 1e6).toFixed(0);
</script>

<div class="tr" data-testid="tc-results">
  <h5 class="tr-title">{t('steel.tc.results.title')}</h5>
  <p class="line">{t('steel.tc.results.hint')}</p>
  <p class="line warn">{t('steel.tc.results.experimental')}</p>
  {#if !hasDemands}
    <p class="line" data-testid="tc-results-solve">{t('steel.tc.results.solveFirst')}</p>
  {:else if rows.length === 0}
    <p class="line" data-testid="tc-results-none">{t('steel.tc.results.none')}</p>
  {:else}
    <table class="rows">
      <thead><tr>
        <th>{t('steel.tc.col.member')}</th><th>Nt (kN)</th><th>An</th><th>U</th><th>Ae</th><th>φPn (kN)</th><th>{t('steel.tc.col.status')}</th>
      </tr></thead>
      <tbody>
        {#each rows as r (r.id)}
          <tr data-testid="tc-result-{r.id}" data-state={r.state}>
            <td>#{r.id}</td>
            <td class="num">{r.Nt.toFixed(1)}</td>
            {#if r.state === 'ok'}
              <td class="num">{mm2(r.An!)} mm²</td>
              <td class="num">{r.U!.toFixed(3)}</td>
              <td class="num">{mm2(r.Ae!)} mm²</td>
              <td class="num">{r.phiPn!.toFixed(1)}</td>
              <td>
                <span class="ratio" data-testid="tc-result-ratio-{r.id}">{(r.ratio! * 100).toFixed(0)} %</span>
                <span class="dim">{tp('steel.tc.results.governs', { which: t(`steel.tc.results.${r.governs}`) })}</span>
                <button class="link" onclick={() => { open = open === r.id ? null : r.id; }} data-testid="tc-result-steps-{r.id}">{t('steel.tc.results.steps')}</button>
              </td>
            {:else}
              <td colspan="4" class="ne">
                <strong>{t('steel.tc.results.notEvaluated')}</strong> —
                {#if r.state === 'missing'}{tp('steel.tc.results.missing', { ends: r.missing ?? '' })}
                {:else if r.state === 'refused'}{r.reason}
                {:else}{t('steel.tc.results.inputs')}{/if}
              </td>
              <td>
                {#if r.state !== 'inputs'}
                  <button class="pk-btn pk-btn-primary" onclick={() => addConnection(r.id)} data-testid="tc-result-add-{r.id}">{t('steel.tc.results.add')}</button>
                {/if}
              </td>
            {/if}
          </tr>
          {#if open === r.id && r.steps}
            <tr class="steps"><td colspan="7"><pre data-testid="tc-result-pre-{r.id}">{r.steps.join('\n')}</pre></td></tr>
          {/if}
        {/each}
      </tbody>
    </table>
  {/if}
</div>

<style>
  .tr { margin-top: 8px; }
  .tr-title { margin: 0 0 2px; font-size: 0.72rem; }
  .num { font-family: var(--st-mono); text-align: right; white-space: nowrap; }
  .ne { color: var(--st-warn); }
  .ratio { font-family: var(--st-mono); }
  .dim { color: var(--st-text-3); font-size: 0.6rem; margin-left: 4px; }
  .link { background: none; border: none; color: var(--st-accent); cursor: pointer; font-size: 0.62rem; padding: 0 0 0 6px; text-decoration: underline; }
  pre { margin: 0; white-space: pre-wrap; font-size: 0.6rem; font-family: var(--st-mono); }
</style>
