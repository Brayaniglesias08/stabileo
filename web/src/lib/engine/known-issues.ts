/**
 * The engine's known-issue warnings (engine/src/known_issues.rs) in the reader's language.
 *
 * The engine writes them in English for the API and the MCP server, with the finding's id in
 * `phase` and the issue number in `value`. The app shows its own sentence for the id, so a
 * Spanish or Portuguese reader is warned in their language; an id the app does not know yet
 * keeps the engine's English rather than nothing.
 */
import { t } from '../i18n';

export function knownIssueMessage(d: { code: string; phase?: string; value?: number; message: string }): string {
  if (d.code !== 'known_issue' || !d.phase) return d.message;
  const key = `knownIssue.${d.phase}`;
  const text = t(key);
  if (text === key) return d.message;
  return d.value !== undefined ? `${text} (#${d.value})` : text;
}
