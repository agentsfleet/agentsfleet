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
//
// `CURATED_ERROR_CODES` (below) is the public-facing key list — exported so
// the invariant test in errors.test.ts iterates the live set instead of a
// hand-typed shadow that goes stale the next time someone adds an entry.
const CODE_MAP = {
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
    title: "We couldn't look up that credential",
    body: "Re-store the credential under the same name, or pick a different one.",
  },
  "UZ-CRED-003": {
    title: "That credential already exists",
    body: "Pick a different name, or delete the existing one first.",
  },
  "UZ-ZMB-009": {
    title: "That zombie is in a state that blocks this action",
    body: "Check the current status on the detail page and try the right transition.",
  },
  "UZ-AUTH-001": {
    title: "You need operator access for that",
    body: "Ask a tenant operator or admin to manage API keys.",
  },
  "UZ-AUTH-021": {
    title: "You need platform-admin access for that",
    body: "Only a usezombie platform admin can enroll or view runners.",
  },
  "UZ-REQ-001": {
    title: "That request wasn't valid",
    body: "We reset to the defaults — try again.",
  },
  "UZ-APIKEY-003": {
    title: "We couldn't find that API key",
    body: "It may have already been deleted — refresh the list.",
  },
  "UZ-APIKEY-005": {
    title: "An API key with that name already exists",
    body: "Pick a different name for this tenant.",
  },
  "UZ-APIKEY-006": {
    title: "That API key is already revoked",
    body: "Refresh the list to see its current state.",
  },
  "UZ-APIKEY-007": {
    title: "A revoked key can't be reactivated",
    body: "Mint a new key instead.",
  },
  "UZ-APIKEY-008": {
    title: "Revoke this key before deleting it",
    body: "Revoke it first, then delete the revoked key.",
  },
} as const satisfies Record<string, CodeEntry>;

/** Every code the dashboard currently maps to operator-friendly copy. */
export const CURATED_ERROR_CODES = Object.keys(CODE_MAP) as ReadonlyArray<keyof typeof CODE_MAP>;

/**
 * Named lookup for backend error codes the TS layer mints or branches on.
 * `with-token.ts` mints UZ-AUTH-401 when the server-side Bearer is null;
 * any future TS-side mint goes here so the code string lives in one place
 * and drift between CODE_MAP keys + the minter sites is caught at compile
 * time via `satisfies`.
 */
export const ERROR_CODE = {
  AUTH_401: "UZ-AUTH-401",
  PLATFORM_ADMIN_REQUIRED: "UZ-AUTH-021",
} as const satisfies Record<string, keyof typeof CODE_MAP>;

export function presentError(input: ErrorInput): ErrorPresentation {
  const { errorCode, message, action } = input;
  if (errorCode && errorCode in CODE_MAP) {
    const entry = CODE_MAP[errorCode as keyof typeof CODE_MAP];
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
 * `errorMessage` prop on ConfirmDialog). Joins title + body with `. `.
 *
 * Invariant: every entry in CODE_MAP has a title that does NOT end in
 * terminal punctuation, so we unconditionally insert the period — both
 * the unit test `presentError titles never end in terminal punctuation`
 * and the per-entry tone pass enforce that. A title ending in `.`/`!`/`?`
 * would produce a double-period here; that's intentional load-bearing
 * pressure on the next person adding to CODE_MAP.
 */
export function presentErrorString(input: ErrorInput): string {
  const p = presentError(input);
  if (!p.body) return p.title;
  return `${p.title}. ${p.body}`;
}
