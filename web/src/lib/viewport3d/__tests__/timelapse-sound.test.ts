/**
 * The timelapse's audio context lives as long as the timelapse: closing it closes the context,
 * which otherwise kept the page's audio running for the rest of the session. Opening it again
 * from a click makes a new one.
 */
import { afterEach, describe, expect, it, vi } from 'vitest';
import { buildTimelapse } from '../../store/build-timelapse.svelte';
import { timelapseSound } from '../timelapse-sound';

class FakeAudioContext {
  static made: FakeAudioContext[] = [];
  state: 'running' | 'suspended' | 'closed' = 'running';
  sampleRate = 48000;
  currentTime = 0;
  destination = {};
  constructor() { FakeAudioContext.made.push(this); }
  createGain() { return { gain: { value: 1 }, connect: (n: unknown) => n }; }
  createMediaStreamDestination() { return { stream: { getAudioTracks: () => [{ kind: 'audio' }] } }; }
  resume() { this.state = 'running'; return Promise.resolve(); }
  suspend() { this.state = 'suspended'; return Promise.resolve(); }
  close() { this.state = 'closed'; return Promise.resolve(); }
}

describe('the timelapse sound', () => {
  afterEach(() => { vi.unstubAllGlobals(); FakeAudioContext.made = []; });

  it('closing the timelapse closes its audio context; opening it again makes a new one', () => {
    vi.stubGlobal('window', { AudioContext: FakeAudioContext });
    buildTimelapse.open();
    timelapseSound.unlock();
    expect(FakeAudioContext.made).toHaveLength(1);
    expect(timelapseSound.track()).not.toBeNull();
    buildTimelapse.close();
    expect(FakeAudioContext.made[0]!.state).toBe('closed');
    expect(timelapseSound.track()).toBeNull();
    timelapseSound.unlock();
    expect(FakeAudioContext.made).toHaveLength(2);
    expect(FakeAudioContext.made[1]!.state).toBe('running');
    buildTimelapse.close();
  });
});
