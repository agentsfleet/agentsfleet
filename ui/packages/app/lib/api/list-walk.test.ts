import { describe, expect, it, vi } from "vitest";
import { MAX_LIST_WALK_REQUESTS, walkList } from "./list-walk";

describe("walkList", () => {
  it("accumulates items across pages and stops on a null cursor", async () => {
    const fetchPage = vi
      .fn()
      .mockResolvedValueOnce({ items: ["a", "b"], total: 3, next_cursor: "c1" })
      .mockResolvedValueOnce({ items: ["c"], total: 3, next_cursor: null });
    const res = await walkList<string>("test list", fetchPage);
    expect(res.items).toEqual(["a", "b", "c"]);
    expect(res.total).toBe(3);
    expect(fetchPage).toHaveBeenNthCalledWith(1, null);
    expect(fetchPage).toHaveBeenNthCalledWith(2, "c1");
  });

  it("keeps the last non-null total when later pages report null", async () => {
    const fetchPage = vi
      .fn()
      .mockResolvedValueOnce({ items: ["a"], total: 2, next_cursor: "c1" })
      .mockResolvedValueOnce({ items: ["b"], total: null, next_cursor: null });
    const res = await walkList<string>("test list", fetchPage);
    expect(res.total).toBe(2);
  });

  it("refuses a runaway cursor after the request bound", async () => {
    const fetchPage = vi.fn().mockResolvedValue({ items: ["x"], total: null, next_cursor: "same" });
    await expect(walkList<string>("test list", fetchPage)).rejects.toThrow(/did not end/);
    expect(fetchPage).toHaveBeenCalledTimes(MAX_LIST_WALK_REQUESTS);
  });
});
