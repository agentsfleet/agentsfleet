import { describe, expect, it, vi, beforeEach } from "vitest";

const eventsApi = vi.hoisted(() => ({ listFleetEvents: vi.fn() }));
vi.mock("@/lib/api/events", () => eventsApi);

import { actorGlobFor, resolveLastDeliveries } from "./last-delivery";
import type { FleetTrigger } from "@/lib/types";

describe("actorGlobFor", () => {
  it("namespaces webhook actors by source", () => {
    expect(actorGlobFor({ type: "webhook", source: "github" })).toBe(
      "webhook:github:*",
    );
  });

  it("returns the unsourced cron glob", () => {
    expect(actorGlobFor({ type: "cron", schedule: "*/15 * * * *" })).toBe(
      "cron:*",
    );
  });

  it("namespaces chat mentions by their provider, as the admission names its actor", () => {
    expect(
      actorGlobFor({ type: "mention", source: "slack", channels: ["C0123456789"] }),
    ).toBe("slack:*");
  });


});

describe("resolveLastDeliveries", () => {
  beforeEach(() => {
    eventsApi.listFleetEvents.mockReset();
  });

  it("returns an empty map when no triggers are declared", async () => {
    const result = await resolveLastDeliveries("ws_1", "agt_a1", "tok", []);
    expect(result).toEqual({});
    expect(eventsApi.listFleetEvents).not.toHaveBeenCalled();
  });

  it("fans out one events call per webhook + cron trigger and returns created_at", async () => {
    eventsApi.listFleetEvents.mockImplementation(async (_w, _z, _t, opts) => {
      if (opts?.actor === "webhook:github:*") {
        return { items: [{ created_at: "2023-11-14T22:13:20Z" }], next_cursor: null };
      }
      if (opts?.actor === "cron:*") {
        return { items: [{ created_at: "2023-11-14T22:13:21Z" }], next_cursor: null };
      }
      return { items: [], next_cursor: null };
    });

    const triggers: FleetTrigger[] = [
      { type: "webhook", source: "github", events: ["workflow_run"] },
      { type: "cron", schedule: "*/15 * * * *" },
    ];
    const result = await resolveLastDeliveries("ws_1", "agt_a1", "tok", triggers);
    expect(result).toEqual({
      "webhook:github": "2023-11-14T22:13:20Z",
      "cron:*/15 * * * *": "2023-11-14T22:13:21Z",
    });
  });

  it("records null when the events page has no rows", async () => {
    eventsApi.listFleetEvents.mockResolvedValue({ items: [], next_cursor: null });
    const result = await resolveLastDeliveries("ws_1", "agt_a1", "tok", [
      { type: "webhook", source: "linear" },
    ]);
    expect(result).toEqual({ "webhook:linear": null });
  });

  it("degrades a thrown events fetch into a null entry", async () => {
    eventsApi.listFleetEvents.mockRejectedValue(new Error("network down"));
    const result = await resolveLastDeliveries("ws_1", "agt_a1", "tok", [
      { type: "webhook", source: "slack" },
    ]);
    expect(result).toEqual({ "webhook:slack": null });
  });

});
