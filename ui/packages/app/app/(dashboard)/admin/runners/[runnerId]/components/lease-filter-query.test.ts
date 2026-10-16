import { describe, expect, it } from "vitest";
import { shortWorkspaceId } from "./lease-filter-query";

describe("shortWorkspaceId", () => {
  it("truncates a long id and leaves a short one alone", () => {
    expect(shortWorkspaceId("ws-0123456789")).toBe("ws-01234…");
    expect(shortWorkspaceId("ws-1")).toBe("ws-1");
  });
});
