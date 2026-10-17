/**
 * Starting the build timelapse's video: the picture's stream (and the sound's track) into a
 * MediaRecorder.
 *
 * Two things the panel's own code did not do:
 * - the picture's capture track was never stopped, so every recording left one live, fed from
 *   the canvas, for the rest of the session;
 * - a recorder the browser refuses (its constructor or `start` throwing: a type it lists but
 *   cannot open, a stream it does not take) threw out of the click, with "recording" already on.
 *
 * Here the video track is stopped when the recording ends or fails to start (the sound's track
 * is the sound's, kept for the next one), and a refusal comes back as null for the panel to tell.
 */

/** The canvas the video is taken from. */
export interface RecordedPicture {
  width: number;
  height: number;
  captureStream(fps?: number): MediaStream;
}

export function startRecording(o: {
  picture: RecordedPicture;
  mime: string;
  /** The sound's track, mixed in; null for a silent video. */
  audio: MediaStreamTrack | null;
  onData(chunk: Blob): void;
  /** After the recorder has stopped and the picture's track with it. */
  onStop(): void;
  /** Just before the recorder starts: the build starts over with it. */
  onStart?(): void;
  /** The recorder class; the browser's by default. */
  Recorder?: typeof MediaRecorder;
}): MediaRecorder | null {
  let stream: MediaStream | null = null;
  const release = () => { for (const track of stream?.getVideoTracks() ?? []) track.stop(); };
  try {
    stream = o.picture.captureStream(60);
    if (o.audio) stream.addTrack(o.audio);
    const pixels = o.picture.width * o.picture.height;
    const rec = new (o.Recorder ?? MediaRecorder)(stream, {
      mimeType: o.mime,
      // About 6 bits a pixel a second at 60 fps: crisp edges on thin members without huge files.
      videoBitsPerSecond: Math.round(Math.min(20e6, Math.max(6e6, pixels * 6))),
      audioBitsPerSecond: 128_000,
    });
    rec.ondataavailable = (e) => { if (e.data.size > 0) o.onData(e.data); };
    rec.onstop = () => { release(); o.onStop(); };
    o.onStart?.();
    rec.start(250);
    return rec;
  } catch {
    release();
    return null;
  }
}
