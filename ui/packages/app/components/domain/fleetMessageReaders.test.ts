import { describe, expect, it } from "vitest";
import type { MessageState } from "@assistant-ui/react";

import { readFailureDetail, readGroupMembers } from "./fleetMessageReaders";
import { GROUP_META } from "./useFleetThreadEntries";

// The readers take assistant-ui's MessageState; a minimal custom-bag stand-in
// is enough to exercise the accessors' type guards directly.
function message(custom: Record<string, unknown>): MessageState {
  return { content: [], metadata: { custom } } as unknown as MessageState;
}

describe("readGroupMembers", () => {
  it("reads a non-empty member array, and null otherwise", () => {
    const members = [{ id: "a" }, { id: "b" }];
    expect(readGroupMembers(message({ [GROUP_META.MEMBERS]: members }))).toHaveLength(2);
    expect(readGroupMembers(message({ [GROUP_META.MEMBERS]: [] }))).toBeNull();
    expect(readGroupMembers(message({}))).toBeNull();
  });
});

describe("readFailureDetail", () => {
  it("keeps a non-empty diagnostic and ignores blank or absent values", () => {
    expect(readFailureDetail(message({ failureDetail: "Runner was unavailable" }))).toBe("Runner was unavailable");
    expect(readFailureDetail(message({ failureDetail: "" }))).toBeNull();
    expect(readFailureDetail(message({}))).toBeNull();
  });
});
