import { describe, it, expect } from "vitest";
import { FLEET_DEFINITION, FLEET_SHORT_GLOSS } from "./copy";

// The user-facing definition is the product's first-touch explanation of the
// core noun. It must stay faithful to docs/architecture/direction.md
// ("a durable runtime, not a one-shot prompt") and name the noun "Fleet".
describe("Fleet copy constants", () => {
  const retiredNoun = ["zom", "bie"].join("");

  it("FLEET_DEFINITION carries the canonical markers", () => {
    expect(FLEET_DEFINITION).toMatch(/^A Fleet is/);
    expect(FLEET_DEFINITION).toMatch(/durable/i);
    expect(FLEET_DEFINITION).toMatch(/autonomous/i);
    expect(FLEET_DEFINITION).toMatch(/\b(wake|wakes|event)\b/i);
    expect(FLEET_DEFINITION).toMatch(/not a one-shot prompt/i);
  });

  it("names the product 'Fleet', never the retired noun", () => {
    expect(FLEET_DEFINITION.toLowerCase()).not.toContain(retiredNoun);
    expect(FLEET_SHORT_GLOSS.toLowerCase()).not.toContain(retiredNoun);
  });

  it("FLEET_SHORT_GLOSS is a one-liner naming the Fleet", () => {
    expect(FLEET_SHORT_GLOSS).toMatch(/^A Fleet/);
    expect(FLEET_SHORT_GLOSS.length).toBeLessThanOrEqual(120);
  });
});
