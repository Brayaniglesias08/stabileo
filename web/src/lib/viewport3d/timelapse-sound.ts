/**
 * The build timelapse's sound: a knock each time a piece lands, and a chime when it is done.
 *
 * Synthesised, so there are no files to ship: a handful of short buffers are rendered once per
 * timbre and played back with a slightly different pitch and level each time, which keeps a
 * long build from sounding like a machine gun. The sound goes to the speakers and, through a
 * stream destination, into the video being recorded.
 *
 * - `blocks`: a gritty, dry knock, like a stone block set down in a building game.
 * - `metal`: a short steel clank, a bolt going home.
 * - `soft`: a muted wooden tick.
 */
export type TimelapseSound = 'off' | 'blocks' | 'metal' | 'soft';

/** At most one knock this often; pieces landing closer than that are heard as one, louder. */
const MIN_GAP_S = 0.045;
const VARIANTS = 5;

type Ctx = AudioContext;

function noise(n: number, seed: number): Float32Array {
  // A small deterministic generator, so each variant sounds the same every session.
  let x = seed * 2654435761 >>> 0;
  const out = new Float32Array(n);
  for (let i = 0; i < n; i++) {
    x ^= x << 13; x >>>= 0; x ^= x >>> 17; x ^= x << 5; x >>>= 0;
    out[i] = (x / 4294967296) * 2 - 1;
  }
  return out;
}

/** One knock of the given timbre, as samples. */
function render(kind: Exclude<TimelapseSound, 'off'>, rate: number, v: number): Float32Array {
  const dur = kind === 'metal' ? 0.32 : kind === 'blocks' ? 0.16 : 0.09;
  const n = Math.round(dur * rate);
  const out = new Float32Array(n);
  const nz = noise(n, 17 + v * 31 + (kind === 'metal' ? 7 : kind === 'soft' ? 3 : 0));
  if (kind === 'blocks') {
    // Gravel: noise in a few short grains, darkened, over a low thump.
    let lp = 0, lp2 = 0;
    const cut = 0.22 + v * 0.03;
    const f0 = 115 + v * 14;
    for (let i = 0; i < n; i++) {
      const t = i / rate;
      const grain = 0.55 + 0.45 * Math.abs(Math.sin(t * (260 + v * 40) * Math.PI));
      lp += cut * (nz[i]! - lp);
      lp2 += 0.5 * (lp - lp2);
      const crunch = (lp - lp2 * 0.6) * grain * Math.exp(-t / 0.035);
      const thump = Math.sin(2 * Math.PI * f0 * t * (1 - t * 1.8)) * Math.exp(-t / 0.03);
      out[i] = crunch * 1.5 + thump * 0.55;
    }
  } else if (kind === 'metal') {
    // A few inharmonic partials ringing out over a hard tick.
    const f = 420 + v * 37;
    const partials = [[1, 0.5, 0.11], [2.76, 0.32, 0.07], [5.4, 0.2, 0.04], [8.93, 0.12, 0.025]] as const;
    for (let i = 0; i < n; i++) {
      const t = i / rate;
      let y = 0;
      for (const [r, a, d] of partials) y += a * Math.sin(2 * Math.PI * f * r * t) * Math.exp(-t / d);
      y += nz[i]! * 0.5 * Math.exp(-t / 0.004);
      out[i] = y * 0.8;
    }
  } else {
    // A small pitched pop that drops as it fades.
    const f = 620 + v * 45;
    for (let i = 0; i < n; i++) {
      const t = i / rate;
      out[i] = Math.sin(2 * Math.PI * f * t * (1 - t * 3)) * Math.exp(-t / 0.02) * 0.8 + nz[i]! * 0.12 * Math.exp(-t / 0.006);
    }
  }
  // Ramp the first and last few samples so nothing clicks.
  const edge = Math.min(32, n >> 2);
  for (let i = 0; i < edge; i++) { const g = i / edge; out[i]! *= g; out[n - 1 - i]! *= g; }
  return out;
}

function createTimelapseSound() {
  let ctx: Ctx | null = null;
  let master: GainNode | null = null;
  let dest: MediaStreamAudioDestinationNode | null = null;
  const banks = new Map<string, AudioBuffer[]>();
  let lastAt = -1;
  let owed = 0;

  function bank(kind: Exclude<TimelapseSound, 'off'>): AudioBuffer[] {
    let b = banks.get(kind);
    if (!b) {
      b = [];
      for (let v = 0; v < VARIANTS; v++) {
        const data = render(kind, ctx!.sampleRate, v);
        const buf = ctx!.createBuffer(1, data.length, ctx!.sampleRate);
        buf.copyToChannel(data as Float32Array<ArrayBuffer>, 0);
        b.push(buf);
      }
      banks.set(kind, b);
    }
    return b;
  }

  function play(buf: AudioBuffer, at: number, gain: number, rate: number): void {
    const src = ctx!.createBufferSource();
    src.buffer = buf;
    src.playbackRate.value = rate;
    const g = ctx!.createGain();
    g.gain.value = gain;
    src.connect(g).connect(master!);
    src.start(at);
  }

  return {
    /**
     * Makes sure there is a running audio context. Call it from a click (play, record, the
     * sound picker): browsers only let a page start sound after the user has done something.
     */
    unlock(): void {
      try {
        if (!ctx) {
          const AC = window.AudioContext ?? (window as unknown as { webkitAudioContext?: typeof AudioContext }).webkitAudioContext;
          if (!AC) return;
          ctx = new AC();
          master = ctx.createGain();
          master.gain.value = 0.55;
          dest = ctx.createMediaStreamDestination();
          master.connect(ctx.destination);
          master.connect(dest);
        }
        if (ctx.state === 'suspended') void ctx.resume();
      } catch { /* no audio on this browser: the timelapse plays silent */ }
    },

    /** The sound as a track for the recorder; null before `unlock` or without audio. */
    track(): MediaStreamTrack | null {
      return dest?.stream.getAudioTracks()[0] ?? null;
    },

    /** `count` pieces landed this frame. */
    hits(kind: TimelapseSound, count: number): void {
      if (kind === 'off' || count <= 0 || !ctx || ctx.state !== 'running') return;
      owed += count;
      const now = ctx.currentTime;
      if (lastAt >= 0 && now - lastAt < MIN_GAP_S) return;
      const b = bank(kind);
      // Several at once are one knock a little louder, never a wall of them.
      const gain = Math.min(1, 0.7 + 0.12 * Math.log2(owed)) * (0.85 + Math.random() * 0.15);
      play(b[Math.floor(Math.random() * b.length)]!, now, gain, 0.9 + Math.random() * 0.22);
      lastAt = now;
      owed = 0;
    },

    /** The build is finished: a soft two-note chime. */
    done(kind: TimelapseSound): void {
      if (kind === 'off' || !ctx || ctx.state !== 'running') return;
      const now = ctx.currentTime;
      for (const [f, delay] of [[659.25, 0], [987.77, 0.12]] as const) {
        const osc = ctx.createOscillator();
        osc.type = 'sine';
        osc.frequency.value = f;
        const g = ctx.createGain();
        g.gain.setValueAtTime(0, now + delay);
        g.gain.linearRampToValueAtTime(0.22, now + delay + 0.015);
        g.gain.exponentialRampToValueAtTime(0.0005, now + delay + 1.4);
        osc.connect(g).connect(master!);
        osc.start(now + delay);
        osc.stop(now + delay + 1.5);
      }
    },

    /** Forget the pacing, after a seek or a restart. */
    reset(): void { lastAt = -1; owed = 0; },
  };
}

export const timelapseSound = createTimelapseSound();
