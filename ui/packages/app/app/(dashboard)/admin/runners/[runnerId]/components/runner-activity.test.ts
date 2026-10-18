import { describe, expect, it } from "vitest";
import { RUNNER_EVENT_TYPE, RUNNER_LIFECYCLE_EVENT_TYPES, type RunnerLifecycleEventType } from "@/lib/api/runners-types";
import { runnerActivity } from "./runner-activity";

function event(event_type: RunnerLifecycleEventType, metadata: unknown = {}) {
  return {id: "event-1", runner_id: "runner-1", occurred_at: "2026-10-17T06:32:49Z", event_type, metadata};
}
describe("recorded runner activity wording", () => {
  it.each(RUNNER_LIFECYCLE_EVENT_TYPES)("provides a readable activity and detail for %s", (type) => {
    const copy = runnerActivity(event(type));
    expect(copy.activity.length).toBeGreaterThan(0);
    expect(copy.details.length).toBeGreaterThan(0);
    expect(copy.activity).not.toContain("runner_");
  });
  it.each([null, [], "unexpected", {worker_count: -1}, {host_id: 3}, {last_seen_at: 0}])("keeps malformed metadata from inventing detail: %j", (metadata) => {
    expect(runnerActivity(event(RUNNER_EVENT_TYPE.policyAssigned, metadata))).toEqual({
      activity: "Runner policy changed", details: "Runner policy was updated.",
    });
  });
  it.each(["1970-01-01T00:00:00Z", "1970-01-01T00:00:00.000Z"])("names first contact from a saved instant: %s", (last_seen_at) => {
    expect(runnerActivity(event(RUNNER_EVENT_TYPE.online, {last_seen_at})).details).toBe("First connection established.");
  });
  it("uses descriptive liveness details without repeating dates", () => {
    expect(runnerActivity(event(RUNNER_EVENT_TYPE.online, {last_seen_at: "2026-10-16T03:33:00Z"})).details).toBe("Connection established.");
    expect(runnerActivity(event(RUNNER_EVENT_TYPE.offline, {last_seen_at: "2026-10-16T03:33:00Z"})).details).toBe("No recent heartbeat received.");
  });
  it("keeps future recorded policy values readable", () => {
    expect(runnerActivity(event(RUNNER_EVENT_TYPE.policyAssigned, {sandbox_tier: "new-isolation", extra: true})).details).toBe("Isolation: new-isolation");
    expect(runnerActivity(event(RUNNER_EVENT_TYPE.cordoned, {from_status: "maintenance", to_status: "cordoned"})).details).toBe("maintenance → New work paused");
  });
  it("does not show an inherited object property as a policy label", () => {
    expect(runnerActivity(event(RUNNER_EVENT_TYPE.policyAssigned, {sandbox_tier: "toString"})).details).toBe("Isolation: toString");
  });
  it("keeps incomplete state transitions honest", () => {
    expect(runnerActivity(event(RUNNER_EVENT_TYPE.drained, {from_status: "draining"})).details).toBe("All active work finished.");
    expect(runnerActivity(event(RUNNER_EVENT_TYPE.registered, {host_id: "host-x"})).details).toBe("Host: host-x");
  });
});
