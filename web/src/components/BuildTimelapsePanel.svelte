<script lang="ts">
  /**
   * The build timelapse's controls: play, scrub, order, length, camera turn, and a video of the
   * canvas. The video is what the WebGL canvas draws, so the panel and the rest of the page stay
   * out of it.
   */
  import { buildTimelapse } from '../lib/store/build-timelapse.svelte';
  import { modelStore, uiStore } from '../lib/store';
  import { t, tp } from '../lib/i18n';
  import type { BuildOrder } from '../lib/viewport3d/build-timelapse';

  let { canvas }: { canvas: HTMLCanvasElement | null } = $props();

  const ORDERS: Array<{ id: BuildOrder; key: string }> = [
    { id: 'construction', key: 'timelapse.orderConstruction' },
    { id: 'bottomUp', key: 'timelapse.orderBottomUp' },
    { id: 'along', key: 'timelapse.orderAlong' },
    { id: 'list', key: 'timelapse.orderList' },
  ];
  const DURATIONS = [6, 10, 12, 15, 20, 30, 45];

  const empty = $derived(modelStore.elements.size + modelStore.plates.size + modelStore.quads.size === 0);

  let recorder: MediaRecorder | null = null;
  let chunks: Blob[] = [];
  let keep = true;

  function pickType(): { mime: string; ext: string } | null {
    if (typeof MediaRecorder === 'undefined') return null;
    for (const [mime, ext] of [
      ['video/mp4;codecs=avc1.640028', 'mp4'], ['video/mp4', 'mp4'],
      ['video/webm;codecs=vp9', 'webm'], ['video/webm', 'webm'],
    ] as const) {
      if (MediaRecorder.isTypeSupported(mime)) return { mime, ext };
    }
    return null;
  }

  function record() {
    const type = pickType();
    if (!canvas || !type || typeof canvas.captureStream !== 'function') {
      uiStore.toast(t('timelapse.noRecorder'), 'error');
      return;
    }
    chunks = [];
    keep = true;
    const rec = new MediaRecorder(canvas.captureStream(60), { mimeType: type.mime, videoBitsPerSecond: 16_000_000 });
    rec.ondataavailable = (e) => { if (e.data.size > 0) chunks.push(e.data); };
    rec.onstop = () => {
      recorder = null;
      buildTimelapse.recording = false;
      if (!keep || chunks.length === 0) return;
      const file = `stabileo-armado.${type.ext}`;
      const url = URL.createObjectURL(new Blob(chunks, { type: type.mime }));
      const a = document.createElement('a');
      a.href = url; a.download = file; a.click();
      setTimeout(() => URL.revokeObjectURL(url), 10_000);
      uiStore.toast(tp('timelapse.saved', { file }), 'success');
    };
    recorder = rec;
    buildTimelapse.recording = true;
    buildTimelapse.restart();
    rec.start(250);
  }

  function stopRecording(save: boolean) {
    keep = save;
    if (recorder && recorder.state !== 'inactive') recorder.stop();
    else buildTimelapse.recording = false;
  }

  // The recording ends with the build (and the hold after it).
  $effect(() => {
    if (buildTimelapse.recording && !buildTimelapse.playing) stopRecording(true);
  });
  // Closing the panel drops a recording in progress.
  $effect(() => () => { if (recorder) stopRecording(false); });

  const fmt = (s: number) => `${s.toFixed(1)} s`;
</script>

