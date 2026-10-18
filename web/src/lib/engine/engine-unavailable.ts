/**
 * The engine is the only source of engineering results.
 *
 * When the WASM engine is not loaded (init not finished, a missing build, a failed fetch) a
 * computation that needs it throws `EngineUnavailableError`, and the screen that asked shows
 * "engine not available". It never falls back to a TypeScript duplicate: those sit outside the
 * engine's verification (docs/verification/TS_ENGINE_LOGIC.md) and are kept only as test
 * oracles.
 *
 * When the engine is loaded but refuses or fails, `EngineCheckError` carries the engine's own
 * message, so the reader sees why and not a blank.
 */

export class EngineUnavailableError extends Error {
  readonly code = 'ENGINE_UNAVAILABLE' as const;
  /** What was asked for, in English, for the message. */
  readonly what: string;
  constructor(what: string) {
    super(`The engine is not available, so ${what} was not computed.`);
    this.name = 'EngineUnavailableError';
    this.what = what;
  }
}

export class EngineCheckError extends Error {
  readonly code = 'ENGINE_ERROR' as const;
  /** The engine's message, as it gave it. */
  readonly engineMessage: string;
  constructor(what: string, engineMessage: string) {
    super(`The engine could not compute ${what}: ${engineMessage}`);
    this.name = 'EngineCheckError';
    this.engineMessage = engineMessage;
  }
}

/** Throw `EngineUnavailableError` unless `ready`. */
export function requireEngine(ready: boolean, what: string): void {
  if (!ready) throw new EngineUnavailableError(what);
}

export function isEngineUnavailable(e: unknown): e is EngineUnavailableError {
  return e instanceof EngineUnavailableError || (e as { code?: unknown } | null)?.code === 'ENGINE_UNAVAILABLE';
}

/** The text to show for a failed engine computation: the engine's message, or "not available". */
export function engineFailureText(e: unknown): string {
  if (e instanceof EngineCheckError) return e.engineMessage;
  if (e instanceof Error) return e.message;
  return String(e);
}
