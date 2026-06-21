import { describe, it, expect } from "vitest";
import { FLEET_DEFINITION, FLEET_SHORT_GLOSS } from "./copy";

// Mirror of the website copy guard — the app renders the same first-touch
// definition (empty state + first-run card) from this constant. Faithful to
// docs/architecture/direction.md and named on the noun "Fleet".
describe("Fleet copy constants", () => {
  it("FLEET_DEFINITION carries the canonical markers", () => {
    expect(FLEET_DEFINITION).toMatch(/^A Fleet is/);
    expect(FLEET_DEFINITION).toMatch(/durable/i);
    expect(FLEET_DEFINITION).toMatch(/autonomous/i);
    expect(FLEET_DEFINITION).toMatch(/\b(wake|wakes|event)\b/i);
    expect(FLEET_DEFINITION).toMatch(/not a one-shot prompt/i);
  });

  it("names the product 'Fleet', never the retired noun 'zombie'", () => {
    expect(FLEET_DEFINITION.toLowerCase()).not.toContain("zombie");
    expect(FLEET_SHORT_GLOSS.toLowerCase()).not.toContain("zombie");
  });
});
