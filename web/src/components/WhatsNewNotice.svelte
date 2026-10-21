<script lang="ts">
  /**
   * Once per results-changing release: says that results may have changed and links the notes.
   * Logic and storage in `lib/release/whats-new.ts`. Not shown to automated browsers, so it
   * cannot sit over what an end-to-end test clicks.
   */
  import { t, i18n } from '../lib/i18n';
  import { APP_VERSION } from '../lib/build-info';
  import { RESULTS_CHANGED_IN, shouldShowWhatsNew, dismissWhatsNew, releaseNotesUrl } from '../lib/release/whats-new';

  const automated = typeof navigator !== 'undefined' && navigator.webdriver === true;
  let open = $state(!automated && shouldShowWhatsNew());

  function dismiss() {
    dismissWhatsNew();
    open = false;
  }
</script>

{#if open}
  <div class="whats-new" role="region" aria-label={t('whatsNew.title').replace('{version}', RESULTS_CHANGED_IN)} data-testid="whats-new">
    <strong class="title">{t('whatsNew.title').replace('{version}', RESULTS_CHANGED_IN)}</strong>
    <p>{t('whatsNew.body')}</p>
    <div class="actions">
      <a class="btn primary" href={releaseNotesUrl(i18n.locale)} target="_blank" rel="noopener noreferrer" onclick={dismiss}>{t('whatsNew.read')}</a>
      <button class="btn" onclick={dismiss}>{t('whatsNew.dismiss')}</button>
      <span class="version">v{APP_VERSION}</span>
    </div>
  </div>
{/if}

<style>
  .whats-new {
    position: fixed;
    left: 16px;
    bottom: 40px;
    z-index: 900;
    max-width: min(26rem, calc(100vw - 32px));
    box-sizing: border-box;
    padding: 0.8rem 0.9rem;
    background: var(--st-surface-2, #1e2430);
    color: var(--st-text, #e6e6e6);
    border: 1px solid var(--st-warn, #d9a441);
    border-radius: var(--st-radius, 6px);
    box-shadow: 0 6px 24px rgba(0, 0, 0, 0.35);
    font-family: var(--st-sans, system-ui, sans-serif);
    font-size: 0.8rem;
    line-height: 1.4;
  }
  .title { display: block; color: var(--st-warn, #d9a441); margin-bottom: 0.35rem; }
  p { margin: 0 0 0.6rem; color: var(--st-text-2, #c8c8c8); }
  .actions { display: flex; align-items: center; gap: 0.5rem; flex-wrap: wrap; }
  .btn {
    font: inherit;
    font-size: 0.75rem;
    padding: 0.3rem 0.7rem;
    border-radius: var(--st-radius, 6px);
    border: 1px solid var(--st-hair-strong, #555);
    background: transparent;
    color: var(--st-text, #e6e6e6);
    cursor: pointer;
    text-decoration: none;
  }
  .btn:hover { background: var(--st-surface-3, #2a3140); }
  .btn.primary { border-color: var(--st-warn, #d9a441); }
  .version { margin-left: auto; color: var(--st-text-3, #8a8a8a); font-family: var(--st-mono, monospace); font-size: 0.7rem; }
</style>
