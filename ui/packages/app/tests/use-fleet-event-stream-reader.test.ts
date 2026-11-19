import { afterEach, describe, expect, it, vi } from "vitest";
import { FRAME_KIND } from "@/lib/api/events-types";
import { createEntry } from "@/lib/streaming/fleet-stream-entry";
import { dispatchReplyFrame } from "@/lib/streaming/fleet-stream-reply-registry";

const { getFleetTurnAction } = vi.hoisted(() => ({ getFleetTurnAction: vi.fn() }));

vi.mock("@/app/(dashboard)/w/[workspaceId]/fleets/actions", () => ({ getFleetTurnAction }));

const TURN_ID = "evt_reader";
const PROCESSED = "processed";
const fetchSpy = vi.fn<typeof fetch>();
vi.stubGlobal("fetch", fetchSpy);

// Loading the chat hook is what installs the reader. Without it, a reply that
// lost its final words would never be read back, and nothing else would fail.
await import("../components/domain/conversation/stream/useFleetEventStream");

afterEach(() => {
  vi.unstubAllGlobals();
});

describe("useFleetEventStream detail reader", () => {
  it("installs the same-origin route as the registry's event detail reader, never the Server Action", async () => {
    // A Server Action would queue behind a send in flight; the route does not.
    fetchSpy.mockResolvedValue(new Response(JSON.stringify({ turn_id: TURN_ID, status: PROCESSED, response_text: "Done." })));
    const entry = createEntry("ws_reader", []);
    const apply = vi.fn();
    dispatchReplyFrame(entry, "fleet_reader", { kind: FRAME_KIND.TURN_COMPLETE, turn_id: TURN_ID, status: PROCESSED }, apply, () => true);
    await vi.waitFor(() => expect(apply).toHaveBeenCalledTimes(2));
    expect(fetchSpy).toHaveBeenCalledExactlyOnceWith(
      `/live/v1/workspaces/ws_reader/fleets/fleet_reader/turns/${TURN_ID}`,
      expect.objectContaining({ signal: expect.any(AbortSignal) }),
    );
    expect(getFleetTurnAction).not.toHaveBeenCalled();
  });
});
