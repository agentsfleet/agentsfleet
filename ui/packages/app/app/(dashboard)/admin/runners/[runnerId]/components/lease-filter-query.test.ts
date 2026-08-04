import { describe, expect, it } from "vitest";
import {
  formatLeaseFilterQuery,
  parseLeaseFilterQuery,
  shortWorkspaceId,
} from "./lease-filter-query";

describe("parseLeaseFilterQuery", () => {
  it("reads both tokens regardless of order", () => {
    expect(parseLeaseFilterQuery("workspace:ws-1 fleet:reviewer")).toEqual({
      workspace: "ws-1",
      fleet: "reviewer",
    });
    expect(parseLeaseFilterQuery("fleet:reviewer workspace:ws-1")).toEqual({
      workspace: "ws-1",
      fleet: "reviewer",
    });
  });

  it("returns nulls for an empty or whitespace-only query", () => {
    expect(parseLeaseFilterQuery("")).toEqual({ workspace: null, fleet: null });
    expect(parseLeaseFilterQuery("   ")).toEqual({ workspace: null, fleet: null });
  });

  it("keeps a quoted fleet name whole", () => {
    // Without quote handling this would tokenize into `fleet:my` plus two
    // orphans, and the operator would silently filter to the wrong fleet.
    expect(parseLeaseFilterQuery('fleet:"my pr reviewer"').fleet).toBe("my pr reviewer");
  });

  it("drops tokens it does not understand rather than guessing", () => {
    // A bare word and an unknown key must narrow nothing — a typo that quietly
    // became a filter is worse than one that did nothing.
    expect(parseLeaseFilterQuery("author:indy something fleet:reviewer")).toEqual({
      workspace: null,
      fleet: "reviewer",
    });
  });

  it("ignores a key with no value", () => {
    expect(parseLeaseFilterQuery("fleet: workspace:ws-1")).toEqual({
      workspace: "ws-1",
      fleet: null,
    });
  });

  it("ignores a leading colon, which names no key", () => {
    expect(parseLeaseFilterQuery(":reviewer")).toEqual({ workspace: null, fleet: null });
  });

  it("takes the last occurrence when a key repeats", () => {
    expect(parseLeaseFilterQuery("fleet:first fleet:second").fleet).toBe("second");
  });

  it("matches the key case-insensitively", () => {
    expect(parseLeaseFilterQuery("Fleet:reviewer").fleet).toBe("reviewer");
  });

  it("preserves the value's case, which the fleet name may depend on", () => {
    expect(parseLeaseFilterQuery("fleet:PR-Reviewer").fleet).toBe("PR-Reviewer");
  });
});

describe("formatLeaseFilterQuery", () => {
  it("round-trips through parse", () => {
    const filters = { workspace: "ws-1", fleet: "my pr reviewer" };
    expect(parseLeaseFilterQuery(formatLeaseFilterQuery(filters))).toEqual(filters);
  });

  it("quotes only values that would otherwise split", () => {
    expect(formatLeaseFilterQuery({ workspace: null, fleet: "reviewer" })).toBe("fleet:reviewer");
    expect(formatLeaseFilterQuery({ workspace: null, fleet: "two words" })).toBe(
      'fleet:"two words"',
    );
  });

  it("renders an empty string when nothing is filtered", () => {
    expect(formatLeaseFilterQuery({ workspace: null, fleet: null })).toBe("");
  });
});

describe("shortWorkspaceId", () => {
  it("truncates a long id and leaves a short one alone", () => {
    expect(shortWorkspaceId("ws-0123456789")).toBe("ws-01234…");
    expect(shortWorkspaceId("ws-1")).toBe("ws-1");
  });
});
