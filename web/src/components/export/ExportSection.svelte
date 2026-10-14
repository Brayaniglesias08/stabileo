<script lang="ts">
  /**
   * A folded section of an export dialog: closed by default, its title saying in one line what
   * is set inside, so the whole dialog reads at a glance.
   */
  import type { Snippet } from 'svelte';

  let { title, summary, open = false, testid, children }: {
    title: string;
    summary: string;
    open?: boolean;
    testid?: string;
    children: Snippet;
  } = $props();

  // Its own state, bound: a plain `{open}` is set again whenever the summary changes, and a
  // section the reader opened closed under the pointer on the first tick.
  // svelte-ignore state_referenced_locally
  let expanded = $state(open);
</script>

<details class="xs" bind:open={expanded} data-testid={testid}>
  <summary><span class="xs-h">{title}</span><span class="xs-s">{summary}</span></summary>
  <div class="xs-body">{@render children()}</div>
</details>

<style>
  .xs { border-top: 1px solid var(--st-hair); padding: 6px 0 2px; }
  .xs:first-child { border-top: none; }
  summary { display: flex; align-items: baseline; gap: 8px; cursor: pointer; list-style: none; min-width: 0; padding: 2px 0; }
  summary::-webkit-details-marker { display: none; }
  summary::before { content: '▸'; color: var(--st-text-3); font-size: 0.7rem; flex: none; }
  details[open] > summary::before { content: '▾'; }
  .xs-h { flex: none; font-size: 0.82rem; font-weight: 500; }
  .xs-s { min-width: 0; color: var(--st-text-3); font-size: 0.74rem; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .xs-body { display: flex; flex-direction: column; gap: 6px; padding: 8px 0 6px 14px; font-size: 0.8rem; }
  .xs-body :global(label.xs-check) { display: flex; align-items: center; gap: 6px; color: var(--st-text-2); cursor: pointer; }
  .xs-body :global(label.xs-check input) { accent-color: var(--st-accent); }
  .xs-body :global(label.xs-check.off) { opacity: 0.5; cursor: not-allowed; }
  .xs-body :global(.xs-grid) { display: grid; grid-template-columns: 1fr 1fr; gap: 4px 12px; }
  .xs-body :global(.xs-field) { display: grid; grid-template-columns: 7.5em 1fr; align-items: center; gap: 8px; color: var(--st-text-2); }
  .xs-body :global(.xs-field input[type='text']), .xs-body :global(.xs-field select), .xs-body :global(.xs-field textarea) {
    min-width: 0;
    padding: 4px 6px;
    background: var(--st-surface-2);
    border: 1px solid var(--st-hair-strong);
    border-radius: var(--st-radius);
    color: var(--st-text);
    font: inherit;
  }
  .xs-body :global(.xs-field textarea) { resize: vertical; }
  .xs-body :global(.xs-seg) { display: inline-flex; border: 1px solid var(--st-hair-strong); border-radius: var(--st-radius); overflow: hidden; }
  .xs-body :global(.xs-seg button) { padding: 3px 10px; background: var(--st-surface-2); border: none; color: var(--st-text-2); font: inherit; cursor: pointer; }
  .xs-body :global(.xs-seg button + button) { border-left: 1px solid var(--st-hair-strong); }
  .xs-body :global(.xs-seg button.on) { background: var(--st-selected-bg); color: var(--st-text); }
  .xs-body :global(.xs-note) { margin: 0; color: var(--st-text-3); font-size: 0.72rem; }
</style>
