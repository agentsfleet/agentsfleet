/**
 * presentError — single entry point for dashboard error rendering.
 *
 * Every "Failed to <verb>" fallback in the TSX layer should route through
 * here so the operator sees a consistent voice + an actionable next step.
 *
 * Pattern (Captain's call):
 *   - Second person, present tense — "Couldn't delete this zombie" beats
 *     "Failed to delete zombie".
 *   - One line, ending on the next action the operator can take.
 *   - The UZ-XXX-NNN code is surfaced for support but never the lead;
 *     callers render it as a small monospaced trailer beneath the title.
 *
 * No localization: this maps to literal strings. A future i18n PR would
 * swap the map values to lookup keys without changing the call sites.
 */

export interface ErrorInput {
  /** ActionResult.error from a server action, or the thrown Error.message. */
  message?: string;
  /** UZ-XXX-NNN extracted by `withToken` from ApiError.code. */
  errorCode?: string;
  /**
   * What the operator was trying to do, in present-tense lowercase. Used
   * to construct the fallback when no errorCode matches. Examples:
   *   "delete this zombie", "store the credential", "load more events".
   */
  action: string;
}

export interface ErrorPresentation {
  /** Operator-first sentence. Sentence case unless a heading slot is used. */
  title: string;
  /** Optional second line — typically the next action. */
  body?: string;
  /** UZ-XXX-NNN to render as a small trailer. */
  code?: string;
}

interface CodeEntry {
  title: string;
  body?: string;
}

// Curated map. Add codes here as the dashboard surfaces them — every entry
// needs to read like a colleague telling you what happened, not a CLI
// traceback. The fallback at the bottom of presentError handles unmapped
// codes gracefully, so this list grows organically.
const CODE_MAP: Record<string, CodeEntry> = {
  "UZ-AUTH-401": {
    title: "Your session expired",
    body: "Sign in again to keep going.",
  },
  "UZ-INTERNAL-001": {
    title: "Something broke on our end",
    body: "Give it another shot — if it keeps failing, send us the code below.",
  },
  "UZ-INTERNAL-002": {
    title: "We're under load and dropped your request",
    body: "Try again in a few seconds.",
  },
  "UZ-VALIDATION-001": {
    title: "That didn't pass validation",
    body: "Double-check the fields above and resubmit.",
  },
  "UZ-CRED-001": {
    title: "Credential lookup failed",
    body: "Re-store the credential under the same name, or pick a different one.",
  },
  "UZ-CRED-003": {
    title: "That credential already exists",
    body: "Pick a different name, or delete the existing one first.",
  },
  "UZ-ZMB-001": {
    title: "We couldn't find that zombie",
    body: "It may have already been deleted — refresh the list.",
  },
  "UZ-ZMB-009": {
    title: "That zombie is in a state that blocks this action",
    body: "Check the current status on the detail page and try the right transition.",
  },
};

export function presentError(input: ErrorInput): ErrorPresentation {
  const { errorCode, message, action } = input;
  if (errorCode && CODE_MAP[errorCode]) {
    const entry = CODE_MAP[errorCode];
    return { title: entry.title, body: entry.body, code: errorCode };
  }
  // Fallback. If the server gave us a usable message, surface it after the
  // verb-based lead; otherwise fall through to a single-sentence default.
  const trimmed = (message ?? "").trim();
  const lead = `Couldn't ${action}`;
  if (trimmed && !/^failed to/i.test(trimmed)) {
    return { title: `${lead} — ${trimmed.replace(/\.$/, "")}.`, code: errorCode };
  }
  return {
    title: `${lead}. Try again, or check Events for what blocked it.`,
    code: errorCode,
  };
}

/**
 * Convenience for callers that only need a single string (e.g. the
 * `errorMessage` prop on ConfirmDialog). Joins title + body with a space,
 * inserting a trailing period on the title when one is missing so the
 * concatenated sentence flows.
 */
export function presentErrorString(input: ErrorInput): string {
  const p = presentError(input);
  if (!p.body) return p.title;
  const tail = /[.!?]$/.test(p.title) ? "" : ".";
  return `${p.title}${tail} ${p.body}`;
}
