import { describe, it, expect } from 'vitest';
import { loadCasesInUse } from '../load-cases-in-use';

const load = (caseId?: number) => ({ data: { caseId } });

describe('Basic opens a model with load cases on when its loads use more than one', () => {
  it('one case, or none written (case 1), is off; two cases are on', () => {
    expect(loadCasesInUse([])).toBe(false);
    expect(loadCasesInUse([load(), load(1), load(1)])).toBe(false);
    expect(loadCasesInUse([load(2), load(2)])).toBe(false);
    expect(loadCasesInUse([load(), load(2)])).toBe(true);
  });
});
