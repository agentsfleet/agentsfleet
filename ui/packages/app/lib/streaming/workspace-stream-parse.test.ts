import { describe, expect, it } from "vitest";

import { FRAME_KIND } from "@/lib/api/events-types";
import { parseWorkspaceFrame } from "./workspace-stream-parse";

const FLEET_A = "z_a";

function frame(fleetId: string): string {
  return JSON.stringify({ fleet_id: fleetId, kind: "event_received", event_id: "e1", actor: "a" });
}

describe("parseWorkspaceFrame", () => {
  it("accepts a tagged object frame and rejects everything else", () => {
    expect(parseWorkspaceFrame(frame(FLEET_A))).toMatchObject({ fleet_id: FLEET_A });
    expect(parseWorkspaceFrame("{bad json")).toBeNull();
    expect(parseWorkspaceFrame("null")).toBeNull();
    expect(parseWorkspaceFrame(JSON.stringify({ kind: "x" }))).toBeNull();
    expect(parseWorkspaceFrame(JSON.stringify({ fleet_id: "z" }))).toBeNull();
  });

  it("accepts workspace control frames without fleet_id", () => {
    expect(parseWorkspaceFrame(JSON.stringify({ kind: FRAME_KIND.HELLO, fleet_ids: [FLEET_A] }))?.kind).toBe(
      FRAME_KIND.HELLO,
    );
    expect(parseWorkspaceFrame(JSON.stringify({ kind: FRAME_KIND.CATCHING_UP, dropped: 1 }))?.kind).toBe(
      FRAME_KIND.CATCHING_UP,
    );
    expect(parseWorkspaceFrame(JSON.stringify({ kind: FRAME_KIND.HELLO, fleet_ids: [1] }))).toBeNull();
    expect(parseWorkspaceFrame(JSON.stringify({ kind: FRAME_KIND.HELLO, fleet_ids: [""] }))).toBeNull();
    expect(parseWorkspaceFrame(JSON.stringify({ kind: FRAME_KIND.CATCHING_UP, dropped: "1" }))).toBeNull();
    expect(parseWorkspaceFrame(JSON.stringify({ kind: FRAME_KIND.CATCHING_UP, dropped: -1 }))).toBeNull();
    expect(parseWorkspaceFrame(JSON.stringify({ kind: FRAME_KIND.CATCHING_UP, dropped: 1.5 }))).toBeNull();
  });
});
