import { StrictMode } from "react";
import { act, cleanup, renderHook, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, it } from "vitest";
import { useFleetModel } from "./useFleetModel";
import { FleetModelFixture, MODEL_OWNER, RUNTIME_MODEL } from "@/tests/helpers/fleet-model-fixture";
import { DRAFT_ID, MODEL, MODEL_REF, response } from "@/tests/helpers/draft-fixtures";

const fixture = new FleetModelFixture();
beforeEach(() => fixture.mount());
afterEach(() => {cleanup(); fixture.dispose();});

async function open() {
  const hook = renderHook(() => useFleetModel(MODEL_OWNER), {wrapper: StrictMode});
  await act(async () => {await hook.result.current.load();});
  return hook;
}

it("saves a runtime-only model change without generating a message", async () => {
  const {result} = await open();
  expect(result.current.models.map(model => model.ref)).toEqual([MODEL_REF, RUNTIME_MODEL.ref]);
  await act(async () => {expect(await result.current.select(RUNTIME_MODEL.ref)).toBe(true);});
  expect(result.current.confirmed).toEqual(RUNTIME_MODEL);
  expect(fixture.mutations().map(call => call.method)).toEqual(["POST", "PATCH", "POST", "DELETE"]);
  expect(fixture.mutations()[0]?.body).not.toHaveProperty("message");
  expect(fixture.mutations()[1]?.body).toEqual({instance_name: "repairer", runtime_model_ref: RUNTIME_MODEL.ref});
  expect(fixture.applyKeys).toHaveLength(1);
  expect(fixture.applyKeys[0]).not.toBe("");
});

it.each(["edit", "apply"])("retries a lost %s acknowledgment with the same key and body", async (step) => {
  const {result} = await open();
  fixture.loseApply = step === "apply"; fixture.loseEdit = step === "edit";
  await act(async () => {expect(await result.current.select(RUNTIME_MODEL.ref)).toBe(false);});
  expect(result.current).toMatchObject({pending: true, locked: true, confirmed: null, retryable: true});
  act(() => result.current.resetRefusal());
  expect(result.current).toMatchObject({pending: true, locked: true, retryable: true});
  const before = fixture.mutations().filter(call => call.method !== "DELETE");
  await act(async () => {expect(await result.current.select(MODEL_REF)).toBe(false);});
  expect(fixture.mutations()).toHaveLength(before.length);
  await act(async () => {expect(await result.current.retry()).toBe(true);});
  const replay = fixture.mutations().filter(call => call.method !== "DELETE");
  const original = step === "edit" ? before[0] : before.at(-1);
  const retried = replay.find((call, index) => index >= before.length && call.path === original?.path && call.method === original.method);
  expect(retried?.body).toEqual(original?.body);
  expect(retried?.headers).toEqual(original?.headers);
  if (step === "edit") expect(replay.filter(call => call.method === "PATCH")).toHaveLength(1);
  expect(fixture.applied.size).toBe(1);
  expect(result.current).toMatchObject({pending: false, confirmed: RUNTIME_MODEL});
});

it("retains the saved model when the setup changes before apply", async () => {
  const {result} = await open(); fixture.revision = "2";
  await act(async () => {expect(await result.current.select(RUNTIME_MODEL.ref)).toBe(false);});
  expect(result.current.confirmed).toBeNull();
  expect(result.current.error).toContain("fleet changed");
  await act(async () => {await result.current.load(); expect(await result.current.select(RUNTIME_MODEL.ref)).toBe(false);});
  expect(result.current.error).toContain("fleet changed");
  expect(fixture.mutations()).toEqual([]);
});

it.each([409, 412])("retains the saved model after a refused apply (%s)", async (status) => {
  const {result} = await open(); fixture.saveStatus = status;
  await act(async () => {expect(await result.current.select(RUNTIME_MODEL.ref)).toBe(false);});
  expect(result.current).toMatchObject({confirmed: null, pending: false, retryable: false, error: "Another editor saved a newer version."});
  expect(fixture.applied.size).toBe(0);
  expect(fixture.mutations().filter(call => call.method === "DELETE")).toHaveLength(1);
});

it("cleans up a refused helper draft before beginning another model save", async () => {
  const {result} = await open(); fixture.saveStatus = 400; fixture.cleanupFails = true;
  await act(async () => {expect(await result.current.select(RUNTIME_MODEL.ref)).toBe(false);});
  expect(fixture.mutations().filter(call => call.method === "DELETE")).toHaveLength(1);
  expect(result.current.confirmed).toBeNull();
  fixture.cleanupFails = false; fixture.saveStatus = 201;
  await act(async () => {expect(await result.current.select(RUNTIME_MODEL.ref)).toBe(true);});
  expect(fixture.mutations().map(call => call.method)).toEqual(["POST", "PATCH", "POST", "DELETE", "DELETE", "POST", "PATCH", "POST", "DELETE"]);
  expect(result.current.confirmed).toEqual(RUNTIME_MODEL);
});

it("keeps a refused save locked until its helper cleanup settles", async () => {
  const {result} = await open(); fixture.saveStatus = 412;
  const deletion = Promise.withResolvers<Response>();
  const normal = fixture.override;
  fixture.override = call => call.method === "DELETE" ? deletion.promise : normal(call);
  let saving: Promise<boolean> | undefined;
  act(() => {saving = result.current.select(RUNTIME_MODEL.ref);});
  await waitFor(() => expect(fixture.mutations().filter(call => call.method === "DELETE")).toHaveLength(1));
  act(() => result.current.resetRefusal());
  expect(result.current).toMatchObject({reloadRequired: true, saving: true, locked: true});
  deletion.resolve(response(null, 204));
  await act(async () => {expect(await saving).toBe(false);});
  act(() => result.current.resetRefusal());
  expect(result.current).toMatchObject({reloadRequired: false, error: null, confirmed: null});
  expect(fixture.applied.size).toBe(0);
});

