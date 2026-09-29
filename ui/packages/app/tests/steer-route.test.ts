// @vitest-environment node
// The same-origin steer proxy at
// app/live/v1/workspaces/[workspaceId]/fleets/[fleetId]/messages. It mints the
// bearer, so it is the trust boundary a Server Action's origin check used to
// be: these pin what it refuses before the mint, and what it passes through
// after. Node's own `Request`, because a browser-like one would refuse to let
// a test set `Origin` and `Host`.

import * as fs from "node:fs";
import * as path from "node:path";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { ApiError } from "@/lib/api/errors";
import { STEER_MESSAGE_MAX_BYTES, steerMessagesUrl } from "@/lib/api/fleets-types";
import { ERROR_CODE } from "@/lib/errors";
import nextConfig from "../next.config";

const { getTokenFn, steerFleetMock } = vi.hoisted(() => ({ getTokenFn: vi.fn(), steerFleetMock: vi.fn() }));
vi.mock("@clerk/nextjs/server", () => ({ auth: () => Promise.resolve({ getToken: getTokenFn }) }));
vi.mock("@/lib/api/fleets", () => ({ steerFleet: steerFleetMock }));

import { POST } from "../app/live/v1/workspaces/[workspaceId]/fleets/[fleetId]/messages/route";

const APP_HOST = "app.example.test";
const APP_ORIGIN = `https://${APP_HOST}`;
const WS = "ws_1";
const FLEET = "fleet_1";
const TOKEN = "api_jwt_token";
const OPERATION_ID = "0190a3b4-0000-7000-8000-000000000001";
const STEER = { message: "deploy the canary", operation_id: OPERATION_ID };
const ACCEPTED = { status: "accepted", event_id: "1790573387481-566", replayed: false };
const JSON_TYPE = "application/json";
// pin test: the statuses are the route's answers to the browser.
const HTTP = { ACCEPTED: 202, BAD_REQUEST: 400, UNAUTHORIZED: 401, FORBIDDEN: 403, CONFLICT: 409, TOO_LARGE: 413, NOT_JSON: 415, BAD_GATEWAY: 502 } as const;
const PROBLEM_TYPE = "application/problem+json";
// The daemon's longest operation id, in bytes.
const OPERATION_ID_MAX_BYTES = 200;
// A control character: JSON's widest escape, six bytes on the wire for one.
const WIDEST_ESCAPE = "\u0001";
const ROUTE_DIR = path.join(__dirname, "..", "app", "live", "v1", "workspaces", "[workspaceId]", "fleets", "[fleetId]", "messages");

type Init = { headers?: Record<string, string>; body?: BodyInit | null };

function steerRequest({ headers = {}, body = JSON.stringify(STEER) }: Init = {}): Request {
  return new Request(`http://${APP_HOST}${steerMessagesUrl(WS, FLEET)}`, {
    method: "POST",
    headers: { Origin: APP_ORIGIN, Host: APP_HOST, "Content-Type": JSON_TYPE, ...headers },
    body,
    duplex: "half",
  } as RequestInit);
}

function post(req: Request, workspaceId = WS, fleetId = FLEET): Promise<Response> {
  return POST(req, { params: Promise.resolve({ workspaceId, fleetId }) });
}

async function expectRefused(res: Response, status: number): Promise<void> {
  expect(res.status).toBe(status);
  expect(res.headers.get("cache-control")).toBe("no-store");
  expect(res.headers.get("content-type")).toBe(PROBLEM_TYPE);
  expect(steerFleetMock).not.toHaveBeenCalled();
}

beforeEach(() => {
  getTokenFn.mockResolvedValue(TOKEN);
});

// Reset, not cleared: an answer queued for a request a test refused must not
// reach the next test.
afterEach(() => {
  vi.resetAllMocks();
});

