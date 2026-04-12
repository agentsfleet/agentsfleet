import { describe, test, expect } from "bun:test";
import { levenshteinDistance, suggestCommand } from "../src/program/suggest.js";

describe("levenshteinDistance", () => {
  test("identical strings return 0", () => {
    expect(levenshteinDistance("login", "login")).toBe(0);
  });

  test("single character difference", () => {
    expect(levenshteinDistance("login", "loign")).toBe(2);
  });

  test("empty string vs non-empty", () => {
    expect(levenshteinDistance("", "abc")).toBe(3);
  });

  test("both empty", () => {
    expect(levenshteinDistance("", "")).toBe(0);
  });

  test("completely different strings", () => {
    expect(levenshteinDistance("abc", "xyz")).toBe(3);
  });
});

describe("suggestCommand", () => {
  test("exact match returns empty for distant input", () => {
    // "doctor" is far enough from all other commands to return only itself-related matches
    const result = suggestCommand("doctor");
    // exact match has distance 0, so it should be excluded
    expect(result).not.toContain("doctor");
  });

  test("close typo 'loign' suggests 'login'", () => {
    const result = suggestCommand("loign");
    expect(result).toContain("login");
  });

  test("close typo 'logut' suggests 'logout'", () => {
    const result = suggestCommand("logut");
    expect(result).toContain("logout");
  });

  test("multi-word: 'workspace ad' suggests 'workspace add'", () => {
    const result = suggestCommand("workspace ad");
    expect(result).toContain("workspace add");
  });

  test("no match for very different input", () => {
    const result = suggestCommand("zzzzzzzzzzzzzzzzz");
    expect(result).toEqual([]);
  });

  test("multiple suggestions sorted by distance", () => {
    const result = suggestCommand("run");
    // "run" is an exact top-level match (distance 0, excluded)
    // but "runs list" and "run status" may be close
    // All results should be sorted by ascending distance
    for (let i = 1; i < result.length; i++) {
      const dPrev = levenshteinDistance("run", result[i - 1]);
      const dCurr = levenshteinDistance("run", result[i]);
      expect(dCurr).toBeGreaterThanOrEqual(dPrev);
    }
  });

  test("'doctr' suggests 'doctor'", () => {
    const result = suggestCommand("doctr");
    expect(result).toContain("doctor");
  });
});
