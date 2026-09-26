import { describe, expect, it } from "vitest";
import { LOADING_VERBS, loadingVerbFor } from "./loading-verbs";

const SPREAD_KEYS = 50;
const MIN_DISTINCT_VERBS = 5;

describe("loadingVerbFor", () => {
  it("test_loading_verb_for_is_stable", () => {
    expect(loadingVerbFor("1725000000000-7")).toBe(loadingVerbFor("1725000000000-7"));
    expect(LOADING_VERBS).toContain(loadingVerbFor(""));
    const verbs = new Set(Array.from({ length: SPREAD_KEYS }, (_, index) => loadingVerbFor(`1725000000000-${index}`)));
    expect(verbs.size).toBeGreaterThanOrEqual(MIN_DISTINCT_VERBS);
  });
});
