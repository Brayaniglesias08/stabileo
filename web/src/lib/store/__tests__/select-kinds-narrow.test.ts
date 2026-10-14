/**
 * Narrowing the kinds a selection takes lets go of what was selected of the kinds left out, in
 * PRO: with Multi on, members and supports selected, turning Multi off left the support lit
 * behind the panel, and Delete removed it. A tab that switches the kind keeps the selection.
 */
import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { uiStore } from '../ui.svelte';

beforeEach(() => {
  uiStore.analysisMode = 'pro';
  uiStore.multiKindSelect = false;
  uiStore.selectMode = 'elements';
  uiStore.clearSelection();
});
afterEach(() => { uiStore.multiKindSelect = false; uiStore.clearSelection(); });

/** Multi on, members and supports armed, one of each selected. */
function membersAndSupports() {
  uiStore.multiKindSelect = true;
  uiStore.toggleSelectKind('supports');
  uiStore.setSelection(new Set(), new Set([1]), true);
  uiStore.selectedSupports = new Set([3]);
}

describe('narrowing the armed kinds', () => {
  it('Multi off keeps the primary kind and drops the others', () => {
    membersAndSupports();
    uiStore.multiKindSelect = false;
    expect(uiStore.selectsKind('supports')).toBe(false);
    expect([...uiStore.selectedSupports]).toEqual([]);
    expect([...uiStore.selectedElements]).toEqual([1]);
  });

  it('unticking a kind with Multi on drops its selection', () => {
    membersAndSupports();
    uiStore.toggleSelectKind('supports');
    expect([...uiStore.selectedSupports]).toEqual([]);
    expect([...uiStore.selectedElements]).toEqual([1]);
  });

  it('a tab that switches the kind keeps the selection (the context menu opens a card on it)', () => {
    uiStore.setSelection(new Set([7]), new Set());
    uiStore.proActiveTab = 'supports';
    expect([...uiStore.selectedNodes]).toEqual([7]);
  });
});
