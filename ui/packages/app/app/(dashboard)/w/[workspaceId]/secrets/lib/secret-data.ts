// Single source of truth for the credential-body contract shared by every
// write path: a credential's `data` must be a non-empty JSON object, and a
// credential name is at most SECRET_NAME_MAX chars. AddSecretForm (zod),
// EditSecretDialog (rotate), and RenameSecretDialog (plain state) all validate
// against this so the write paths can never drift on what they accept or the
// messages they show.

export const SECRET_NAME_MAX = 64;
// The vault never returns plaintext, so every write path that re-stores an
// existing secret (Edit/rotate, Rename) asks the user to re-enter the value.
// Shared so the rotate and rename dialogs never drift on the empty-input copy.
export const SECRET_DATA_REENTER_REQUIRED = "Re-enter the secret as a JSON object";
export const SECRET_DATA_NOT_OBJECT =
  "Data must be a JSON object — strings, arrays, and scalars are rejected";
export const SECRET_DATA_EMPTY_OBJECT = "Object must have at least one field";
export const SECRET_DATA_MALFORMED_JSON =
  "Couldn't parse that as JSON — check for a missing quote, brace, or comma.";

// `JSON.parse`'s native `SyntaxError.message` is engine-specific ("Unexpected
// token o in JSON at position 1") and never a sentence a non-engineer should
// read, so it's discarded in favor of one fixed friendly message regardless
// of the actual thrown value. Exported so the branch is exercised directly
// without round-tripping a throw.
export function jsonParseErrorMessage(_err: unknown): string {
  return SECRET_DATA_MALFORMED_JSON;
}

export type ParsedSecretData =
  | { ok: true; data: Record<string, unknown> }
  | { ok: false; message: string };

/**
 * Parse + shape-check a credential `data` body. `requiredMessage` is the
 * caller's wording for an empty input (Add vs Edit phrase it differently); the
 * malformed/not-object/empty-object messages are shared so the contract is
 * identical across write paths.
 */
export function parseSecretDataObject(raw: string, requiredMessage: string): ParsedSecretData {
  const trimmed = raw.trim();
  if (trimmed === "") return { ok: false, message: requiredMessage };
  let parsed: unknown;
  try {
    parsed = JSON.parse(trimmed);
  } catch (err) {
    return { ok: false, message: jsonParseErrorMessage(err) };
  }
  if (parsed === null || typeof parsed !== "object" || Array.isArray(parsed)) {
    return { ok: false, message: SECRET_DATA_NOT_OBJECT };
  }
  if (Object.keys(parsed).length === 0) {
    return { ok: false, message: SECRET_DATA_EMPTY_OBJECT };
  }
  return { ok: true, data: parsed as Record<string, unknown> };
}
