// Primitive type guards shared across the CLI. One definition instead of a
// per-file copy: the guard is trivial, but eighteen private clones of it is
// how the same fix gets made seventeen times.

export const TYPE_STRING = "string" as const;

export const isString = (value: unknown): value is string =>
  typeof value === TYPE_STRING;
