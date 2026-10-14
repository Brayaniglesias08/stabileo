/**
 * Opening the report leaves the verification tab's results alone.
 */
import { describe, it, expect } from 'vitest';
import { readFileSync } from 'node:fs';

describe('report export', () => {
  it('opening the report does not write the verification store', () => {
    const src = readFileSync(new URL('../../../components/pro/ProPanel.svelte', import.meta.url), 'utf8');
    expect(src).not.toMatch(/verificationStore\.setConcrete/);
  });

  it('the report is the printable document only: the spreadsheet is Export › Excel', () => {
    const src = readFileSync(new URL('../report-export.ts', import.meta.url), 'utf8');
    expect(src).not.toMatch(/downloadProjectWorkbook|xlsx/);
  });
});
