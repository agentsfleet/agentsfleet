import { describe, expect, it, vi } from "vitest";
import { FRAME_KIND } from "@/lib/api/events-types";
import { createEntry } from "@/lib/streaming/fleet-stream-entry";
import { dispatchReplyFrame } from "@/lib/streaming/fleet-stream-reply-registry";

const { getFleetEventAction } = vi.hoisted(() => ({
  getFleetEventAction: vi.fn().mockResolvedValue({ ok: true, data: {} }),
}));

vi.mock("@/app/(dashboard)/w/[workspaceId]/fleets/actions", () => ({ getFleetEventAction }));

// Loading the chat hook is what installs the reader. Without it, a reply that
// lost its final words would never be read back, and nothing else would fail.
await import("../components/domain/useFleetEventStream");

describe("useFleetEventStream detail reader", () => {
  it("installs the fleet Server Action as the registry's event detail reader", async () => {
    const entry = createEntry("ws_reader", []);
    dispatchReplyFrame(entry, "fleet_reader", { kind: FRAME_KIND.EVENT_COMPLETE, event_id: "evt_reader", status: "processed" }, vi.fn(), () => true);
    await vi.waitFor(() => expect(getFleetEventAction).toHaveBeenCalledWith("ws_reader", "fleet_reader", "evt_reader"));
  });
});
