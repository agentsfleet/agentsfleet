/**
 * prime-fleets.ts — the prime set, read from the one place it is written.
 *
 * `tests/fixtures/fleets/<name>.json` says what each fixture fleet is: its
 * name, the message a person types to create it, what they type next, and
 * what its journey proves. A journey reads those words here and never spells
 * them itself, so the file stays the only statement of what a fleet is.
 */
import { readFileSync } from "node:fs";
import * as path from "node:path";
import { fileURLToPath } from "node:url";
import { z } from "zod";
import type { ClientHandle } from "./api-client";
import { awaitFirstTurnSettled, createFixtureFleet } from "./fleet-create";

const FIXTURE_DIR = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../../../../../../../tests/fixtures/fleets");

export const PRIME_FLEET = {
  morningRooster: "morning-rooster",
  carefulSquirrel: "careful-squirrel",
  curiousOtter: "curious-otter",
  keenHawk: "keen-hawk",
  nightOwl: "night-owl",
  queenBee: "queen-bee",
  busyBee: "busy-bee",
  stubbornMule: "stubborn-mule",
  watchDog: "watch-dog",
  chattyParrot: "chatty-parrot",
} as const;
export type PrimeFleetName = (typeof PRIME_FLEET)[keyof typeof PRIME_FLEET];

const Text = z.string().min(1);
const Access = z.strictObject({
  kind: z.enum(["secret", "integration"]), name: Text,
  scope: z.strictObject({access: z.enum(["read", "write"])}).optional(),
});
const Wake = z.discriminatedUnion("kind", [
  z.strictObject({kind: z.literal("schedule"), cron: Text}),
  z.strictObject({kind: z.literal("webhook"), source: Text, secret_name: Text}),
  z.strictObject({kind: z.literal("app_event"), source: Text, events: z.array(Text).min(1)}),
  z.strictObject({kind: z.literal("mention"), source: Text}),
]);
const PrimeFleetSchema = z.strictObject({
  name: Text, journey: Text, about: Text, first_message: Text,
  follow_ups: z.array(Text), remembers: z.array(Text), asks_for: z.array(Access),
  wakes_on: z.array(Wake), steers: z.array(Text), proves: z.array(Text).min(1),
  // The one repository a fleet that works on GitHub is given, as `owner/name`.
  repository: Text.optional(),
  image_png: Text.optional(),
});
export type PrimeFleet = z.infer<typeof PrimeFleetSchema>;
type Wake = z.infer<typeof Wake>;

/** A fixture fleet, as its file says it. A file whose `name` is not its own is refused. */
export function primeFleet(name: PrimeFleetName): PrimeFleet {
  const fixture = PrimeFleetSchema.parse(JSON.parse(readFileSync(path.join(FIXTURE_DIR, `${name}.json`), "utf8")));
  if (fixture.name !== name) throw new Error(`${name}.json names the fleet ${fixture.name}`);
  return fixture;
}

/**
 * The fixture's `proves` lines, which a journey uses as its test names. A
 * journey says how many it asserts, so a line added to the file with no test
 * behind it stops the journey before it runs.
 */
export function proofs(fixture: PrimeFleet, asserted: number): (index: number) => string {
  if (fixture.proves.length !== asserted) {
    throw new Error(`${fixture.name} proves ${fixture.proves.length} lines and its journey asserts ${asserted}`);
  }
  return (index) => entry(fixture, fixture.proves, index, "proof");
}

/** One entry of a fixture's list, or a sentence naming the list it is missing from. */
export function entry<T>(fixture: PrimeFleet, list: readonly T[], index: number, what: string): T {
  const found = list[index];
  if (found === undefined) throw new Error(`${fixture.name} has no ${what} at ${index}`);
  return found;
}

/** What the fixture's journey wakes on, by kind. */
export function wake<K extends Wake["kind"]>(fixture: PrimeFleet, kind: K): Extract<Wake, {kind: K}> {
  const found = fixture.wakes_on.find((row): row is Extract<Wake, {kind: K}> => row.kind === kind);
  if (found === undefined) throw new Error(`${fixture.name} wakes on no ${kind}`);
  return found;
}

export type PrimedFleet = {readonly id: string; readonly name: string; readonly turnId: string};

/**
 * Creates the fixture's fleet as a person does, with one message: its own
 * first message unless the journey sends that message with this run's value
 * added. Returns once the first turn has left the queue and its run, so the
 * next message starts a run of its own instead of joining this one.
 */
export async function createPrimeFleet(
  handle: ClientHandle, workspaceId: string, fixture: PrimeFleet, message: string = fixture.first_message,
): Promise<PrimedFleet> {
  const {fleet, turnId} = await createFixtureFleet(handle, workspaceId, {name: fixture.name, message});
  await awaitFirstTurnSettled(handle, workspaceId, fleet.id, turnId);
  return {id: fleet.id, name: fleet.name, turnId};
}