describe("steer route — what it refuses before minting", () => {
  it("test_steer_route_refuses_a_foreign_origin: refuses a POST with no Origin, an opaque one, a foreign one, or one that is not a URL", async () => {
    const origins = [undefined, "null", "https://evil.example", "not a url"];
    for (const origin of origins) {
      const req = steerRequest();
      if (origin === undefined) req.headers.delete("origin");
      else req.headers.set("origin", origin);
      await expectRefused(await post(req), HTTP.FORBIDDEN);
    }
    expect(getTokenFn).not.toHaveBeenCalled();
  });

  it("reads this app's host as Next reads it: the proxy's forwarded host first", async () => {
    steerFleetMock.mockResolvedValueOnce(ACCEPTED);
    const behindProxy = steerRequest({ headers: { Host: "internal:3000", "X-Forwarded-Host": `${APP_HOST}, edge.example` } });
    expect((await post(behindProxy)).status).toBe(HTTP.ACCEPTED);
    steerFleetMock.mockClear();
    const spoofed = steerRequest({ headers: { "X-Forwarded-Host": "evil.example" } });
    await expectRefused(await post(spoofed), HTTP.FORBIDDEN);
  });

  it("reads this app's host off the request URL when no Host header arrives", async () => {
    steerFleetMock.mockResolvedValueOnce(ACCEPTED);
    const hostless = steerRequest();
    hostless.headers.delete("host");
    expect((await post(hostless)).status).toBe(HTTP.ACCEPTED);
  });

  it("refuses dot-only path segments", async () => {
    for (const [ws, fleet] of [["..", FLEET], [WS, "."]] as const) {
      await expectRefused(await post(steerRequest(), ws, fleet), HTTP.BAD_REQUEST);
    }
  });

  it("refuses a body that is not JSON by its type, before reading it", async () => {
    await expectRefused(await post(steerRequest({ headers: { "Content-Type": "text/plain" } })), HTTP.NOT_JSON);
    const untyped = steerRequest();
    untyped.headers.delete("content-type");
    await expectRefused(await post(untyped), HTTP.NOT_JSON);
  });

  it("refuses a body larger than any steer, declared or streamed", async () => {
    const tooLong = "x".repeat(STEER_MESSAGE_MAX_BYTES * 8);
    // Declared: refused on the header, before a byte of the body is read.
    const declared = steerRequest({ headers: { "Content-Length": String(tooLong.length) } });
    await expectRefused(await post(declared), HTTP.TOO_LARGE);
    expect(declared.bodyUsed).toBe(false);
    const streamed = new ReadableStream<Uint8Array>({
      start(controller) {
        controller.enqueue(new TextEncoder().encode(tooLong));
        controller.close();
      },
    });
    await expectRefused(await post(steerRequest({ body: streamed })), HTTP.TOO_LARGE);
  });

  it("refuses a body that is not a steer: not JSON, no body, a missing field, or an extra one", async () => {
    const bodies = ["{not json", null, JSON.stringify({ message: "m" }), JSON.stringify({ ...STEER, sender: "someone" })];
    for (const body of bodies) await expectRefused(await post(steerRequest({ body })), HTTP.BAD_REQUEST);
  });

  it("answers 401 without calling the daemon when nobody is signed in", async () => {
    getTokenFn.mockResolvedValueOnce(null);
    const res = await post(steerRequest());
    await expectRefused(res, HTTP.UNAUTHORIZED);
    expect(await res.json()).toMatchObject({ error_code: ERROR_CODE.AUTH_401 });
  });
});

describe("steer route — what it passes on", () => {
  it("mints the bearer and answers the daemon's 202, uncached", async () => {
    steerFleetMock.mockResolvedValueOnce(ACCEPTED);
    const res = await post(steerRequest({ headers: { "Content-Type": "application/json; charset=utf-8" } }));
    expect(res.status).toBe(HTTP.ACCEPTED);
    expect(res.headers.get("cache-control")).toBe("no-store");
    expect(await res.json()).toEqual(ACCEPTED);
    expect(steerFleetMock).toHaveBeenCalledExactlyOnceWith(WS, FLEET, STEER, TOKEN);
  });

  it("takes the largest steer the daemon can: its longest message, every byte escaped, and its longest id", async () => {
    steerFleetMock.mockResolvedValueOnce(ACCEPTED);
    const widest = { message: WIDEST_ESCAPE.repeat(STEER_MESSAGE_MAX_BYTES), operation_id: WIDEST_ESCAPE.repeat(OPERATION_ID_MAX_BYTES) };
    expect((await post(steerRequest({ body: JSON.stringify(widest) }))).status).toBe(HTTP.ACCEPTED);
    expect(steerFleetMock).toHaveBeenCalledWith(WS, FLEET, widest, TOKEN);
  });

  it("passes the daemon's refusal through with its status, code and request id", async () => {
    steerFleetMock.mockRejectedValueOnce(new ApiError("conflict", HTTP.CONFLICT, ERROR_CODE.AGENTSFLEET_OPERATION_CONFLICT, "req_1"));
    const res = await post(steerRequest());
    expect(res.status).toBe(HTTP.CONFLICT);
    expect(res.headers.get("content-type")).toBe(PROBLEM_TYPE);
    expect(await res.json()).toEqual({ detail: "conflict", error_code: ERROR_CODE.AGENTSFLEET_OPERATION_CONFLICT, request_id: "req_1" });
  });

  it("answers 502 for anything that is not the daemon answering", async () => {
    steerFleetMock.mockRejectedValueOnce(new ApiError("request cancelled before its first attempt", 0, "CANCELLED"));
    expect((await post(steerRequest())).status).toBe(HTTP.BAD_GATEWAY);
    steerFleetMock.mockRejectedValueOnce(new TypeError("fetch failed"));
    const res = await post(steerRequest());
    expect(res.status).toBe(HTTP.BAD_GATEWAY);
    expect(await res.json()).not.toHaveProperty("error_code");
  });

  it("is served by a handler on disk, outside every rewrite's prefix", async () => {
    expect(fs.existsSync(path.join(ROUTE_DIR, "route.ts"))).toBe(true);
    const rewrites = await nextConfig.rewrites?.();
    const rules = Array.isArray(rewrites) ? rewrites : (rewrites?.beforeFiles ?? []);
    const url = steerMessagesUrl(WS, FLEET);
    for (const rule of rules) expect(url.startsWith(String(rule.source).split("/:")[0] + "/")).toBe(false);
    expect(rules.length).toBeGreaterThan(0);
  });
});
