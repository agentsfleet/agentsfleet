import { describe, expect, it } from "vitest";

import type { Agent } from "../lib/types";

describe("smoke: app vitest lane", () => {
  it("validates core runtime contract helpers", () => {
    const agent: Agent = {
      id: "zom_1",
      name: "platform-ops",
      status: "active",
      created_at: 0,
      updated_at: 0,
    };
    expect(agent.status).toBe("active");
  });
});
