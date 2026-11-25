import { describe, expect, it } from "bun:test";
import fs from "node:fs";
import path from "node:path";

import { PRIME_FLEETS_DIR, decodePrimeFleet, entry, field, primeFleet, said, wake } from "./prime-fleets.ts";

const ACCEPTANCE_DIR = path.resolve(import.meta.dir, "..");
const FIXTURE_SUFFIX = ".json";
const JOURNEY_SPEC = /^prime-(.+)-live\.spec\.ts$/u;

// A fleet with every field, for the refusals below to break one field of.
const WHOLE = {
  name: "night-heron",
  journey: "first-message",
  about: "Answers once.",
  first_message: "Reply with one word.",
  follow_ups: ["Say it again."],
  remembers: [],
  asks_for: [{ kind: "secret", name: "heron-key" }],
  wakes_on: [{ kind: "schedule", cron: "0 7 * * *" }],
  steers: [],
  proves: ["It answers."],
};

const fixtureNames = (): ReadonlyArray<string> =>
  fs.readdirSync(PRIME_FLEETS_DIR)
    .filter((file) => file.endsWith(FIXTURE_SUFFIX))
    .map((file) => file.slice(0, -FIXTURE_SUFFIX.length));

describe("the prime set's loader", () => {
  it("reads every fixture file as a fleet named for its file", () => {
    const names = fixtureNames();
    expect(names.length).toBeGreaterThan(0);
    expect(names.map((name) => primeFleet(name).name)).toEqual([...names]);
  });

  it("finds a fixture file for every fleet another fixture steers", () => {
    const names = fixtureNames();
    const steered = names.flatMap((name) => primeFleet(name).steers);
    expect(steered.filter((name) => !names.includes(name))).toEqual([]);
  });

  it("finds a fixture file for every prime journey spec", () => {
    const journeys = fs.readdirSync(ACCEPTANCE_DIR)
      .map((file) => JOURNEY_SPEC.exec(file)?.[1])
      .filter((name): name is string => name !== undefined);
    expect(journeys.length).toBeGreaterThan(0);
    expect(journeys.filter((name) => !fixtureNames().includes(name))).toEqual([]);
  });

  it("keeps a whole fleet's fields as written", () => {
    expect(decodePrimeFleet(WHOLE)).toEqual(WHOLE);
  });

  it("refuses a fleet with no first message", () => {
    const { first_message: _dropped, ...rest } = WHOLE;
    expect(() => decodePrimeFleet(rest)).toThrow(/first_message/u);
  });

  it("refuses a fleet whose first message is empty", () => {
    expect(() => decodePrimeFleet({ ...WHOLE, first_message: "" })).toThrow(/first_message/u);
  });

  it("refuses a follow-up that is not text", () => {
    expect(() => decodePrimeFleet({ ...WHOLE, follow_ups: [7] })).toThrow(/follow_ups/u);
  });

  it("refuses a name no fixture file carries", () => {
    expect(() => primeFleet("no-such-fleet")).toThrow(/no-such-fleet\.json/u);
  });

  it("names the list a journey read past the end of", () => {
    expect(entry(WHOLE.follow_ups, 0, "follow-up")).toBe("Say it again.");
    expect(() => entry(WHOLE.follow_ups, 1, "follow-up")).toThrow("the fixture has no follow-up at position 1");
  });

  it("finds what wakes a fleet by its kind, and names a kind it does not wake on", () => {
    const fleet = decodePrimeFleet(WHOLE);
    expect(wake(fleet, "schedule")).toEqual({ kind: "schedule", cron: "0 7 * * *" });
    expect(() => wake(fleet, "webhook")).toThrow("night-heron wakes on no webhook");
  });

  it("names the field a fixture entry lacks", () => {
    const schedule = wake(decodePrimeFleet(WHOLE), "schedule");
    expect(field(schedule.cron, "schedule cron")).toBe("0 7 * * *");
    expect(() => field(schedule.source, "webhook source")).toThrow("the fixture carries no webhook source");
  });

  it("puts a run's words after the fixture's message", () => {
    expect(said("Say it again.", "The word is heron.")).toBe("Say it again. The word is heron.");
  });
});
