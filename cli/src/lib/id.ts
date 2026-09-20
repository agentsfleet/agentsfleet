// What counts as an identifier, for every caller on both sides of the parser.
//
// This lived in `program/validators.ts` next to the commander option parsers,
// which made it look like parse-time machinery. It is not: five command
// handlers call `validateRequiredId` at run time, long after parsing, and they
// have to keep working when the commander tree and its parsers are deleted.

import { validate as isValidUuid, version as uuidVersion } from "uuid";
import { isString } from "./guards.ts";

/** A well-formed id, for the "here is what one looks like" half of a refusal. */
export const EXAMPLE_UUIDV7 = "0192a3b4-c5d6-7e8f-9012-345678901234";

const UUID_VERSION_7 = 7;

export type ValidateResult = { ok: true } | { ok: false; message: string };

export const isValidId = (value: unknown): value is string => {
  if (!value || !isString(value)) return false;
  // `uuid`'s validate is case-insensitive, but server ids are canonical
  // lowercase: an uppercase alias is the same row in Postgres and a different
  // key in Dragonfly, so the server rejects it too. Keep the two runtimes
  // agreeing — see `rustd/crates/afd_core/src/id.rs`.
  if (value !== value.toLowerCase()) return false;
  if (!isValidUuid(value)) return false;
  return uuidVersion(value) === UUID_VERSION_7;
};

/**
 * Missing and malformed are two refusals, not one.
 *
 * A caller that collapses them tells someone who forgot the argument that
 * their id is the wrong shape. The two messages are what the handler tests
 * assert on, which is how the distinction stays.
 */
export const validateRequiredId = (value: unknown, name: string): ValidateResult => {
  if (!value || !isString(value) || value.trim().length === 0) {
    return { ok: false, message: `${name} is required` };
  }
  if (!isValidId(value)) {
    return {
      ok: false,
      message: `invalid ${name}: expected uuidv7 format (e.g. ${EXAMPLE_UUIDV7})`,
    };
  }
  return { ok: true };
};
