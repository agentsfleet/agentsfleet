import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { stopTurn, TURN_STOP_KIND } from "./fleet-turn-stop";

const WORKSPACE = "ws/1";
const FLEET = "fleet 1";
const TURN = "0199a801-1000-7000-8000-0000000000e1";
// pin test: literal is the contract — the route the browser PATCHes, ids encoded.
const ROUTE = `/live/v1/workspaces/ws%2F1/fleets/fleet%201/turns/${TURN}`;
const CANCELLED = { fleet_id: FLEET, turn_id: TURN, workspace_id: WORKSPACE, actor: "steer:user_1", event_type: "chat",
  status: "cancelled", request_json: "{}", response_text: null, tool_calls: null, tokens: null, wall_ms: null,
  failure_label: null, failure_detail: null, created_at: "2026-11-20T10:00:00.000Z", updated_at: "2026-11-20T10:00:01.000Z" };

const fetchMock = vi.fn();

function answer(status: number, body: unknown, contentType = "application/json"): Response {
  return new Response(JSON.stringify(body), { status, headers: { "Content-Type": contentType } });
}

function stop() {
  return stopTurn(WORKSPACE, FLEET, TURN, new AbortController().signal);
}

beforeEach(() => vi.stubGlobal("fetch", fetchMock));
afterEach(() => {
  fetchMock.mockReset();
  vi.unstubAllGlobals();
});

describe("stopTurn", () => {
  it("PATCHes the turn's status to cancelled on the same-origin route and reads the saved row", async () => {
    fetchMock.mockResolvedValueOnce(answer(200, CANCELLED));
    expect(await stop()).toEqual({ kind: TURN_STOP_KIND.STOPPED, turn: CANCELLED });
    const [url, init] = fetchMock.mock.calls[0] as [string, RequestInit];
    expect(url).toBe(ROUTE);
    expect(init.method).toBe("PATCH");
    expect(JSON.parse(init.body as string)).toEqual({ status: "cancelled" });
    expect(init.headers).toEqual({ "Content-Type": "application/json" });
    expect(init.redirect).toBe("manual");
    expect(init.cache).toBe("no-store");
  });

  it("reads a 409 as a turn that finished first, so its answer stands", async () => {
    fetchMock.mockResolvedValueOnce(answer(409, { error_code: "UZ-AGT-021" }, "application/problem+json"));
    expect(await stop()).toEqual({ kind: TURN_STOP_KIND.FINISHED });
  });

  it("carries any other refusal's status", async () => {
    fetchMock.mockResolvedValueOnce(answer(404, { error_code: "UZ-AGT-015" }, "application/problem+json"));
    expect(await stop()).toEqual({ kind: TURN_STOP_KIND.FAILED, status: 404 });
  });

  it("fails without a status when nothing answered, or the answer is not a turn", async () => {
    fetchMock.mockRejectedValueOnce(new TypeError("network"));
    expect(await stop()).toEqual({ kind: TURN_STOP_KIND.FAILED });
    fetchMock.mockResolvedValueOnce(answer(200, { ok: true }));
    expect(await stop()).toEqual({ kind: TURN_STOP_KIND.FAILED });
    fetchMock.mockResolvedValueOnce(new Response("<html>", { status: 200 }));
    expect(await stop()).toEqual({ kind: TURN_STOP_KIND.FAILED });
  });

  it("reads a sign-in redirect as a failure, never as the turn", async () => {
    fetchMock.mockResolvedValueOnce({ type: "opaqueredirect", status: 0, ok: false } as Response);
    expect(await stop()).toEqual({ kind: TURN_STOP_KIND.FAILED });
  });
});
