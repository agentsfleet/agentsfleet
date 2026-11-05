// @vitest-environment node
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { TASK_IDS, detail, summary } from "./helpers/task-reads";

const {getToken} = vi.hoisted(() => ({getToken: vi.fn()}));
vi.mock("@clerk/nextjs/server", () => ({auth: async () => ({getToken})}));

import { GET as list } from "../app/live/v1/workspaces/[workspaceId]/fleets/[fleetId]/tasks/route";
import { GET as one, PATCH as change } from "../app/live/v1/workspaces/[workspaceId]/fleets/[fleetId]/tasks/[taskId]/route";

beforeEach(() => {getToken.mockReset(); getToken.mockResolvedValue("fixture-bearer");});
afterEach(() => vi.unstubAllGlobals());

it("proxies scoped task pages and conditional details with server-only identity and no shared cache", async () => {
  const nativePage = {items: [summary()], next_cursor: null};
  const nativeDetail = detail();
  const tag = '"task-version-one"';
  const network = vi.fn<typeof fetch>()
    .mockResolvedValueOnce(Response.json(nativePage))
    .mockResolvedValueOnce(Response.json(nativeDetail, {headers: {ETag: tag}}))
    .mockResolvedValueOnce(new Response(null, {status: 304, headers: {ETag: tag}}));
  vi.stubGlobal("fetch", network);
  const scope = {workspaceId: "workspace/one", fleetId: "fleet?two", taskId: TASK_IDS[0]};
  const first = await list(new Request("https://dashboard.test/live?limit=2&starting_after=stamp%3Aid&token=untrusted"), {params: Promise.resolve(scope)});
  expect([first.status, first.headers.get("cache-control"), await first.json()]).toEqual([200, "no-store", nativePage]);
  const [url, request] = network.mock.calls[0]!;
  expect(url).toMatch(/\/v1\/workspaces\/workspace%2Fone\/fleets\/fleet%3Ftwo\/tasks\?starting_after=stamp%3Aid&limit=2$/);
  expect(request).toMatchObject({method: "GET", cache: "no-store", headers: {Authorization: "Bearer fixture-bearer", Accept: "application/json"}});
  const fresh = await one(new Request("https://dashboard.test/live?limit=100"), {params: Promise.resolve(scope)});
  expect([fresh.status, fresh.headers.get("etag"), await fresh.json()]).toEqual([200, tag, nativeDetail]);
  expect(network.mock.calls[1]?.[0]).toMatch(new RegExp(`/tasks/${TASK_IDS[0]}$`));
  const same = await one(new Request("https://dashboard.test/live", {headers: {"If-None-Match": tag}}), {params: Promise.resolve(scope)});
  expect([same.status, same.headers.get("etag"), same.headers.get("cache-control"), await same.text()]).toEqual([304, tag, "no-store", ""]);
  expect(network.mock.calls[2]?.[1]?.headers).toMatchObject({"If-None-Match": tag, Authorization: "Bearer fixture-bearer"});
  expect(network).toHaveBeenCalledTimes(3);
});


it("forwards task mutations with exact concurrency preconditions through the authenticated whitelist", async () => {
  const tag = `"${"a".repeat(64)}"`;
  const network = vi.fn<typeof fetch>().mockResolvedValue(Response.json(detail(), {headers: {etag: tag}}));
  vi.stubGlobal("fetch", network);
  const scope = {workspaceId: TASK_IDS[0], fleetId: TASK_IDS[1], taskId: TASK_IDS[2]};
  const response = await change(new Request("https://dashboard.test/live", {method: "PATCH",
    headers: {Origin: "https://dashboard.test", "content-type": "application/json", "if-match": tag}, body: JSON.stringify({state: "paused", expected_generation: 1})}), {params: Promise.resolve(scope)});
  expect(response.status).toBe(200);
  expect(network.mock.calls[0]?.[0]).toContain(`/fleets/${TASK_IDS[1]}/tasks/${TASK_IDS[2]}`);
  expect(network.mock.calls[0]?.[1]?.method).toBe("PATCH");
  expect(new Headers(network.mock.calls[0]?.[1]?.headers).get("if-match")).toBe(tag);
});
