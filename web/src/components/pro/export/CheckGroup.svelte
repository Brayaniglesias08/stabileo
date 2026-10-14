<script lang="ts">
  /**
   * A group of choices in an export dialog: its name as a check that turns the whole group on or
   * off (half-set when only some are), and its items in one column underneath, read top to bottom.
   * An item can carry a short hint, shown under it in a smaller line.
   */
  import type { Snippet } from 'svelte';

  interface Item { id: string; label: string; hint?: string; disabled?: boolean }
  let { title, items, on, toggle, setAll, testid, after }: {
    title: string;
    items: Item[];
    on: (id: string) => boolean;
    toggle: (id: string) => void;
    setAll: (value: boolean) => void;
    testid?: string;
    /** Something under the items: a field that only applies to this group. */
    after?: Snippet;
  } = $props();

  const live = $derived(items.filter((i) => !i.disabled));
  const count = $derived(live.filter((i) => on(i.id)).length);
  const all = $derived(live.length > 0 && count === live.length);
  const some = $derived(count > 0 && !all);
</script>

{#if items.length}
  <div class="cg" data-testid={testid}>
    <label class="cg-head">
      <input type="checkbox" checked={all} indeterminate={some} onchange={() => setAll(!all)} disabled={!live.length} />
      <span>{title}</span>
    </label>
    <div class="cg-items">
      {#each items as it (it.id)}
        <label class="cg-item" class:off={it.disabled}>
          <input type="checkbox" checked={!it.disabled && on(it.id)} disabled={it.disabled} onchange={() => toggle(it.id)} data-testid={testid ? `${testid}-${it.id}` : undefined} />
          <span class="cg-text">{it.label}{#if it.hint}<small>{it.hint}</small>{/if}</span>
        </label>
      {/each}
      {#if after}{@render after()}{/if}
    </div>
  </div>
{/if}

<style>
  .cg { display: flex; flex-direction: column; gap: 3px; min-width: 0; }
  .cg-head { display: flex; align-items: center; gap: 6px; font-size: 0.78rem; font-weight: 600; color: var(--st-text); cursor: pointer; }
  .cg-items { display: flex; flex-direction: column; gap: 3px; padding-left: 20px; }
  .cg-item { display: flex; align-items: flex-start; gap: 6px; color: var(--st-text-2); font-size: 0.78rem; cursor: pointer; line-height: 1.3; }
  .cg-item input, .cg-head input { accent-color: var(--st-accent); margin: 2px 0 0; flex: none; }
  .cg-item.off { opacity: 0.45; cursor: not-allowed; }
  .cg-text { display: flex; flex-direction: column; min-width: 0; }
  .cg-text small { color: var(--st-text-3); font-size: 0.7rem; }
</style>
