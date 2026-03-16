/**
 * ID format validation utilities.
 */

const UUID_RE = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;
const SAFE_ID_RE = /^[a-zA-Z0-9_-]{4,128}$/;

export function isValidId(value) {
  if (!value || typeof value !== "string") return false;
  return UUID_RE.test(value) || SAFE_ID_RE.test(value);
}

export function validateRequiredId(value, name) {
  if (!value || typeof value !== "string" || value.trim().length === 0) {
    return { ok: false, message: `${name} is required` };
  }
  if (!isValidId(value)) {
    return {
      ok: false,
      message: `invalid ${name}: expected UUID format (e.g. 550e8400-e29b-41d4-a716-446655440000) or alphanumeric identifier (4-128 chars)`,
    };
  }
  return { ok: true };
}
