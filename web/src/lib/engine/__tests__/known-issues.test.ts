import { describe, it, expect, afterEach } from 'vitest';
import { knownIssueMessage } from '../known-issues';
import { setLocale } from '../../i18n';

afterEach(() => setLocale('en'));

describe('known-issue warnings in the reader’s language', () => {
  const d = { code: 'known_issue', phase: 'SH-6', value: 410, message: 'engine English (known issue #410, SH-6; being fixed)' };

  it('shows the app’s sentence for the id, with the issue number', () => {
    setLocale('es');
    const es = knownIssueMessage(d);
    expect(es).toMatch(/láminas curvas/);
    expect(es).toMatch(/#410/);
    setLocale('en');
    expect(knownIssueMessage(d)).toMatch(/Curved shells/);
  });

  it('keeps the engine’s text for an id the app does not know, and leaves other codes alone', () => {
    expect(knownIssueMessage({ ...d, phase: 'XX-99' })).toBe(d.message);
    expect(knownIssueMessage({ code: 'residual_high', message: 'r' })).toBe('r');
  });

  it('has a sentence for every finding the engine contains, in es, en and pt', () => {
    for (const id of ['NL-5', 'SP-12', 'FR-4', 'SH-6', 'SH-7', 'SP-15', 'AC-12', 'AC-13', 'AC-16', 'PC-3D']) {
      for (const loc of ['es', 'en', 'pt'] as const) {
        setLocale(loc);
        expect(knownIssueMessage({ code: 'known_issue', phase: id, message: 'ENGINE' }), `${id} ${loc}`).not.toBe('ENGINE');
      }
    }
  });
});
