import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { FleetDetailSchema } from "./api/fleet-detail";
import { FLEET_DETAIL, response } from "@/tests/helpers/draft-fixtures";
import { advanceFleetLifecycle } from "./fleet-lifecycle";

const {archive, remove} = vi.hoisted(() => ({archive: vi.fn(), remove: vi.fn()}));
vi.mock("@/app/(dashboard)/w/[workspaceId]/fleets/actions", () => ({setFleetStatusAction: archive, deleteFleetAction: remove}));
const ACTIVE = {...FLEET_DETAIL, status: "active", etag: '"before-archive"'};
const ARCHIVED = FleetDetailSchema.parse({...FLEET_DETAIL, status: "archived", etag: '"after-archive"'});
const network = vi.fn<typeof fetch>();
const START = {stage: "read"} as const;
const request = (action: "archive" | "delete" = "delete", signal = new AbortController().signal) => ({workspace: "release", fleetId: ACTIVE.id, action, signal});

beforeEach(() => {
  vi.resetAllMocks(); vi.stubGlobal("fetch", network);
  network.mockImplementation(async () => response(ACTIVE));
  archive.mockResolvedValue({ok: true, data: ARCHIVED});
  remove.mockResolvedValue({ok: true, data: undefined});
});
afterEach(() => vi.unstubAllGlobals());

it("archives with the observed version and retains history without a delete", async () => {
  const result = await advanceFleetLifecycle(request("archive"), START);
  expect(result).toEqual({kind: "completed", outcome: "archived"});
  expect(archive).toHaveBeenCalledExactlyOnceWith("release", ACTIVE.id, "archived", ACTIVE.etag);
  expect(remove).not.toHaveBeenCalled();
});

it("permanently deletes directly with the freshly observed version", async () => {
  const result = await advanceFleetLifecycle(request(), START);
  expect(result).toEqual({kind: "completed", outcome: "deleted"});
  expect(remove).toHaveBeenCalledExactlyOnceWith("release", ACTIVE.id, ACTIVE.etag);
  expect(archive).not.toHaveBeenCalled();
});

it("checks an uncertain archive before retrying and never blindly repeats it", async () => {
  archive.mockResolvedValueOnce({ok: false, error: "Response lost"});
  const first = await advanceFleetLifecycle(request("archive"), START);
  if (first.kind !== "retry") throw new Error("Expected recovery");
  expect(first.checkpoint.stage).toBe("check_archive");
  expect(await advanceFleetLifecycle(request("archive"), first.checkpoint)).toMatchObject({kind: "retry", checkpoint: {stage: "read"}});
  expect(archive).toHaveBeenCalledOnce(); expect(remove).not.toHaveBeenCalled();
  network.mockResolvedValueOnce(response(ARCHIVED));
  expect(await advanceFleetLifecycle(request("archive"), first.checkpoint)).toEqual({kind: "completed", outcome: "archived"});
  expect(archive).toHaveBeenCalledOnce();
});

it("checks a failed delete and retries deletion with the newly observed version", async () => {
  remove.mockResolvedValueOnce({ok: false, status: 503, error: "Unavailable"});
  const first = await advanceFleetLifecycle(request(), START);
  if (first.kind !== "retry") throw new Error("Expected recovery");
  expect(first.message).toContain("History is retained");
  network.mockResolvedValueOnce(response(ARCHIVED));
  const checked = await advanceFleetLifecycle(request(), first.checkpoint);
  if (checked.kind !== "retry") throw new Error("Expected deletion retry");
  expect(checked.checkpoint).toEqual({stage: "delete", etag: ARCHIVED.etag});
  expect(await advanceFleetLifecycle(request(), checked.checkpoint)).toEqual({kind: "completed", outcome: "deleted"});
  expect(archive).not.toHaveBeenCalled(); expect(remove).toHaveBeenCalledTimes(2);
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
  expect(remove).not.toHaveBeenCalled(); expect(archive).not.toHaveBeenCalled();
});

it("abandons an archive response after its owner leaves", async () => {
  const pending = Promise.withResolvers<{ok: true; data: typeof ARCHIVED}>();
  archive.mockReturnValueOnce(pending.promise);
  const owner = new AbortController();
  const operation = advanceFleetLifecycle(request("archive", owner.signal), START);
  await vi.waitFor(() => expect(archive).toHaveBeenCalledOnce());
  owner.abort(); pending.resolve({ok: true, data: ARCHIVED});
  expect(await operation).toEqual({kind: "cancelled"});
  expect(remove).not.toHaveBeenCalled();
});

it("keeps malformed reads explicit and makes no mutation", async () => {
  network.mockResolvedValueOnce(response({status: "archived"}));
  expect(await advanceFleetLifecycle(request(), {stage: "check_delete"})).toMatchObject({kind: "retry"});
  expect(remove).not.toHaveBeenCalled(); expect(archive).not.toHaveBeenCalled();
});

it.each(["active", "installing", "archived"])("deletes a %s fleet without a preceding archive", async status => {
  network.mockResolvedValueOnce(response({...ACTIVE, status}));
  expect(await advanceFleetLifecycle(request(), START)).toEqual({kind: "completed", outcome: "deleted"});
  expect(remove).toHaveBeenCalledExactlyOnceWith("release", ACTIVE.id, ACTIVE.etag);
  expect(archive).not.toHaveBeenCalled();
});
