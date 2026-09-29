import React, { Profiler } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render } from "@testing-library/react";
import { TooltipProvider } from "@agentsfleet/design-system";

import type { Fleet } from "@/lib/api/fleets";

import {
  chunk,
  completed,
  flushFrame,
  push,
  received,
  setupWorkspaceWire,
  toolStarted,
  WORKSPACE_ID,
} from "@/tests/helpers/workspace-store-harness";
import { WorkspaceStreamProvider } from "@/components/domain/useWorkspaceStream";
import FleetTile from "./FleetTile";

vi.mock("next/link", () => ({
  default: ({ href, children, ...rest }: React.PropsWithChildren<{ href: string }>) =>
    React.createElement("a", { href, ...rest }, children),
}));

const TILES = 100;
const CHUNK_FRAMES = 1_000;
const TOOL = "search_repo";
const COUNTERS = { events_processed: 9, budget_used_nanos: 2_000_000_000 };

setupWorkspaceWire();
afterEach(cleanup);

function fleetAt(index: number): Fleet {
  return {
    id: `fleet_${index}`,
    name: `fleet ${index}`,
    status: "active",
    created_at: 0,
    updated_at: 0,
    budget_used_nanos: 0,
    events_processed: 0,
  };
}

const FLEETS = Array.from({ length: TILES }, (_, index) => fleetAt(index));
const FLEET_IDS = FLEETS.map((fleet) => fleet.id);

function Wall({ onRender }: { onRender: () => void }) {
  return (
    <WorkspaceStreamProvider workspaceId={WORKSPACE_ID} fleetIds={FLEET_IDS}>
      <TooltipProvider>
        <Profiler id="wall" onRender={onRender}>
          {FLEETS.map((fleet) => (
            <FleetTile key={fleet.id} fleet={fleet} workspaceId={WORKSPACE_ID} />
          ))}
        </Profiler>
      </TooltipProvider>
    </WorkspaceStreamProvider>
  );
}

function deliver(frames: () => void) {
  act(() => {
    frames();
    flushFrame();
  });
}

describe("the wall under a streaming reply", () => {
  it("test_wall_tile_ignores_chunks: 1,000 chunk and tool frames across 100 open rows render no tile", () => {
    const onRender = vi.fn();
    render(<Wall onRender={onRender} />);
    deliver(() => FLEET_IDS.forEach((fleetId) => push(fleetId, received(fleetId, `${fleetId}_e1`))));
    expect(onRender).toHaveBeenCalled();
    onRender.mockClear();

    deliver(() => {
      for (let frame = 0; frame < CHUNK_FRAMES; frame += 1) {
        const fleetId = FLEET_IDS[frame % TILES] ?? "";
        push(fleetId, chunk(fleetId, `${fleetId}_e1`, `word ${frame} `));
        push(fleetId, toolStarted(fleetId, `${fleetId}_e1`, TOOL));
      }
    });
    expect(onRender).not.toHaveBeenCalled();

    // The same wall still repaints for what a tile does show.
    const moved = FLEET_IDS[0] ?? "";
    deliver(() => push(moved, completed(moved, `${moved}_e1`, COUNTERS)));
    expect(onRender).toHaveBeenCalled();
  });
});
