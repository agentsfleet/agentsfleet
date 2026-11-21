import { describe, expect, it } from "vitest";

import { fleetCallsign } from "./identity";

describe("fleet callsigns", () => {
  it("retains the callsign for an existing fleet", () => {
    expect(fleetCallsign("0190aaaa-bbbb-7ccc-8ddd-eeeeeeeeeeee")).toBe("Lumen-8453");
  });

  it("is a pure function of the id", () => {
    expect(fleetCallsign("fleet-alpha")).toBe(fleetCallsign("fleet-alpha"));
    expect(fleetCallsign("fleet-alpha")).not.toBe(fleetCallsign("fleet-bravo"));
  });
});
