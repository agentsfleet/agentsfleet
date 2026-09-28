import { afterEach, beforeEach, describe, expect, expectTypeOf, it, vi } from "vitest";
import { ApiError } from "./errors";
import { steerFleet } from "./fleets";
import type { SteerRequest } from "./fleets-types";

// The steer POST through the real retry policy (`fetch` stubbed, everything
// else live). A dropped socket after the request left is the one failure the
// policy replays for a non-idempotent method (`retry.ts` `#mayRetry`), and the
// replay is safe only because both attempts carry the same operation id.

const fetchMock = vi.fn();
vi.stubGlobal("fetch", fetchMock);

const REQUEST: SteerRequest = { message: "deploy the canary", operation_id: "3f2a9c1e-7b4d-4e0a-9c2f-1a8b7d6e5f40" };
const NOOP_SLEEP = (_ms: number) => Promise.resolve();
const NOOP_RANDOM = () => 0;
const RETRY = { sleepImpl: NOOP_SLEEP, randomFn: NOOP_RANDOM };
const HTTP_ACCEPTED = 202;
const HTTP_UNAVAILABLE = 503;
const SOCKET_RESET = "ECONNRESET";

beforeEach(() => {
  fetchMock.mockReset();
  vi.stubEnv("AGENTSFLEET_NO_RETRY", "");
});
afterEach(() => {
  fetchMock.mockReset();
  vi.unstubAllEnvs();
});

function answered(status: number, body: unknown) {
  return {
    ok: status >= 200 && status < 300,
    status,
    headers: { get: () => null },
    json: async () => body,
  };
}

// What Node's fetch throws when the connection resets with the request on the
// wire: a TypeError whose cause names the socket code.
function socketReset(): TypeError {
  return Object.assign(new TypeError("fetch failed"), { cause: { code: SOCKET_RESET } });
}

function sentBodies(): string[] {
  return fetchMock.mock.calls.map(([, init]) => (init as RequestInit).body as string);
}

describe("steerFleet — replay", () => {
  it("test_socket_drop_replays_same_operation_id", async () => {
    fetchMock.mockRejectedValueOnce(socketReset()).mockResolvedValueOnce(answered(HTTP_ACCEPTED, { event_id: "evt_1" }));
    const result = await steerFleet("ws_1", "zom_1", REQUEST, "tok", RETRY);
    expect(result).toEqual({ event_id: "evt_1" });
    expect(fetchMock).toHaveBeenCalledTimes(2);
    const [first, second] = sentBodies();
    // Byte-equal: the daemon's dedup key is in the body, and a replay that
    // re-serialised differently would still be one operation, but this is the
    // stronger claim and it holds.
    expect(first).toBe(second);
    expect(JSON.parse(String(first))).toEqual(REQUEST);
  });

  it("does not replay a steer the server refused with a 503", async () => {
    fetchMock.mockResolvedValue(answered(HTTP_UNAVAILABLE, { detail: "svc" }));
    await expect(steerFleet("ws_1", "zom_1", REQUEST, "tok", RETRY)).rejects.toBeInstanceOf(ApiError);
    expect(fetchMock).toHaveBeenCalledTimes(1);
  });

  it("test_steer_request_requires_operation_id", () => {
    // The type refuses a steer with no operation id, so no caller can send one
    // the daemon would run twice on a replayed socket drop.
    expectTypeOf<{ message: string }>().not.toMatchTypeOf<SteerRequest>();
    expectTypeOf<SteerRequest>().toHaveProperty("operation_id").toEqualTypeOf<string>();
    expect(JSON.parse(JSON.stringify(REQUEST))).toHaveProperty("operation_id", REQUEST.operation_id);
  });
});
