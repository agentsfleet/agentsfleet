import { describe, expect, test } from "bun:test";

import {
  AGENTSFLEET_STATUS,
  TERMINAL_STATUSES,
} from "./acceptance/fixtures/constants.ts";
import { fleetReachedActive } from "./acceptance/fixtures/seed.ts";

const FLEET_ID = "019b0000-0000-7000-8000-000000000001";

describe("fleet readiness classification", () => {
  test("active completes readiness", () => {
    expect(fleetReachedActive(FLEET_ID, AGENTSFLEET_STATUS.active)).toBe(true);
  });

  test("transitional status keeps polling", () => {
    expect(fleetReachedActive(FLEET_ID, AGENTSFLEET_STATUS.installing)).toBe(false);
    expect(fleetReachedActive(FLEET_ID, undefined)).toBe(false);
  });

  test("terminal status fails immediately with the observed status", () => {
    for (const status of TERMINAL_STATUSES) {
      expect(() => fleetReachedActive(FLEET_ID, status)).toThrow(
        `fleet ${FLEET_ID} entered terminal status=${status} before becoming active`,
      );
    }
  });
});
