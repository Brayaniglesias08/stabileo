/**
 * Whether a model's loads are spread over more than one load case: what decides that Basic opens
 * it with load cases and combinations on. A model whose loads all sit in one case opens without
 * them (the default combinations every new model carries do not count), and they can be switched
 * on afterwards.
 */
export function loadCasesInUse(loads: Iterable<{ data: { caseId?: number } }>): boolean {
  const cases = new Set<number>();
  for (const l of loads) {
    cases.add(l.data.caseId ?? 1);
    if (cases.size > 1) return true;
  }
  return false;
}
