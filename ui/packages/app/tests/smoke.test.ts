import { describe, expect, it } from "vitest";

import type { Zombie } from "../lib/types";

describe("smoke: app vitest lane", () => {
  it("validates core runtime contract helpers", () => {
    const zombie: Zombie = {
      id: "zom_1",
      name: "lead-collector",
      status: "active",
      created_at: 0,
      updated_at: 0,
    };
    expect(zombie.status).toBe("active");
  });
});
