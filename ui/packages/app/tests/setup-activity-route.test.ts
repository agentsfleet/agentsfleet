// @vitest-environment node
import { afterEach, beforeEach, expect, it, vi } from "vitest";

const {getToken} = vi.hoisted(() => ({getToken: vi.fn()}));
vi.mock("@clerk/nextjs/server", () => ({auth: async () => ({getToken})}));
vi.mock("@/lib/api/client", () => ({API_ORIGIN: "https://api.example.test", request: vi.fn()}));
import { GET } from "@/app/live/v1/workspaces/[workspaceId]/fleets/[fleetId]/messages/route";

const upstream = vi.fn();
const params = {params: Promise.resolve({workspaceId: "workspace", fleetId: "fleet"})};
beforeEach(() => {getToken.mockReset(); upstream.mockReset(); vi.stubGlobal("fetch", upstream);});
afterEach(() => vi.unstubAllGlobals());

it("forwards a bounded history read with the existing server-held credential", async () => {
  getToken.mockResolvedValue("fixture-session");
  upstream.mockResolvedValue(new Response(JSON.stringify({items: [], next_cursor: null})));
  const result = await GET(new Request("http://app.test/live?limit=25&starting_after=one&actor=drop"), params);
  expect(result.status).toBe(200);
  expect(await result.json()).toEqual({items: [], next_cursor: null});
  expect(result.headers.get("Cache-Control")).toBe("no-store");
  expect(upstream.mock.calls[0]?.[0]).toBe("https://api.example.test/v1/workspaces/workspace/fleets/fleet/messages?starting_after=one&limit=25");
});

it("refuses signed-out history reads without calling the backend", async () => {
  getToken.mockResolvedValue(null);
  const result = await GET(new Request("http://app.test/live"), params);
  expect(result.status).toBe(401);
  expect(await result.json()).toMatchObject({error: "Unauthorized"});
  expect(upstream.mock.calls).toEqual([]);
});

it.each([403, 404, 503])("preserves an upstream history refusal (%i)", async status => {
  getToken.mockResolvedValue("fixture-session");
  upstream.mockResolvedValue(new Response(JSON.stringify({detail: "History unavailable"}), {status, headers: {"Content-Type": "application/json"}}));
  const result = await GET(new Request("http://app.test/live"), params);
  expect(result.status).toBe(status);
  expect(await result.json()).toEqual({detail: "History unavailable"});
});
