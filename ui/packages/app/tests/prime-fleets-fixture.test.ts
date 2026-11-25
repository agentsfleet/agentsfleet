// @vitest-environment node
/**
 * The prime set's loader. A journey takes its fleet's name, messages and the
 * lines it proves from `tests/fixtures/fleets/`, so the loader refusing a
 * file it cannot stand behind is what keeps a journey from running on words
 * nobody wrote.
 */
import { readdirSync } from "node:fs";
import * as path from "node:path";
import { describe, expect, it } from "vitest";
import { entry, PRIME_FLEET, primeFleet, proofs, wake } from "./e2e/acceptance/fixtures/prime-fleets";

const FIXTURE_DIR = path.resolve(process.cwd(), "../../../tests/fixtures/fleets");
const ACCEPTANCE_DIR = path.resolve(process.cwd(), "tests/e2e/acceptance");
// busy-bee is steered inside queen-bee's journey; every other fleet has a journey file of its own.
const WITHOUT_OWN_JOURNEY: readonly string[] = [PRIME_FLEET.busyBee];

describe("the prime set", () => {
  it("loads every fleet file on disk, each under its own name", () => {
    const onDisk = readdirSync(FIXTURE_DIR).filter((file) => file.endsWith(".json")).map((file) => file.slice(0, -".json".length)).toSorted();
    expect(Object.values(PRIME_FLEET).toSorted()).toEqual(onDisk);
    for (const name of Object.values(PRIME_FLEET)) {
      const fixture = primeFleet(name);
      expect(fixture.name).toBe(name);
      expect(fixture.first_message.length).toBeGreaterThan(0);
      expect(fixture.proves.length).toBeGreaterThan(0);
    }
  });

  it("has one journey spec per fleet, named for it", () => {
    const specs = readdirSync(ACCEPTANCE_DIR).filter((file) => file.startsWith("prime-")).toSorted();
    const expected = Object.values(PRIME_FLEET).filter((name) => !WITHOUT_OWN_JOURNEY.includes(name))
      .map((name) => `prime-${name}.spec.ts`).toSorted();
    expect(specs).toEqual(expected);
  });

  it("reads the words a journey sends from the file", () => {
    const rooster = primeFleet(PRIME_FLEET.morningRooster);
    expect(entry(rooster, rooster.follow_ups, 0, "follow-up")).toBe("Say hello to the team.");
    const owl = primeFleet(PRIME_FLEET.nightOwl);
    expect(wake(owl, "schedule")).toEqual({kind: "schedule", cron: "* * * * *"});
    expect(wake(owl, "webhook")).toEqual({kind: "webhook", source: "owl-post", secret_name: "owl-post-key"});
  });

  it("refuses a journey that asserts a different number of lines than the file proves", () => {
    const rooster = primeFleet(PRIME_FLEET.morningRooster);
    expect(proofs(rooster, 4)(0)).toBe("One message creates an active fleet and its first turn.");
    expect(() => proofs(rooster, 3)).toThrow("morning-rooster proves 4 lines and its journey asserts 3");
  });

  it("names the list and the place a journey asked for and the file lacks", () => {
    const rooster = primeFleet(PRIME_FLEET.morningRooster);
    expect(() => entry(rooster, rooster.follow_ups, 1, "follow-up")).toThrow("morning-rooster has no follow-up at 1");
    expect(() => proofs(rooster, 4)(4)).toThrow("morning-rooster has no proof at 4");
    expect(() => wake(rooster, "webhook")).toThrow("morning-rooster wakes on no webhook");
  });
});
