import { describe, it, expect } from 'vitest';
import { exportResultsCSV } from '../file';
import { resultsStore } from '..';

describe('the results CSV of a model that could not be solved', () => {
  it('still comes out, and says why', () => {
    resultsStore.clear();
    const csv = exportResultsCSV('At least 1 support is needed');
    expect(csv.split('\n')[0]).toMatch(/^# Stabileo/);
    expect(csv).toContain('# At least 1 support is needed');
    expect(csv).toContain('# The model could not be solved');
  });

  it('with no results and no reason there is nothing to write', () => {
    resultsStore.clear();
    expect(exportResultsCSV()).toBe('');
  });
});
