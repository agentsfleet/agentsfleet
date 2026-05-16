/**
 * ID format validation utilities.
 *
 * Single source of truth: every server-generated id is a uuidv7
 * (`src/types/id_format.zig → allocUuidV7`). The CLI validates the same
 * shape client-side so a malformed id is rejected before any network
 * call — saves a round-trip and never stresses the API with garbage.
 *
 * The `uuid` npm package (Apache-2.0, no postinstall, single dep tree)
 * provides `validate` + `version` — pinned in package.json, vetted as
 * acceptable supply-chain posture for runtime use.
 */

import { validate as isValidUuid, version as uuidVersion } from "uuid";

export const EXAMPLE_UUIDV7 = "0192a3b4-c5d6-7e8f-9012-345678901234";

export type ValidateResult =
  | { ok: true }
  | { ok: false; message: string };

export function isValidId(value: unknown): value is string {
  if (!value || typeof value !== "string") return false;
  if (!isValidUuid(value)) return false;
  return uuidVersion(value) === 7;
}

export function validateRequiredId(
  value: unknown,
  name: string,
): ValidateResult {
  if (!value || typeof value !== "string" || value.trim().length === 0) {
    return { ok: false, message: `${name} is required` };
  }
  if (!isValidId(value)) {
    return {
      ok: false,
      message: `invalid ${name}: expected uuidv7 format (e.g. ${EXAMPLE_UUIDV7})`,
    };
  }
  return { ok: true };
}
