import { setImmediate } from "node:timers/promises";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import globalTeardown from "./e2e/acceptance/global-teardown";
import { FIXTURE_KEYS } from "./e2e/acceptance/fixtures/constants";

const state = vi.hoisted(() => ({ revoked: false, fleets: 1, apiUnavailable: false }));
const SESSION_ID = "sess_teardownOrder";
const FRESH_JWT = "refreshed-fixture-jwt";

vi.mock("node:fs", async (importOriginal) => {
  const fs = await importOriginal<typeof import("node:fs")>();
  const isCache = (path: unknown) => String(path).endsWith(".fixture-jwts.json");
  return {
    ...fs,
    existsSync: (path: string) => isCache(path) || fs.existsSync(path),
    readFileSync: (path: string) => isCache(path)
      ? JSON.stringify(Object.fromEntries(FIXTURE_KEYS.map((key) => [key, {
        sessionId: `${SESSION_ID}${key}`, sessionJwt: "expired-fixture-jwt",
      }])))
      : fs.readFileSync(path),
  };
});

function serviceResponse(input: string | URL | Request, init?: RequestInit): Response {
  const url = new URL(input instanceof Request ? input.url : String(input));
  if (url.pathname.endsWith("/revoke")) {
    state.revoked = true;
    return Response.json({ status: "revoked" });
  }
  if (url.pathname.endsWith("/tokens")) {
    return state.revoked
      ? new Response("session revoked", { status: 401 })
      : Response.json({ jwt: FRESH_JWT });
  }
  if (url.pathname.endsWith("/users")) return Response.json([]);
  if (state.apiUnavailable) return new Response("API unavailable", { status: 503 });
  if (new Headers(init?.headers).get("Authorization") !== `Bearer ${FRESH_JWT}`) {
    return new Response("expired JWT", { status: 401 });
  }
  if (url.pathname.endsWith("/workspaces")) {
    return Response.json({ items: state.fleets ? [{ id: "fixture-workspace" }] : [] });
  }
  if (url.pathname.endsWith("/fleets")) {
    return Response.json({ items: [{ id: "fixture-fleet", name: "fixture", status: "killed" }] });
  }
  if (init?.method === "DELETE") {
    state.fleets = 0;
    return new Response(null, { status: 204 });
  }
  throw new Error(`Unexpected fixture request ${init?.method} ${url.pathname}`);
}

beforeEach(() => {
  state.revoked = false;
  state.fleets = 1;
  state.apiUnavailable = false;
  vi.stubEnv("NEXT_PUBLIC_API_URL", "https://api-dev.agentsfleet.net");
  vi.stubEnv("CLERK_SECRET_KEY", "sk_test_fixture");
  vi.stubEnv("AGENTSFLEET_E2E_SESSION_DIRECTORY", undefined);
  vi.stubGlobal("fetch", vi.fn(serviceResponse));
});

afterEach(() => {
  vi.unstubAllEnvs();
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

it("cleans up fleets before revoking the session required to refresh an expired JWT", async () => {
  await globalTeardown();
  expect(state.fleets).toBe(0);
  expect(state.revoked).toBe(true);
});

it("still revokes the session when fleet cleanup fails", async () => {
  state.apiUnavailable = true;
  const log = vi.spyOn(console, "error").mockImplementation(() => {});
  await globalTeardown();
  expect(state.fleets).toBe(1);
  expect(state.revoked).toBe(true);
  expect(log).toHaveBeenCalledWith(
    "[e2e:sweep] done — 0 fixture fleet(s) removed, 3 failed",
  );
});


it("waits for every session revocation when one fails before its siblings finish", async () => {
  const slowResponse = Promise.withResolvers<Response>();
  const allStarted = Promise.withResolvers<void>();
  const calls = { started: 0, completed: 0, teardownFinished: false };
  const log = vi.spyOn(console, "error").mockImplementation(() => {});
  vi.stubGlobal("fetch", vi.fn((input: string | URL | Request, init?: RequestInit) => {
    const url = new URL(input instanceof Request ? input.url : String(input));
    if (!url.pathname.endsWith("/revoke")) return serviceResponse(input, init);
    calls.started += 1;
    if (calls.started === FIXTURE_KEYS.length) allStarted.resolve();
    if (calls.started === 1) return new Response("revocation unauthorized", { status: 401 });
    if (calls.started === 2) return slowResponse.promise.then((response) => {
      calls.completed += 1;
      return response;
    });
    calls.completed += 1;
    return Response.json({ status: "revoked" });
  }));
  const teardown = globalTeardown().then(() => { calls.teardownFinished = true; });
  try {
    await allStarted.promise;
    // Let the rejected request's microtasks settle while its sibling remains
    // held behind an explicit response barrier, independent of network timing.
    await setImmediate();
    expect(calls.teardownFinished).toBe(false);
    expect(calls.completed).toBe(1);
    slowResponse.resolve(Response.json({ status: "revoked" }));
    await teardown;
    expect(calls.completed).toBe(FIXTURE_KEYS.length - 1);
    expect(log).toHaveBeenCalledWith("[e2e:auth] session revocation failed:",
      expect.objectContaining({ errors: [expect.objectContaining({ status: 401 })] }));
  } finally {
    slowResponse.resolve(Response.json({ status: "revoked" }));
    await teardown;
  }
});