<div class="tl-panel" data-testid="build-timelapse-panel">
  <div class="tl-row">
    <span class="tl-title">{t('timelapse.title')}</span>
    <button class="tl-x" title={t('timelapse.close')} aria-label={t('timelapse.close')} onclick={() => buildTimelapse.close()}>✕</button>
  </div>

  {#if empty}
    <p class="tl-empty">{t('timelapse.empty')}</p>
  {:else}
    <div class="tl-row">
      <button
        class="tl-btn tl-play"
        data-testid="timelapse-play"
        disabled={buildTimelapse.recording}
        title={buildTimelapse.playing ? t('timelapse.pause') : t('timelapse.play')}
        onclick={() => (buildTimelapse.playing ? buildTimelapse.pause() : buildTimelapse.play())}
      >{buildTimelapse.playing ? '❚❚' : '▶'}</button>
      <button class="tl-btn" title={t('timelapse.restart')} disabled={buildTimelapse.recording} onclick={() => buildTimelapse.restart()}>⟲</button>
      <input
        class="tl-scrub"
        type="range"
        min="0"
        max={buildTimelapse.total || 1}
        step="0.01"
        value={buildTimelapse.shownTime}
        disabled={buildTimelapse.recording}
        oninput={(e) => { buildTimelapse.pause(); buildTimelapse.seek(+e.currentTarget.value); }}
      />
      <span class="tl-time">{fmt(buildTimelapse.shownTime)} / {fmt(buildTimelapse.total)}</span>
    </div>

    <div class="tl-row tl-opts">
      <label class="tl-field">
        <span>{t('timelapse.order')}</span>
        <select value={buildTimelapse.order} disabled={buildTimelapse.recording} onchange={(e) => { buildTimelapse.order = e.currentTarget.value as BuildOrder; buildTimelapse.restart(); }}>
          {#each ORDERS as o (o.id)}<option value={o.id}>{t(o.key)}</option>{/each}
        </select>
      </label>
      <label class="tl-field">
        <span>{t('timelapse.duration')}</span>
        <select value={buildTimelapse.duration} disabled={buildTimelapse.recording} onchange={(e) => { buildTimelapse.duration = +e.currentTarget.value; buildTimelapse.restart(); }}>
          {#each DURATIONS as d (d)}<option value={d}>{d} s</option>{/each}
        </select>
      </label>
      <label class="tl-check"><input type="checkbox" bind:checked={buildTimelapse.orbit} />{t('timelapse.orbit')}</label>
      <label class="tl-check"><input type="checkbox" bind:checked={buildTimelapse.loop} />{t('timelapse.loop')}</label>
      <label class="tl-check"><input type="checkbox" bind:checked={buildTimelapse.cleanBackground} />{t('timelapse.clean')}</label>
    </div>

    <div class="tl-row">
      {#if buildTimelapse.recording}
        <span class="tl-rec-dot"></span><span class="tl-rec">{t('timelapse.recording')}</span>
        <button class="tl-btn tl-wide" onclick={() => stopRecording(true)}>{t('timelapse.stopRecording')}</button>
      {:else}
        <button class="tl-btn tl-wide tl-record" data-testid="timelapse-record" onclick={record}>● {t('timelapse.record')}</button>
      {/if}
    </div>
  {/if}
</div>

<style>
  .tl-panel {
    position: absolute;
    right: 16px;
    bottom: 16px;
    z-index: 22;
    width: min(400px, calc(100% - 32px));
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 10px 12px;
    border: 1px solid var(--st-hair-strong);
    border-radius: var(--st-radius);
    background: var(--st-surface);
    box-shadow: 0 10px 22px -8px rgba(0, 0, 0, 0.55);
    color: var(--st-text);
    font-size: 0.78rem;
  }
  .tl-row { display: flex; align-items: center; gap: 8px; min-width: 0; }
  .tl-opts { flex-wrap: wrap; gap: 6px 12px; }
  .tl-title { flex: 1; font-weight: 600; letter-spacing: 0.02em; }
  .tl-x { background: none; border: none; color: var(--st-text-3); cursor: pointer; font-size: 0.9rem; padding: 2px 4px; }
  .tl-x:hover { color: var(--st-text); }
  .tl-btn {
    flex: none;
    min-width: 32px;
    height: 28px;
    padding: 0 8px;
    border: 1px solid var(--st-hair-strong);
    border-radius: var(--st-radius);
    background: var(--st-surface-2);
    color: var(--st-text);
    cursor: pointer;
  }
  .tl-btn:hover:not(:disabled) { border-color: var(--st-accent); }
  .tl-btn:disabled { opacity: 0.5; cursor: default; }
  .tl-play { color: var(--st-accent); }
  .tl-wide { flex: 1; }
  .tl-record { color: var(--st-danger); }
  .tl-scrub { flex: 1; min-width: 0; accent-color: var(--st-accent); }
  .tl-time { flex: none; font-variant-numeric: tabular-nums; color: var(--st-text-2); font-size: 0.72rem; }
  .tl-field { display: flex; align-items: center; gap: 4px; color: var(--st-text-2); }
  .tl-field select {
    background: var(--st-surface-2);
    color: var(--st-text);
    border: 1px solid var(--st-hair-strong);
    border-radius: var(--st-radius);
    padding: 2px 4px;
    font-size: 0.74rem;
  }
  .tl-check { display: flex; align-items: center; gap: 4px; color: var(--st-text-2); cursor: pointer; }
  .tl-empty { margin: 0; color: var(--st-text-2); }
  .tl-rec { color: var(--st-danger); }
  .tl-rec-dot { width: 8px; height: 8px; border-radius: 50%; background: var(--st-danger); animation: tl-blink 1s infinite; }
  @keyframes tl-blink { 50% { opacity: 0.25; } }
</style>