it("refuses a review that does not confirm the selected runtime model", async () => {
  const {result} = await open(); fixture.mismatch = true;
  await act(async () => {expect(await result.current.select(RUNTIME_MODEL.ref)).toBe(false);});
  expect(result.current.error).toContain("selected model could not be confirmed");
  expect(fixture.applyKeys).toEqual([]);
});

it("shows model loading failures without changing the saved model", async () => {
  const hook = renderHook(() => useFleetModel(MODEL_OWNER));
  fixture.override = () => response({}, 503);
  await act(async () => {await hook.result.current.load();});
  expect(hook.result.current).toMatchObject({confirmed: null, error: "Models couldn’t be loaded. Try again.", loading: false});
  expect(fixture.mutations()).toEqual([]);
});

it("clears models when workspace access is lost", async () => {
  const {result} = await open(); fixture.override = () => response({}, 403);
  await act(async () => {await result.current.load();});
  expect(result.current).toMatchObject({models: [], inaccessible: true, locked: true});
  await act(async () => {expect(await result.current.select(RUNTIME_MODEL.ref)).toBe(false); await result.current.load(true);});
  expect(fixture.mutations()).toEqual([]);
});

it.each([401, 403, 404, 410])("clears private choices when the fleet preview loses access (%s)", async (status) => {
  const {result} = await open();
  const normal = fixture.override;
  fixture.override = call => call.path.endsWith(`/fleets/${MODEL_OWNER.fleet}`) ? response({}, status) : normal(call);
  await act(async () => {expect(await result.current.select(RUNTIME_MODEL.ref)).toBe(false);});
  expect(result.current).toMatchObject({models: [], inaccessible: true, locked: true, confirmed: null});
  expect(fixture.mutations()).toEqual([]);
});

it("ignores an unchanged selection and rejects an unavailable model without a write", async () => {
  const {result} = await open();
  await act(async () => {expect(await result.current.select(MODEL_REF)).toBe(false); expect(await result.current.retry()).toBe(false);});
  await act(async () => {expect(await result.current.select("model:removed")).toBe(false);});
  expect(result.current.error).toContain("no longer available");
  expect(fixture.mutations()).toEqual([]);
});

it("keeps a confirmed model save successful when private draft cleanup fails", async () => {
  const {result} = await open(); fixture.cleanupFails = true;
  await act(async () => {expect(await result.current.select(RUNTIME_MODEL.ref)).toBe(true);});
  expect(result.current).toMatchObject({confirmed: RUNTIME_MODEL, error: null});
});

it("loads another bounded model page without creating a draft", async () => {
  const more = {...RUNTIME_MODEL, ref: "model:later", label: "Later model"};
  let reads = 0;
  fixture.override = call => call.path.endsWith("/models") ? response({items: reads++ ? [more] : [MODEL], next_cursor: reads === 1 ? "next" : null}) : undefined;
  const {result} = await open();
  expect(result.current.more).toBe(true);
  await act(async () => {await result.current.load(true);});
  expect(result.current.models.map(model => model.ref)).toEqual([MODEL_REF, more.ref]);
  expect(result.current.more).toBe(false);
  expect(fixture.mutations()).toEqual([]);
});

it("aborts reads and refuses late updates after the owning workspace unmounts", async () => {
  const {result, unmount} = await open();
  const pending = Promise.withResolvers<Response>();
  fixture.override = call => call.path.endsWith(`/drafts/${DRAFT_ID}`) && call.method === "PATCH" ? pending.promise : undefined;
  let saving: Promise<boolean> | undefined;
  act(() => {saving = result.current.select(RUNTIME_MODEL.ref);});
  await act(async () => {await new Promise(resolve => setTimeout(resolve, 0));});
  unmount(); pending.resolve(response({}));
  expect(await saving).toBe(false);
  expect(fixture.applyKeys).toEqual([]);
});

it("keeps a missing account inert", async () => {
  const {result} = renderHook(() => useFleetModel(null));
  await act(async () => {await result.current.load(); await result.current.select(RUNTIME_MODEL.ref); await result.current.retry(); result.current.resetRefusal();});
  expect(result.current.locked).toBe(true);
  expect(fixture.calls).toEqual([]);
});

it("retries a failed setup read before apply without repeating its native model edit", async () => {
  const {result} = await open();
  const normal = fixture.override;
  let fleetReads = 0;
  fixture.override = call => call.path.endsWith(`/fleets/${MODEL_OWNER.fleet}`) && ++fleetReads === 2
    ? response({}, 503) : normal(call);
  await act(async () => {expect(await result.current.select(RUNTIME_MODEL.ref)).toBe(false);});
  expect(result.current).toMatchObject({pending: false, retryable: true, confirmed: null});
  await act(async () => {expect(await result.current.retry()).toBe(true);});
  expect(fixture.mutations().filter(call => call.method === "PATCH")).toHaveLength(1);
  expect(result.current.confirmed).toEqual(RUNTIME_MODEL);
});

it("does not publish a late model-list result after unmount", async () => {
  const {result, unmount} = renderHook(() => useFleetModel(MODEL_OWNER));
  const pending = Promise.withResolvers<Response>();
  fixture.override = call => call.path.endsWith("/models") ? pending.promise : undefined;
  let loading: Promise<void> | undefined;
  act(() => {loading = result.current.load();});
  unmount();
  pending.resolve(response({items: [MODEL], next_cursor: null}));
  await loading;
  expect(fixture.mutations()).toEqual([]);
});
