import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const io = vi.hoisted(() => ({get: vi.fn()}));
vi.mock("./e2e/acceptance/fixtures/api-client", () => ({clientFor: () => ({get: io.get})}));
import { probeRunnerHeartbeatAdvances } from "./e2e/acceptance/fixtures/preflight";

const BEAT = {
  whole: "2026-10-21T00:00:00Z",
  tenth: "2026-10-21T00:00:00.1Z",
  tenthPadded: "2026-10-21T00:00:00.100Z",
  later: "2026-10-21T00:00:00.11Z",
};
const INVALID_BEAT = "not-a-date";
const STALLED_DIAGNOSIS = "the selected runner heartbeat did not advance";
const observation = (lastSeen: string) => ({items: [{id: "candidate-runner", liveness: "online", last_seen_at: lastSeen}]});

beforeEach(() => {vi.resetAllMocks(); vi.useFakeTimers();});
afterEach(() => vi.useRealTimers());

describe("runner heartbeat preflight", () => {
  it.each([
    [BEAT.whole, BEAT.tenth],
    [BEAT.tenth, BEAT.later],
  ])("should accept chronological advancement from %s to %s", async (first, second) => {
    io.get.mockResolvedValueOnce(observation(first)).mockResolvedValueOnce(observation(second));
    const check = expect(probeRunnerHeartbeatAdvances()).resolves.toBeUndefined();
    await Promise.all([check, vi.runAllTimersAsync()]);
    expect(io.get).toHaveBeenCalledTimes(2);
  });

  it.each([
    [BEAT.tenth, BEAT.tenth],
    [BEAT.tenthPadded, BEAT.tenth],
    [BEAT.later, BEAT.tenth],
    [BEAT.tenth, INVALID_BEAT],
    [INVALID_BEAT, BEAT.later],
  ])("should refuse a stalled, backward or invalid heartbeat from %s to %s", async (first, second) => {
    io.get.mockResolvedValueOnce(observation(first)).mockResolvedValueOnce(observation(second));
    const check = expect(probeRunnerHeartbeatAdvances()).rejects.toMatchObject({
      name: "ReleasePreflightError", message: expect.stringContaining(STALLED_DIAGNOSIS),
    });
    await Promise.all([check, vi.runAllTimersAsync()]);
    expect(io.get).toHaveBeenCalledTimes(2);
  });
});
