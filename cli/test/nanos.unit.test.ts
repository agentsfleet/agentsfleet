import { expect, test } from "bun:test";
import { Schema } from "effect";
import { isNanos, nanosSchema } from "../src/lib/nanos.ts";

// Pin tests: these literals cover canonical spelling, safe-number overflow and the wire ceiling.
test.each(["0", "1", "9007199254740993", "9223372036854775807"])("retains every nanos digit in %s", value => {
  expect(isNanos(value)).toBe(true);
  expect(Schema.decodeUnknownSync(nanosSchema)(value)).toBe(value);
});
test.each([0, 1, null, undefined, {}, [], true, "", "01", "-1", "1.5", "1e3", "9223372036854775808"].map(value => ({ value })))(
  "refuses malformed nanos %j", ({ value }) => {
    expect(isNanos(value)).toBe(false);
    expect(() => Schema.decodeUnknownSync(nanosSchema)(value)).toThrow();
  },
);
