// Instants as the daemon writes them: RFC 3339 in UTC, the fraction dropped on
// a whole second and otherwise the shortest that names the millisecond
// (`2026-10-17T18:06:49Z`, `2026-10-17T18:06:49.25Z`). One definition, so every
// decoder, table and sort reads the same shape. Not fixed-width, so two of them
// compare by the moment they name, never by their text. A number is refused.

import { Schema } from "effect";
import { isString } from "./guards.ts";

/** The shape alone. `isInstant` also asks the calendar, so `…-13-…` fails. */
const INSTANT_SHAPE = /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d{1,9})?Z$/;

export const isInstant = (value: unknown): value is string =>
  isString(value) && INSTANT_SHAPE.test(value) && !Number.isNaN(Date.parse(value));

/** A wire field decoded as an instant. */
export const instant = Schema.String.check(Schema.isPattern(INSTANT_SHAPE));

/** The epoch millisecond an instant names, or null for anything that is not one. */
export const instantMs = (value: unknown): number | null => (isInstant(value) ? Date.parse(value) : null);

/** Orders two instants by the moment each names. */
export const byInstant = (a: string, b: string): number => Date.parse(a) - Date.parse(b);
