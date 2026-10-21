import { describe, it, expect, afterEach } from 'vitest';
import { knownIssueMessage } from '../known-issues';
import { setLocale } from '../../i18n';

afterEach(() => setLocale('en'));

describe('known-issue warnings in the reader’s language', () => {
  const d = { code: 'known_issue', phase: 'SH-7', value: 411, message: 'engine English (known issue #411, SH-7; being fixed)' };

  it('shows the app’s sentence for the id, with the issue number', () => {
    setLocale('es');
    const es = knownIssueMessage(d);
    expect(es).toMatch(/geometría no plana/);
    expect(es).toMatch(/#411/);
    setLocale('en');
    expect(knownIssueMessage(d)).toMatch(/non-planar geometry/);
  });

  it('keeps the engine’s text for an id the app does not know, and leaves other codes alone', () => {
    expect(knownIssueMessage({ ...d, phase: 'XX-99' })).toBe(d.message);
    expect(knownIssueMessage({ code: 'residual_high', message: 'r' })).toBe('r');
  });

  it('has a sentence for every finding the engine contains, in es, en and pt', () => {
    for (const id of ['NL-5', 'FR-4', 'SH-7', 'AC-12', 'AC-13', 'AC-16']) {
      for (const loc of ['es', 'en', 'pt'] as const) {
        setLocale(loc);
        expect(knownIssueMessage({ code: 'known_issue', phase: id, message: 'ENGINE' }), `${id} ${loc}`).not.toBe('ENGINE');
      }
    }
  });
});
