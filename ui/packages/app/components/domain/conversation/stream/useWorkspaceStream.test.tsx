import { cleanup, renderHook } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";

import { EMPTY_TILE, EMPTY_WORKSPACE } from "@/lib/streaming/workspace-store";
import { useWorkspaceFleetStream, useWorkspaceStream } from "./useWorkspaceStream";

afterEach(cleanup);

describe("workspace stream hooks outside a provider", () => {
  it("read the empty snapshots and unsubscribe without a store", () => {
    const wall = renderHook(() => useWorkspaceStream());
    const tile = renderHook(() => useWorkspaceFleetStream("fleet_unwired"));
    expect(wall.result.current).toBe(EMPTY_WORKSPACE);
    expect(tile.result.current).toBe(EMPTY_TILE);
    // Unmounting runs the no-op unsubscribes a missing store hands back.
    expect(() => {
      wall.unmount();
      tile.unmount();
    }).not.toThrow();
  });
});
