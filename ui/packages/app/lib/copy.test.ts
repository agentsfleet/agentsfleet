import { describe, it, expect } from "vitest";
import { AGENT_DEFINITION, AGENT_SHORT_GLOSS } from "./copy";

// Mirror of the website copy guard — the app renders the same first-touch
// definition (empty state + first-run card) from this constant. Faithful to
// docs/architecture/direction.md and named on the noun "agent".
describe("agent copy constants", () => {
  it("AGENT_DEFINITION carries the canonical markers", () => {
    expect(AGENT_DEFINITION).toMatch(/^An agent is/);
    expect(AGENT_DEFINITION).toMatch(/durable/i);
    expect(AGENT_DEFINITION).toMatch(/autonomous/i);
    expect(AGENT_DEFINITION).toMatch(/\b(wake|wakes|event)\b/i);
    expect(AGENT_DEFINITION).toMatch(/not a one-shot prompt/i);
  });

  it("names the product 'agent', never the retired noun 'agent'", () => {
    expect(AGENT_DEFINITION.toLowerCase()).not.toContain("agent");
    expect(AGENT_SHORT_GLOSS.toLowerCase()).not.toContain("agent");
  });
});
