import { describe, it, expect } from "vitest";
import { AGENT_DEFINITION, AGENT_SHORT_GLOSS } from "./copy";

// The user-facing definition is the product's first-touch explanation of the
// core noun. It must stay faithful to docs/architecture/direction.md
// ("a durable runtime, not a one-shot prompt") and name the noun "agent".
describe("agent copy constants", () => {
  it("AGENT_DEFINITION carries the canonical markers", () => {
    expect(AGENT_DEFINITION).toMatch(/^An agent is/);
    expect(AGENT_DEFINITION).toMatch(/durable/i);
    expect(AGENT_DEFINITION).toMatch(/autonomous/i);
    expect(AGENT_DEFINITION).toMatch(/\b(wake|wakes|event)\b/i);
    expect(AGENT_DEFINITION).toMatch(/not a one-shot prompt/i);
  });

  it("names the product 'agent', never the retired noun 'zombie'", () => {
    expect(AGENT_DEFINITION.toLowerCase()).not.toContain("zombie");
    expect(AGENT_SHORT_GLOSS.toLowerCase()).not.toContain("zombie");
  });

  it("AGENT_SHORT_GLOSS is a one-liner naming the agent", () => {
    expect(AGENT_SHORT_GLOSS).toMatch(/^An agent/);
    expect(AGENT_SHORT_GLOSS.length).toBeLessThanOrEqual(120);
  });
});
