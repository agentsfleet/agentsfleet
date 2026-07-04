// Single source of truth for the credential-body contract shared by every
// write path: a credential's `data` must be a non-empty JSON object, and a
// credential name is at most SECRET_NAME_MAX chars. AddSecretForm (zod)
// and EditSecretDialog (plain state) both validate against this so the two
// can never drift on what they accept or the messages they show.

export const SECRET_NAME_MAX = 64;
export const SECRET_DATA_NOT_OBJECT =
  "Data must be a JSON object — strings, arrays, and scalars are rejected";
export const SECRET_DATA_EMPTY_OBJECT = "Object must have at least one field";

// `JSON.parse` only ever throws `SyntaxError` (an `Error`), so the fallback is
// a belt-and-braces default for the `unknown` catch binding. Exported so the
// branch is exercised directly without round-tripping a non-Error throw.
export function jsonParseErrorMessage(err: unknown): string {
  return err instanceof Error ? err.message : "Invalid JSON";
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
    return { ok: false, message: `Invalid JSON: ${jsonParseErrorMessage(err)}` };
  }
  if (parsed === null || typeof parsed !== "object" || Array.isArray(parsed)) {
    return { ok: false, message: SECRET_DATA_NOT_OBJECT };
  }
  if (Object.keys(parsed).length === 0) {
    return { ok: false, message: SECRET_DATA_EMPTY_OBJECT };
  }
  return { ok: true, data: parsed as Record<string, unknown> };
}
