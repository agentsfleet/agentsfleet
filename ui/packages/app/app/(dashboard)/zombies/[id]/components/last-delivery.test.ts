import { describe, expect, it, vi, beforeEach } from "vitest";

const eventsApi = vi.hoisted(() => ({ listZombieEvents: vi.fn() }));
vi.mock("@/lib/api/events", () => eventsApi);

import { actorGlobFor, resolveLastDeliveries } from "./last-delivery";
import type { ZombieTrigger } from "@/lib/types";

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

  it("opts api triggers out — no stable actor namespace", () => {
    expect(actorGlobFor({ type: "api" })).toBeNull();
  });
});

describe("resolveLastDeliveries", () => {
  beforeEach(() => {
    eventsApi.listZombieEvents.mockReset();
  });

  it("returns an empty map when no triggers are declared", async () => {
    const result = await resolveLastDeliveries("ws_1", "zmb_1", "tok", []);
    expect(result).toEqual({});
    expect(eventsApi.listZombieEvents).not.toHaveBeenCalled();
  });

  it("fans out one events call per webhook + cron trigger and returns created_at", async () => {
    eventsApi.listZombieEvents.mockImplementation(async (_w, _z, _t, opts) => {
      if (opts?.actor === "webhook:github:*") {
        return { items: [{ created_at: 1_700_000_000_000 }], next_cursor: null };
      }
      if (opts?.actor === "cron:*") {
        return { items: [{ created_at: 1_700_000_001_000 }], next_cursor: null };
      }
      return { items: [], next_cursor: null };
    });

    const triggers: ZombieTrigger[] = [
      { type: "webhook", source: "github", events: ["workflow_run"] },
      { type: "cron", schedule: "*/15 * * * *" },
    ];
    const result = await resolveLastDeliveries("ws_1", "zmb_1", "tok", triggers);
    expect(result).toEqual({
      "webhook:github": 1_700_000_000_000,
      "cron:*/15 * * * *": 1_700_000_001_000,
    });
  });

  it("records null when the events page has no rows", async () => {
    eventsApi.listZombieEvents.mockResolvedValue({ items: [], next_cursor: null });
    const result = await resolveLastDeliveries("ws_1", "zmb_1", "tok", [
      { type: "webhook", source: "linear" },
    ]);
    expect(result).toEqual({ "webhook:linear": null });
  });

  it("degrades a thrown events fetch into a null entry", async () => {
    eventsApi.listZombieEvents.mockRejectedValue(new Error("network down"));
    const result = await resolveLastDeliveries("ws_1", "zmb_1", "tok", [
      { type: "webhook", source: "jira" },
    ]);
    expect(result).toEqual({ "webhook:jira": null });
  });

  it("skips the events fetch for api triggers and leaves the key absent", async () => {
    // The absent key is load-bearing: TriggerPanel reads `undefined` as
    // "parent did not look", suppressing the "never" delivery badge and
    // the auto-expand-on-mount path. Writing `null` would falsely fire
    // both on every api trigger.
    const result = await resolveLastDeliveries("ws_1", "zmb_1", "tok", [
      { type: "api" },
    ]);
    expect(result).toEqual({});
    expect("api" in result).toBe(false);
    expect(eventsApi.listZombieEvents).not.toHaveBeenCalled();
  });

  it("records api keys as absent even when mixed with delivering triggers", async () => {
    eventsApi.listZombieEvents.mockResolvedValueOnce({
      items: [{ id: "evt_1", created_at: 1_700_000_000_000 }],
      next_cursor: null,
    });
    const result = await resolveLastDeliveries("ws_1", "zmb_1", "tok", [
      { type: "webhook", source: "github" },
      { type: "api" },
    ]);
    expect(result).toEqual({ "webhook:github": 1_700_000_000_000 });
    expect("api" in result).toBe(false);
  });
});
