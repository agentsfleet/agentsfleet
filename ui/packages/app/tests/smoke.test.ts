import { describe, expect, it } from "vitest";

import type { RunStatus } from "../lib/types";
import { formatDuration } from "../lib/utils";

describe("smoke: app vitest lane", () => {
  it("validates core runtime contract helpers", () => {
    const status: RunStatus = "DONE";
    expect(status).toBe("DONE");
    expect(formatDuration(60)).toBe("1m");
  });
});
