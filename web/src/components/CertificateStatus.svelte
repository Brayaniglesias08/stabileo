<script lang="ts">
  /**
   * Whether the linear static result on screen is certified (`store/certificate.svelte.ts`):
   * a rigorous bound on how far the computed displacements are from the exact solution.
   * Shown only while the base solve is on screen; nothing for any other analysis.
   */
  import { t } from '../lib/i18n';
  import { uiStore } from '../lib/store';
  import { certificateStore, formatBound } from '../lib/store/certificate.svelte';

  const shown = $derived(certificateStore.shown(uiStore.is3DWorkspace));
</script>

{#if shown.status === 'pending'}
  <div class="cert pending" data-testid="certificate-status">{t('certificate.pending')}</div>
{:else if shown.status === 'done'}
  {#if shown.certificate.status === 'certified'}
    <div class="cert ok" title={t('certificate.tooltip')} data-testid="certificate-status">
      {t('certificate.certified')
        .replace('{abs}', formatBound(shown.certificate.absBound))
        .replace('{rel}', formatBound(shown.certificate.relBound))}
    </div>
  {:else}
    <div class="cert no" title={t('certificate.tooltip')} data-testid="certificate-status">
      {t('certificate.notCertified').replace('{reason}', shown.certificate.reason)}
    </div>
  {/if}
{/if}

<style>
  .cert { margin: 4px 0; padding: 3px 7px; border: 1px solid var(--st-hair); border-radius: 4px; font-size: 0.66rem; line-height: 1.35; color: var(--st-text-2); cursor: help; }
  .cert.ok { border-color: var(--st-ok); }
  .cert.no { border-color: var(--st-warn); }
  .cert.pending { opacity: 0.7; cursor: default; }
</style>
