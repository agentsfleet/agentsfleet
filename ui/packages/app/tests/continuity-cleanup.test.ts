import {afterEach, beforeEach, expect, it, vi} from "vitest";
import {cleanupContinuity} from "./e2e/acceptance/fixtures/continuity-cleanup";
import {CONTINUITY_FLEET_PREFIX} from "./e2e/acceptance/fixtures/continuity-scenario";
import {AGENTSFLEET_STATUS} from "@/lib/api/fleets-types";
import {HTTP_STATUS_NOT_FOUND} from "@/lib/api/errors";
import {CANDIDATE_ID, DRAFT_ID, FLEET, FLEET_ID, WORKSPACE} from "./helpers/draft-fixtures";

const NAME = `${CONTINUITY_FLEET_PREFIX}single-retained`;
const IDENTIFIERS = {workspace: WORKSPACE, name: NAME, fleet: FLEET_ID, drafts: [DRAFT_ID, CANDIDATE_ID]};
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
    : Response.json({...FLEET, name: NAME, status: init?.method === METHOD.patch ? AGENTSFLEET_STATUS.KILLED : AGENTSFLEET_STATUS.ACTIVE}));
});
afterEach(() => {vi.unstubAllGlobals(); vi.unstubAllEnvs();});

it("refuses a production target before reading or deleting any records", async () => {
  vi.stubEnv("NEXT_PUBLIC_API_URL", "https://api.agentsfleet.net");
  await expect(cleanupContinuity(IDENTIFIERS, handle)).rejects.toThrow("refusing to mass-delete");
  expect(fetcher).not.toHaveBeenCalled();
});

it.each([{...IDENTIFIERS, name: "a-user-fleet"}, {...IDENTIFIERS, workspace: "../other"}, {...IDENTIFIERS, extra: true}])(
  "rejects an invalid identifier attachment before any API call", async input => {
    await expect(cleanupContinuity(input, handle)).rejects.toThrow();
    expect(fetcher).not.toHaveBeenCalled();
  },
);

it("leaves the fleet and drafts intact if the live name differs", async () => {
  fetcher.mockResolvedValueOnce(Response.json({...FLEET, name: "different-fleet"}));
  await expect(cleanupContinuity(IDENTIFIERS, handle)).rejects.toThrow("name does not match");
  expect(fetcher).toHaveBeenCalledOnce();
  expect(fetcher.mock.calls[0]?.[1]?.method).toBe(METHOD.get);
});

it("removes only the supplied fleet and drafts, after a guarded name check", async () => {
  await cleanupContinuity(IDENTIFIERS, handle);
  const deleted = fetcher.mock.calls.filter(([, init]) => init?.method === METHOD.delete).map(([url]) => requestUrl(url));
  const scope = `https://api-dev.agentsfleet.net/v1/workspaces/${WORKSPACE}`;
  expect(deleted).toEqual([`${scope}/fleets/${FLEET_ID}`, ...IDENTIFIERS.drafts.map(id => `${scope}/drafts/${id}`)]);
});

it("retains drafts when the fleet cannot be removed", async () => {
  fetcher.mockImplementation(async (_url, init) => init?.method === METHOD.delete
    ? Response.json({error_code: "fixture-delete-failed"}, {status: 409})
    : Response.json({...FLEET, name: NAME, status: AGENTSFLEET_STATUS.KILLED}));
  await expect(cleanupContinuity(IDENTIFIERS, handle)).rejects.toThrow("409");
  expect(fetcher.mock.calls.some(([url]) => requestUrl(url).includes("/drafts/"))).toBe(false);
});

it("resumes a partial cleanup using the same saved identifiers", async () => {
  const removed = new Set<string>();
  const interruption = new Error("draft deletion interrupted");
  let interrupted = false;
  fetcher.mockImplementation(async (url, init) => {
    const target = requestUrl(url);
    if (removed.has(target)) return Response.json({error_code: "fixture-already-removed"}, {status: HTTP_STATUS_NOT_FOUND});
    if (init?.method !== METHOD.delete) return Response.json({...FLEET, name: NAME, status: AGENTSFLEET_STATUS.KILLED});
    if (target.endsWith(CANDIDATE_ID) && !interrupted) {interrupted = true; throw interruption;}
    removed.add(target);
    return new Response(null, {status: 204});
  });
  await expect(cleanupContinuity(IDENTIFIERS, handle)).rejects.toBe(interruption);
  expect(removed.size).toBe(2);
  await expect(cleanupContinuity(IDENTIFIERS, handle)).resolves.toBeUndefined();
  expect(removed.size).toBe(3);
});

// pin test: authentication, ownership, and server failures never mean absence.
it.each([401, 403, 503])("keeps HTTP %i errors visible and performs no deletes", async status => {
  fetcher.mockResolvedValueOnce(Response.json({error_code: "fixture-read-failed"}, {status}));
  await expect(cleanupContinuity(IDENTIFIERS, handle)).rejects.toMatchObject({status});
  expect(fetcher).toHaveBeenCalledOnce();
  expect(fetcher.mock.calls[0]?.[1]?.method).toBe(METHOD.get);
});
