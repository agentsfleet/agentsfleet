import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { listFleetMemoriesAction } from "@/app/(dashboard)/w/[workspaceId]/fleets/actions";

const {getToken} = vi.hoisted(() => ({getToken: vi.fn()}));
vi.mock("@clerk/nextjs/server", () => ({auth: async () => ({getToken})}));
const SESSION = ["fixture", "session"].join("-");
const WORKSPACE = "workspace";
const FLEET = "fleet";
beforeEach(() => {getToken.mockResolvedValue(SESSION);});
afterEach(() => {vi.resetAllMocks(); vi.unstubAllGlobals();});

// Value: protects=authenticated fleet memory reads; fails_when=workspace or fleet scope is lost, or the server credential reaches returned data;
// why_new=component tests replace the server action; seam=none; no_claim=identity verification or backend authorization.
it("reads fleet memories with the server credential and returns only response data", async () => {
  const fetch = vi.fn(async (_url: RequestInfo | URL, _init?: RequestInit) => Response.json({items: [], next_cursor: null}));
  vi.stubGlobal("fetch", fetch);
  const result = await listFleetMemoriesAction(WORKSPACE, FLEET);
  expect(result).toEqual({ok: true, data: {items: []}});
  expect(fetch).toHaveBeenCalledOnce();
  const [url, init] = fetch.mock.calls[0] ?? [];
  expect(url).toBe(`/backend/v1/workspaces/${WORKSPACE}/fleets/${FLEET}/memories?limit=100`);
  expect(new Headers(init?.headers).get("Authorization")).toBe(`Bearer ${SESSION}`);
  expect(JSON.stringify(result)).not.toContain(SESSION);
});

// Value: protects=anonymous read refusal; fails_when=memory reads fetch private data without a session;
// why_new=forwarding tests replace authentication; seam=none; no_claim=backend tenant isolation.
it("refuses anonymous fleet memory reads before a network request", async () => {
  getToken.mockResolvedValue(null);
  const fetch = vi.fn();
  vi.stubGlobal("fetch", fetch);
  expect(await listFleetMemoriesAction(WORKSPACE, FLEET)).toMatchObject({ok: false, error: "Not authenticated", status: 401});
  expect(fetch).not.toHaveBeenCalled();
});

// Value: protects=permission failures stay visible; fails_when=denied reads become empty successful memory data;
// why_new=component tests replace server normalization; seam=none; no_claim=backend permission evaluation.
it("preserves a denied fleet memory read instead of returning an empty success", async () => {
  const fetch = vi.fn(async () => Response.json({user_message: "Workspace access refused", error_code: "UZ-AUTH-022"}, {status: 403}));
  vi.stubGlobal("fetch", fetch);
  expect(await listFleetMemoriesAction(WORKSPACE, FLEET)).toMatchObject({ok: false, status: 403, error: "Workspace access refused", errorCode: "UZ-AUTH-022"});
  expect(fetch).toHaveBeenCalledOnce();
});
