import { describe, expect, it } from "vitest";
import { type EventRow, type LiveFrame } from "@/lib/api/events";
import { FRAME_KIND } from "@/lib/api/events-types";
import { HEADLINE, OUTCOME } from "@/lib/events/event-summary";
import { maxServerCreatedAt, mergeBackfill, rfc3339Seconds } from "./fleet-stream-frames";
import type { FleetEvent } from "./fleet-stream-row";
import { MS_PER_SECOND, evt, row } from "@/tests/helpers/fleet-stream-fixtures";

// The merge and the watermark: how a page of durable rows folds into the
// timeline. The live reducers are `fleet-stream-frames.live.test.ts` and
// `fleet-stream-frames.tools.test.ts`.

describe("mergeBackfill", () => {
  it("dedupes by id and sorts the union oldest-first", () => {
    const prev = [evt({ id: "e2", createdAt: new Date(2000) })];
    const merged = mergeBackfill(prev, [
      row({ event_id: "e1", created_at: MS_PER_SECOND, response_text: "a" }),
      row({ event_id: "e2", created_at: 2000 }),
    ]);
    expect(merged.map((e) => e.id)).toEqual(["e1", "e2"]);
  });

  it("maps a null response_text with request context and a readable failure outcome", () => {
    const [first] = mergeBackfill([], [row({
      response_text: null,
      request_json: "{\"a\":1}",
      failure_label: "startup_posture",
    })]);
    expect(first?.text).toBe("");
    expect(first?.custom?.requestJson).toBe("{\"a\":1}");
    expect(first?.outcome).toBe("Failed a startup safety check");
  });

  // `mergeBackfill` takes `EventRow[]`, and a list row carries no bodies — the
  // read split exists precisely so a page of them stays off oversized-attribute
  // storage. The factory above over-supplies both, so this is the only case
  // that reaches the payload fallback with the shape production delivers.
  it("a list row carrying no bodies reconstructs against an empty payload", () => {
    const { request_json: _request, response_text: _response, ...listRow } = row({ event_id: "e9" });
    const [first] = mergeBackfill([], [listRow as EventRow]);
    expect(first?.id).toBe("e9");
    expect(first?.custom?.requestJson).toBe("{}");
  });

  it("replaces a partial live row with a terminal backfill row of the same id", () => {
    // An event that straddled an outage: live chunks accumulated a partial
    // text, the durable row carries the full final text + terminal status.
    const prev = [evt({ id: "e1", reply: "partial chu", status: "received" })];
    const merged = mergeBackfill(prev, [
      row({ event_id: "e1", status: "processed", response_text: "the full final text" }),
    ]);
    expect(merged).toHaveLength(1);
    expect(merged[0]?.reply).toBe("the full final text");
    expect(merged[0]?.status).toBe("processed");
  });

  it("retains live tool evidence when a terminal row is reconciled", () => {
    const tools = [{ name: "inspect", ms: 12, done: true }];
    const prev = [evt({ id: "e1", status: "received", tools })];
    const merged = mergeBackfill(prev, [
      row({ event_id: "e1", status: "processed", response_text: "done" }),
    ]);
    expect(merged[0]?.tools).toEqual(tools);
  });

  it("recovers the operator's own submitted text from the durable row", () => {
    // The reply field belongs to the fleet. An operator's message lives in the
    // stored request payload, so reading the reply field renders their own
    // message blank the moment the page reloads.
    const [first] = mergeBackfill(
      [],
      [
        row({
          actor: "steer:user_3gkbgxjnujsxbdxttcwcslpc87k",
          event_type: "chat",
          request_json: '{"message":"are you alive"}',
          response_text: null,
        }),
      ],
    );
    expect(first?.role).toBe("user");
    expect(first?.text).toBe("are you alive");
  });

  it("gives an integration event a headline instead of an empty body", () => {
    const [first] = mergeBackfill(
      [],
      [
        row({
          actor: "github-app",
          event_type: "webhook",
          request_json: JSON.stringify({ repo: "owner/repo", number: 12, action: "opened" }),
          response_text: null,
        }),
      ],
    );
    expect(first?.role).toBe("system");
    expect(first?.text).toBe("opened · owner/repo#12");
  });

  it("carries a non-empty outcome for a row with no body at all", () => {
    const [first] = mergeBackfill(
      [],
      [row({ response_text: null, status: "fleet_error", failure_label: "startup_posture" })],
    );
    expect(first?.text).toBe("");
    expect(first?.outcome).toBe("Failed a startup safety check");
  });

  it("keeps the live accumulation when the backfill row is still in progress", () => {
    // The live chunk stream is newer than the list snapshot for a running
    // event — a "received" backfill row must not clobber it.
    const prev = [evt({ id: "e1", reply: "live chunks so far", status: "received" })];
    const merged = mergeBackfill(prev, [
      row({ event_id: "e1", status: "received", response_text: "stale snapshot" }),
    ]);
    expect(merged).toHaveLength(1);
    expect(merged[0]?.reply).toBe("live chunks so far");
  });
});

describe("maxServerCreatedAt", () => {
  it("folds the newest created_at into the watermark and ignores non-numeric values", () => {
    expect(maxServerCreatedAt(null, [])).toBeNull();
    expect(
      maxServerCreatedAt(null, [row({ created_at: 5_000 }), row({ created_at: 3_000 })]),
    ).toBe(5_000);
    expect(maxServerCreatedAt(7_000, [row({ created_at: 5_000 })])).toBe(7_000);
    expect(
      maxServerCreatedAt(null, [row({ created_at: "bogus" as unknown as number })]),
    ).toBeNull();
  });
});

describe("rfc3339Seconds", () => {
  it("truncates to the 20-char second-granular shape and clamps negatives", () => {
    expect(rfc3339Seconds(Date.UTC(2026, 4, 15, 18, 29, 58, 789))).toBe("2026-05-15T18:29:58Z");
    expect(rfc3339Seconds(-5)).toBe("1970-01-01T00:00:00Z");
  });
});
