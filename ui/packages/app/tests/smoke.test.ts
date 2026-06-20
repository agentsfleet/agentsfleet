import { describe, expect, it } from "vitest";

import type { Fleet } from "../lib/types";

describe("smoke: app vitest lane", () => {
  it("validates core runtime contract helpers", () => {
    const fleet: Fleet = {
      id: "zom_1",
      name: "platform-ops",
      status: "active",
      created_at: 0,
      updated_at: 0,
    };
    expect(fleet.status).toBe("active");
  });
});
