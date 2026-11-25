import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { listFleetMemoriesAction } from "@/app/(dashboard)/w/[workspaceId]/fleets/actions";
import { readConnectorCatalogAction } from "@/app/(dashboard)/w/[workspaceId]/integrations/connector-actions";

const {getToken} = vi.hoisted(() => ({getToken: vi.fn()}));
vi.mock("@clerk/nextjs/server", () => ({auth: async () => ({getToken})}));
const SESSION = ["fixture", "session"].join("-");
const WORKSPACE = "workspace";
const FLEET = "fleet";
const CASES = [
  {name: "connector catalog", read: () => readConnectorCatalogAction(WORKSPACE),
    path: `/v1/workspaces/${WORKSPACE}/connectors`, payload: [{id: "github", configured: true}]},
  {name: "fleet memories", read: () => listFleetMemoriesAction(WORKSPACE, FLEET),
    path: `/v1/workspaces/${WORKSPACE}/fleets/${FLEET}/memories?limit=100`, payload: {items: [], next_cursor: null}},
];
beforeEach(() => {getToken.mockResolvedValue(SESSION);});
afterEach(() => {vi.resetAllMocks(); vi.unstubAllGlobals();});

// Value: protects=authenticated setup reads; fails_when=workspace or fleet scope is lost, or the server credential reaches returned data;
// why_new=component tests replace both server actions; seam=none; no_claim=identity verification or backend authorization.
it.each(CASES)("reads $name with the server credential and returns only response data", async ({read, path, payload}) => {
  const fetch = vi.fn(async (_url: RequestInfo | URL, _init?: RequestInit) => Response.json(payload));
  vi.stubGlobal("fetch", fetch);
  const result = await read();
  expect(result).toEqual({ok: true, data: Array.isArray(payload) ? payload : {items: []}});
  expect(fetch).toHaveBeenCalledOnce();
  const [url, init] = fetch.mock.calls[0] ?? [];
  expect(url).toBe(`/backend${path}`);
  expect(new Headers(init?.headers).get("Authorization")).toBe(`Bearer ${SESSION}`);
  expect(JSON.stringify(result)).not.toContain(SESSION);
});

// Value: protects=anonymous read refusal; fails_when=setup reads fetch private data without a session;
// why_new=forwarding tests replace authentication; seam=none; no_claim=backend tenant isolation.
it.each(CASES)("refuses anonymous $name reads before a network request", async ({read}) => {
  getToken.mockResolvedValue(null);
  const fetch = vi.fn();
  vi.stubGlobal("fetch", fetch);
  expect(await read()).toMatchObject({ok: false, error: "Not authenticated", status: 401});
  expect(fetch).not.toHaveBeenCalled();
});

// Value: protects=permission failures stay visible; fails_when=denied reads become empty successful setup data;
// why_new=component tests replace server normalization; seam=none; no_claim=backend permission evaluation.
it.each(CASES)("preserves a denied $name read instead of returning an empty success", async ({read}) => {
  const fetch = vi.fn(async () => Response.json({user_message: "Workspace access refused", error_code: "UZ-AUTH-022"}, {status: 403}));
  vi.stubGlobal("fetch", fetch);
  expect(await read()).toMatchObject({ok: false, status: 403, error: "Workspace access refused", errorCode: "UZ-AUTH-022"});
  expect(fetch).toHaveBeenCalledOnce();
});
