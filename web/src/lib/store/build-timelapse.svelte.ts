/**
 * The build timelapse's state: whether it is on, its options and its clock.
 *
 * The 3D viewport reads it every frame (`tick`) and draws the structure as far as `time` has
 * got; the panel sets the options and plays, pauses, scrubs and records.
 */
import type { BuildOrder } from '../viewport3d/build-timelapse';

/** Seconds the finished structure is held before a loop starts again. */
export const HOLD_S = 2;
/** Seconds the supports take to appear, before the first piece. */
export const LEAD_S = 0.8;

function createBuildTimelapse() {
  let active = $state(false);
  let playing = $state(false);
  let order = $state<BuildOrder>('construction');
  let duration = $state(12);
  let orbit = $state(true);
  let loop = $state(true);
  let cleanBackground = $state(false);
  let recording = $state(false);
  /** Shown on the scrubber; written a few times a second, not every frame. */
  let shownTime = $state(0);
  /** Seconds the build lasts once scheduled (lead + pieces), set by the viewport. */
  let total = $state(0);

  let time = 0;
  let lastNow = -1;
  let lastShown = 0;

  return {
    get active() { return active; },
    get playing() { return playing; },
    get order() { return order; }, set order(v: BuildOrder) { order = v; },
    get duration() { return duration; }, set duration(v: number) { duration = v; },
    get orbit() { return orbit; }, set orbit(v: boolean) { orbit = v; },
    get loop() { return loop; }, set loop(v: boolean) { loop = v; },
    get cleanBackground() { return cleanBackground; }, set cleanBackground(v: boolean) { cleanBackground = v; },
    get recording() { return recording; }, set recording(v: boolean) { recording = v; },
    get shownTime() { return shownTime; },
    get total() { return total; }, set total(v: number) { total = v; },
    /** Seconds into the build, lead included. */
    get time() { return time; },

    open() { active = true; time = 0; shownTime = 0; lastNow = -1; playing = true; },
    close() { active = false; playing = false; recording = false; },
    play() { if (time >= total + HOLD_S) time = 0; lastNow = -1; playing = true; },
    pause() { playing = false; },
    restart() { time = 0; shownTime = 0; lastNow = -1; playing = true; },
    seek(t: number) { time = Math.max(0, Math.min(total, t)); shownTime = time; lastNow = -1; },

    /**
     * Advance the clock to `now` (ms) and say whether a cycle just ended. Called once a frame
     * by the viewport while the timelapse is on.
     */
    tick(now: number): { ended: boolean } {
      let ended = false;
      if (playing) {
        if (lastNow >= 0) time += Math.min(0.1, (now - lastNow) / 1000);
        lastNow = now;
        if (time >= total + HOLD_S) {
          ended = true;
          if (loop && !recording) time = 0;
          else { time = total + HOLD_S; playing = false; }
        }
      }
      if (Math.abs(time - lastShown) > 0.05 || ended) { lastShown = time; shownTime = Math.min(time, total); }
      return { ended };
    },
  };
}

export const buildTimelapse = createBuildTimelapse();
