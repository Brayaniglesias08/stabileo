/**
 * The timelapse video's recorder: the picture's capture track ends with the recording, a
 * recorder the browser refuses comes back as null (for the panel to say so and leave
 * "recording" off), and the sound's track is left for the next recording.
 */
import { describe, expect, it, vi } from 'vitest';
import { startRecording } from '../timelapse-recording';

function fakes() {
  const video = { kind: 'video', stop: vi.fn() };
  const audio = { kind: 'audio', stop: vi.fn() };
  const tracks: Array<typeof video> = [video];
  const stream = { getVideoTracks: () => tracks.filter((t) => t.kind === 'video'), addTrack: (t: typeof audio) => { tracks.push(t); } };
  const picture = { width: 1920, height: 1080, captureStream: () => stream as unknown as MediaStream };
  return { video, audio, picture };
}

class FakeRecorder {
  static last: FakeRecorder | null = null;
  ondataavailable: ((e: { data: Blob }) => void) | null = null;
  onstop: (() => void) | null = null;
  state = 'inactive';
  constructor(readonly stream: unknown, readonly options: unknown) { FakeRecorder.last = this; }
  start() { this.state = 'recording'; }
  stop() { this.state = 'inactive'; this.onstop?.(); }
}

describe('the timelapse recorder', () => {
  it('stops the picture\'s capture track when the recording ends, and keeps the sound\'s', () => {
    const { video, audio, picture } = fakes();
    const onStop = vi.fn(), onStart = vi.fn();
    const rec = startRecording({ picture, mime: 'video/webm', audio: audio as unknown as MediaStreamTrack, onData: () => {}, onStop, onStart, Recorder: FakeRecorder as unknown as typeof MediaRecorder });
    expect(rec).not.toBeNull();
    expect(onStart).toHaveBeenCalledOnce();
    expect(video.stop).not.toHaveBeenCalled();
    rec!.stop();
    expect(video.stop).toHaveBeenCalledOnce();
    expect(audio.stop).not.toHaveBeenCalled();
    expect(onStop).toHaveBeenCalledOnce();
  });

  it('a recorder the browser refuses to make is null, and its track is stopped', () => {
    const { video, picture } = fakes();
    class Refused { constructor() { throw new Error('NotSupportedError'); } }
    const onStart = vi.fn();
    expect(startRecording({ picture, mime: 'video/mp4', audio: null, onData: () => {}, onStop: () => {}, onStart, Recorder: Refused as unknown as typeof MediaRecorder })).toBeNull();
    expect(video.stop).toHaveBeenCalledOnce();
    expect(onStart).not.toHaveBeenCalled();
  });

  it('a recorder that will not start is null, and its track is stopped', () => {
    const { video, picture } = fakes();
    class WontStart extends FakeRecorder { override start() { throw new Error('InvalidStateError'); } }
    expect(startRecording({ picture, mime: 'video/mp4', audio: null, onData: () => {}, onStop: () => {}, Recorder: WontStart as unknown as typeof MediaRecorder })).toBeNull();
    expect(video.stop).toHaveBeenCalledOnce();
  });
});
