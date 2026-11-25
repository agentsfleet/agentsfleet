// The prime set: the fixture fleets every journey creates, read from the one
// place they are written, `tests/fixtures/fleets/<name>.json`. A spec takes a
// fleet's name, first message and follow-ups from here and types none of them.

import fs from "node:fs";
import path from "node:path";
import { Schema } from "effect";

/** Where the prime set lives, from the repository root. */
export const PRIME_FLEETS_DIR = path.resolve(import.meta.dir, "..", "..", "..", "..", "tests", "fixtures", "fleets");

const text = Schema.String.check(Schema.isMinLength(1));
const texts = Schema.Array(text);

// What wakes a fleet. Each kind fills its own fields: a schedule its `cron`, a
// webhook its `source` and `secret_name`, an App event its `source` and `events`.
const WakeSchema = Schema.Struct({
  kind: text,
  cron: Schema.optionalKey(text),
  source: Schema.optionalKey(text),
  secret_name: Schema.optionalKey(text),
  events: Schema.optionalKey(texts),
});
export type PrimeWake = typeof WakeSchema.Type;

// One file's fields, as `tests/fixtures/fleets/README.md` lists them. A fleet
// that asks for nothing or wakes on nothing carries an empty list.
const PrimeFleetSchema = Schema.Struct({
  name: text,
  journey: text,
  about: text,
  first_message: text,
  follow_ups: texts,
  remembers: texts,
  asks_for: Schema.Array(Schema.Struct({
    kind: text,
    name: text,
    scope: Schema.optionalKey(Schema.Struct({ access: text })),
  })),
  wakes_on: Schema.Array(WakeSchema),
  steers: texts,
  proves: texts,
  // The GitHub repository a fleet works in, as `owner/name`, when it has one.
  repository: Schema.optionalKey(text),
});

export type PrimeFleet = typeof PrimeFleetSchema.Type;

/** A fixture file's parsed body as a fleet, refusing one that lacks a field. */
export function decodePrimeFleet(value: unknown): PrimeFleet {
  return Schema.decodeUnknownSync(PrimeFleetSchema)(value);
}

/** The fleet `tests/fixtures/fleets/<name>.json` describes. */
export function primeFleet(name: string): PrimeFleet {
  const file = path.join(PRIME_FLEETS_DIR, `${name}.json`);
  const fleet = decodePrimeFleet(JSON.parse(fs.readFileSync(file, "utf8")));
  if (fleet.name !== name) throw new Error(`${file} names its fleet ${fleet.name}`);
  return fleet;
}

/** One entry of a fixture's list, or a failure naming the list that lacks it. */
export function entry<T>(items: ReadonlyArray<T>, index: number, what: string): T {
  const item = items[index];
  if (item === undefined) throw new Error(`the fixture has no ${what} at position ${index}`);
  return item;
}

/** What wakes the fleet by `kind`, or a failure naming the kind it does not wake on. */
export function wake(fleet: PrimeFleet, kind: string): PrimeWake {
  const found = fleet.wakes_on.find((row) => row.kind === kind);
  if (found === undefined) throw new Error(`${fleet.name} wakes on no ${kind}`);
  return found;
}

/** A field the journey reads from a fixture entry, or a failure naming the field it lacks. */
export function field<T>(value: T | undefined, what: string): T {
  if (value === undefined) throw new Error(`the fixture carries no ${what}`);
  return value;
}

/** A message with this run's words after it, as the README lets a journey add them. */
export function said(message: string, added: string): string {
  return `${message} ${added}`;
}
