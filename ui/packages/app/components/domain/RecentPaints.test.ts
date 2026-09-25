import { describe, expect, it } from "vitest";
import { RecentPaints } from "./RecentPaints";

describe("RecentPaints", () => {
  it("deduplicates recent keys and evicts the oldest at capacity", () => {
    const paints = new RecentPaints(2);
    expect(paints.has("fleet-a:evt:1")).toBe(false);
    paints.add("fleet-a:evt:1");
    paints.add("fleet-a:evt:1");
    expect(paints.has("fleet-a:evt:1")).toBe(true);
    paints.add("fleet-b:evt:2");
    paints.add("fleet-c:evt:3");
    expect(paints.has("fleet-a:evt:1")).toBe(false);
    expect(paints.has("fleet-b:evt:2")).toBe(true);
    expect(paints.has("fleet-c:evt:3")).toBe(true);
  });
});
