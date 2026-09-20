// Primitive type guards shared across the CLI. One definition instead of a
// per-file copy: the guard is trivial, but eighteen private clones of it is
// how the same fix gets made seventeen times.
//
// That warning was written and then ignored — `isString` was redefined
// privately in thirteen more files, each alongside its own `TYPE_STRING`
// constant, because RULE UFS asks for a named const per file and nothing asks
// whether the file next door already named it. A guard is the shape that
// removes both: the `typeof` tag stops being a value any caller repeats, and
// narrowing arrives at the call site instead of a comparison.

const TYPEOF = {
  string: "string",
  number: "number",
  object: "object",
  boolean: "boolean",
} as const;

export const isString = (value: unknown): value is string =>
  typeof value === TYPEOF.string;

export const isNumber = (value: unknown): value is number =>
  typeof value === TYPEOF.number;

export const isBoolean = (value: unknown): value is boolean =>
  typeof value === TYPEOF.boolean;

/**
 * A non-null object, which is what every caller that wrote
 * `value !== null && typeof value === "object"` meant.
 *
 * An array satisfies this, exactly as the hand-written checks did — callers
 * that need to exclude one reach for `Array.isArray` alongside it rather than
 * inheriting a narrower guard they did not ask for.
 */
export const isRecord = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === TYPEOF.object;
