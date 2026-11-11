import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { FleetDetailSchema } from "./api/fleet-detail";
import { FLEET_DETAIL, response } from "@/tests/helpers/draft-fixtures";
import { advanceFleetLifecycle } from "./fleet-lifecycle";

const {stop, remove} = vi.hoisted(() => ({stop: vi.fn(), remove: vi.fn()}));
vi.mock("@/app/(dashboard)/w/[workspaceId]/fleets/actions", () => ({setFleetStatusAction: stop, deleteFleetAction: remove}));
const ACTIVE = {...FLEET_DETAIL, status: "active", etag: '"before-stop"'};
const ARCHIVED = FleetDetailSchema.parse({...FLEET_DETAIL, status: "killed", etag: '"after-stop"'});
const network = vi.fn<typeof fetch>();
const START = {stage: "read"} as const;
const request = (action: "archive" | "delete" = "delete", signal = new AbortController().signal) => ({workspace: "release", fleetId: ACTIVE.id, action, signal});

beforeEach(() => {
  vi.resetAllMocks(); vi.stubGlobal("fetch", network);
  network.mockImplementation(async () => response(ACTIVE));
  stop.mockResolvedValue({ok: true, data: ARCHIVED});
  remove.mockResolvedValue({ok: true, data: undefined});
});
afterEach(() => vi.unstubAllGlobals());

it("archives with the observed version and retains history without a delete", async () => {
  const result = await advanceFleetLifecycle(request("archive"), START);
  expect(result).toEqual({kind: "completed", outcome: "archived"});
  expect(stop).toHaveBeenCalledExactlyOnceWith("release", ACTIVE.id, "killed", ACTIVE.etag);
  expect(remove).not.toHaveBeenCalled();
});

it("permanently deletes directly and uses the version returned by the confirmed stop", async () => {
  const result = await advanceFleetLifecycle(request(), START);
  expect(result).toEqual({kind: "completed", outcome: "deleted"});
  expect(remove).toHaveBeenCalledExactlyOnceWith("release", ACTIVE.id, ARCHIVED.etag);
});

it("checks an uncertain stop before continuing and never blindly repeats it", async () => {
  stop.mockResolvedValueOnce({ok: false, error: "Response lost"});
  const first = await advanceFleetLifecycle(request(), START);
  if (first.kind !== "retry") throw new Error("Expected recovery");
  expect(first.checkpoint.stage).toBe("check_stop");
  const checked = await advanceFleetLifecycle(request(), first.checkpoint);
  expect(checked).toMatchObject({kind: "retry", checkpoint: {stage: "read"}});
  expect(stop).toHaveBeenCalledOnce(); expect(remove).not.toHaveBeenCalled();
  network.mockResolvedValueOnce(response({...ACTIVE, status: "killed", etag: ARCHIVED.etag}));
  expect(await advanceFleetLifecycle(request(), first.checkpoint)).toEqual({kind: "completed", outcome: "deleted"});
  expect(stop).toHaveBeenCalledOnce();
});

it("keeps a confirmed stop and retries deletion alone after checking a failed delete", async () => {
  remove.mockResolvedValueOnce({ok: false, status: 503, error: "Unavailable"});
  const first = await advanceFleetLifecycle(request(), START);
  if (first.kind !== "retry") throw new Error("Expected recovery");
  expect(first.message).toContain("History is retained");
  network.mockResolvedValueOnce(response(ARCHIVED));
  const checked = await advanceFleetLifecycle(request(), first.checkpoint);
  if (checked.kind !== "retry") throw new Error("Expected deletion retry");
  expect(checked.checkpoint).toEqual({stage: "delete", etag: ARCHIVED.etag});
  expect(await advanceFleetLifecycle(request(), checked.checkpoint)).toEqual({kind: "completed", outcome: "deleted"});
  expect(stop).toHaveBeenCalledOnce(); expect(remove).toHaveBeenCalledTimes(2);
});

it("confirms a lost DELETE only from a successful authenticated missing-resource response", async () => {
  remove.mockRejectedValueOnce(new Error("Response lost"));
  const first = await advanceFleetLifecycle(request(), START);
  if (first.kind !== "retry") throw new Error("Expected recovery");
  network.mockResolvedValueOnce(response({}, 404));
  expect(await advanceFleetLifecycle(request(), first.checkpoint)).toEqual({kind: "completed", outcome: "deleted"});
  expect(remove).toHaveBeenCalledOnce();
});

it.each([401, 403, 503])("never counts a %s reconciliation read as deletion", async status => {
  network.mockResolvedValueOnce(response({}, status));
  expect(await advanceFleetLifecycle(request(), {stage: "check_delete"})).toMatchObject({kind: "retry", checkpoint: {stage: "check_delete"}});
  expect(remove).not.toHaveBeenCalled(); expect(stop).not.toHaveBeenCalled();
});

it("does not delete after its owner leaves during a permanent stop", async () => {
  const pending = Promise.withResolvers<{ok: true; data: typeof ARCHIVED}>();
  stop.mockReturnValueOnce(pending.promise);
  const owner = new AbortController();
  const operation = advanceFleetLifecycle(request("delete", owner.signal), START);
  await vi.waitFor(() => expect(stop).toHaveBeenCalledOnce());
  owner.abort(); pending.resolve({ok: true, data: ARCHIVED});
  expect(await operation).toEqual({kind: "cancelled"});
  expect(remove).not.toHaveBeenCalled();
});

it("keeps malformed reads explicit and makes no mutation", async () => {
  network.mockResolvedValueOnce(response({status: "killed"}));
  expect(await advanceFleetLifecycle(request(), {stage: "check_delete"})).toMatchObject({kind: "retry"});
  expect(remove).not.toHaveBeenCalled(); expect(stop).not.toHaveBeenCalled();
});
