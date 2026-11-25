import {afterEach, beforeEach, expect, it, vi} from "vitest";
import {cleanupSoak} from "./e2e/acceptance/fixtures/soak-cleanup";
import {SOAK_FLEET_PREFIX} from "./e2e/acceptance/fixtures/soak-scenario";
import {AGENTSFLEET_STATUS} from "@/lib/api/fleets-types";
import {HTTP_STATUS_NOT_FOUND} from "@/lib/api/errors";
import {FLEET_DETAIL, FLEET_ID, WORKSPACE} from "./helpers/fleet-fixtures";

const NAME = `${SOAK_FLEET_PREFIX}single-retained`;
const IDENTIFIERS = {workspace: WORKSPACE, name: NAME, fleet: FLEET_ID};
const METHOD = {get: "GET", patch: "PATCH", delete: "DELETE"} as const;
const handle = {sessionJwt: crypto.randomUUID()};
const fetcher = vi.fn<typeof fetch>();
const requestUrl = (url: Parameters<typeof fetch>[0]): string => url instanceof Request ? url.url : url.toString();

beforeEach(() => {
  vi.stubEnv("NEXT_PUBLIC_API_URL", "https://api-dev.agentsfleet.net");
  vi.stubGlobal("fetch", fetcher);
  fetcher.mockReset();
  fetcher.mockImplementation(async (_url, init) => init?.method === METHOD.delete
    ? new Response(null, {status: 204})
    : Response.json({...FLEET_DETAIL, name: NAME, status: AGENTSFLEET_STATUS.ACTIVE}));
});
afterEach(() => {vi.unstubAllGlobals(); vi.unstubAllEnvs();});

it("refuses a production target before reading or deleting any records", async () => {
  vi.stubEnv("NEXT_PUBLIC_API_URL", "https://api.agentsfleet.net");
  await expect(cleanupSoak(IDENTIFIERS, handle)).rejects.toThrow("refusing to mass-delete");
  expect(fetcher).not.toHaveBeenCalled();
});

it.each([{...IDENTIFIERS, name: "a-user-fleet"}, {...IDENTIFIERS, workspace: "../other"}, {...IDENTIFIERS, extra: true},
  {...IDENTIFIERS, drafts: []}])(
  "rejects an invalid identifier attachment before any API call", async input => {
    await expect(cleanupSoak(input, handle)).rejects.toThrow();
    expect(fetcher).not.toHaveBeenCalled();
  },
);

it("leaves the fleet intact if the live name differs", async () => {
  fetcher.mockResolvedValueOnce(Response.json({...FLEET_DETAIL, name: "different-fleet"}));
  await expect(cleanupSoak(IDENTIFIERS, handle)).rejects.toThrow("name does not match");
  expect(fetcher).toHaveBeenCalledOnce();
  expect(fetcher.mock.calls[0]?.[1]?.method).toBe(METHOD.get);
});

it("removes only the supplied fleet, after a guarded name check", async () => {
  await cleanupSoak(IDENTIFIERS, handle);
  const deleted = fetcher.mock.calls.filter(([, init]) => init?.method === METHOD.delete).map(([url]) => requestUrl(url));
  expect(deleted).toEqual([`https://api-dev.agentsfleet.net/v1/workspaces/${WORKSPACE}/fleets/${FLEET_ID}`]);
  // DELETE ends a live fleet's work itself: nothing archives first.
  expect(fetcher.mock.calls.some(([, init]) => init?.method === METHOD.patch)).toBe(false);
});

it("reads a fleet with no identifier as nothing left to remove", async () => {
  await cleanupSoak({...IDENTIFIERS, fleet: null}, handle);
  expect(fetcher).not.toHaveBeenCalled();
});

it("resumes a cleanup whose fleet deletion was interrupted, and treats a fleet already gone as removed", async () => {
  const interruption = new Error("fleet deletion interrupted");
  let removed = false;
  fetcher.mockImplementation(async (_url, init) => {
    if (removed) return Response.json({error_code: "fixture-already-removed"}, {status: HTTP_STATUS_NOT_FOUND});
    if (init?.method !== METHOD.delete) return Response.json({...FLEET_DETAIL, name: NAME, status: AGENTSFLEET_STATUS.ARCHIVED});
    removed = true;
    throw interruption;
  });
  await expect(cleanupSoak(IDENTIFIERS, handle)).rejects.toBe(interruption);
  await expect(cleanupSoak(IDENTIFIERS, handle)).resolves.toBeUndefined();
  expect(fetcher.mock.calls.at(-1)?.[1]?.method).toBe(METHOD.get);
});

// pin test: authentication, ownership, and server failures never mean absence.
it.each([401, 403, 503])("keeps HTTP %i errors visible and performs no deletes", async status => {
  fetcher.mockResolvedValueOnce(Response.json({error_code: "fixture-read-failed"}, {status}));
  await expect(cleanupSoak(IDENTIFIERS, handle)).rejects.toMatchObject({status});
  expect(fetcher).toHaveBeenCalledOnce();
  expect(fetcher.mock.calls[0]?.[1]?.method).toBe(METHOD.get);
});
