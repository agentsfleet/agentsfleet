import { describe, expect, it } from "vitest";

import { BANNER_MIN_FAILURES, failureBannerFor } from "@/lib/events/event-banner";
import { GUIDANCE, OUTCOME } from "@/lib/events/event-summary";
import type { FleetEvent } from "@/lib/streaming/fleet-stream-frames";

const CAUSE = "no instructions configured";

let seq = 0;

function evt(over: Partial<FleetEvent> = {}): FleetEvent {
  seq += 1;
  return {
    id: over.id ?? `e${seq}`,
    role: over.role ?? "system",
    actor: over.actor ?? "webhook:github",
    text: over.text ?? "edited #541",
    reply: over.reply ?? "",
    outcome: over.outcome ?? "Failed a startup safety check",
    // `in` rather than `??`: a test that passes an explicit null is asserting
    // the null, and `??` would quietly hand it the default instead.
    failureLabel: "failureLabel" in over ? over.failureLabel ?? null : "startup_posture",
    failureDetail: "failureDetail" in over ? over.failureDetail ?? null : CAUSE,
    createdAt: over.createdAt ?? new Date(Date.UTC(2026, 6, 22, 12, 3, 0)),
    status: over.status ?? "fleet_error",
  };
}

function failures(count: number, over: Partial<FleetEvent> = {}): FleetEvent[] {
  return Array.from({ length: count }, () => evt(over));
}

const processed = (over: Partial<FleetEvent> = {}) =>
  evt({ status: "processed", failureLabel: null, failureDetail: null, outcome: OUTCOME.NO_REPLY, ...over });

describe("failureBannerFor", () => {
  it("names the failure, its cause, how often, and what to do", () => {
    const last = new Date(Date.UTC(2026, 6, 22, 12, 3, 0));
    const banner = failureBannerFor([
      ...failures(14),
      evt({ createdAt: last }),
    ]);
    expect(banner?.count).toBe(15);
    expect(banner?.sentence).toBe("Failed a startup safety check");
    expect(banner?.detail).toBe(CAUSE);
    expect(banner?.guidance).toBe(GUIDANCE.STARTUP);
    expect(banner?.lastSeen).toEqual(last);
  });

  it("stays quiet for a single failure", () => {
    // One failure is an event; only a pattern earns an interruption.
    expect(failureBannerFor(failures(BANNER_MIN_FAILURES - 1))).toBeNull();
    expect(failureBannerFor(failures(BANNER_MIN_FAILURES))).not.toBeNull();
  });

  it("clears the moment the fleet recovers", () => {
    // The newest terminal event is a success, so the fleet is not currently
    // failing — the banner must not outlive the recovery.
    expect(failureBannerFor([...failures(15), processed()])).toBeNull();
  });

  it("counts only the current run, not every failure ever seen", () => {
    const events = [...failures(9), processed(), ...failures(3)];
    expect(failureBannerFor(events)?.count).toBe(3);
  });

  it("does not merge two different failure classes into one count", () => {
    const events = [...failures(5, { failureLabel: "oom_kill" }), ...failures(2)];
    const banner = failureBannerFor(events);
    expect(banner?.label).toBe("startup_posture");
    expect(banner?.count).toBe(2);
  });

  it("is not disturbed by a run still in flight", () => {
    // A `received` row is not evidence either way: it neither extends the run
    // nor counts as a recovery.
    const banner = failureBannerFor([
      ...failures(3),
      evt({ status: "received", failureLabel: null, failureDetail: null }),
    ]);
    expect(banner?.count).toBe(3);
  });

  it("ignores the operator's own rows entirely", () => {
    const banner = failureBannerFor([
      ...failures(2),
      evt({ role: "user", actor: "steer:user_abc", status: "optimistic", failureLabel: null, failureDetail: null }),
    ]);
    expect(banner?.count).toBe(2);
  });

  it("offers no guidance for a class the operator cannot act on", () => {
    const banner = failureBannerFor(failures(3, { failureLabel: "oom_kill" }));
    expect(banner?.guidance).toBeNull();
  });

  it("reports a failure whose runner named no cause without inventing one", () => {
    const banner = failureBannerFor(failures(2, { failureDetail: null }));
    expect(banner?.detail).toBeNull();
    expect(banner?.sentence).toBe("Failed a startup safety check");
  });

  it("has nothing to say about an empty thread or one with no terminal event", () => {
    expect(failureBannerFor([])).toBeNull();
    expect(failureBannerFor([evt({ status: "received", failureLabel: null })])).toBeNull();
  });
});
