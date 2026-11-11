import type {ModelCreationRequest, ModelCreationState} from "@/app/(dashboard)/w/[workspaceId]/settings/models/components/model-creation";
import {act, cleanup, renderHook, waitFor} from "@testing-library/react";
import {afterEach, beforeEach, expect, it, vi} from "vitest";
import {createModel, retryModel} from "@/app/(dashboard)/w/[workspaceId]/settings/models/components/model-creation-operations";
import {useModelCreation} from "@/app/(dashboard)/w/[workspaceId]/settings/models/components/use-model-creation";

const api = vi.hoisted(() => ({store: vi.fn(), register: vi.fn(), activate: vi.fn(), find: vi.fn(), read: vi.fn()}));
vi.mock("@/app/(dashboard)/w/[workspaceId]/secrets/actions", () => ({createSecretAction: api.store}));
vi.mock("@/app/(dashboard)/w/[workspaceId]/settings/models/actions", () => ({createModelEntryAction: api.register,
  setProviderSelfManagedAction: api.activate, findRegisteredModelAction: api.find, readModelActivationAction: api.read}));
const REQUEST: ModelCreationRequest = {workspaceId: "workspace", modelId: "release-model", secretRef: "release-key", activate: true};
const DATA = {provider: "openai", api_key: "sample-provider-key"};
beforeEach(() => {
  vi.resetAllMocks(); api.store.mockResolvedValue({ok: true, data: {name: REQUEST.secretRef}});
  api.register.mockResolvedValue({ok: true, data: {id: "entry"}});
  api.activate.mockResolvedValue({ok: true, data: {mode: "self_managed", model: REQUEST.modelId, secret_ref: REQUEST.secretRef, workspace_id: REQUEST.workspaceId}});
  api.find.mockResolvedValue({ok: true, data: {id: "entry"}}); api.read.mockResolvedValue({ok: true, data: true});
});
afterEach(cleanup);
function operation() {
  const controller = new AbortController();
  const callbacks = {clearKey: vi.fn(), onSecretsChanged: vi.fn(), onCreated: vi.fn(), onComplete: vi.fn()};
  return {controller, callbacks, signal: controller.signal};
}
function held(stage: Exclude<ModelCreationState["stage"], "editing">): ModelCreationState {return {stage, request: REQUEST, error: "Earlier response was lost"};}

it("refuses an already cancelled create or retry before any resource write", async () => {
  const owner = operation(); owner.controller.abort();
  expect(await createModel(REQUEST, DATA, owner)).toMatchObject({stage: "editing"});
  const previous = held("registration"); expect(await retryModel(previous, owner)).toBe(previous);
  expect(api.store).not.toHaveBeenCalled(); expect(api.register).not.toHaveBeenCalled();
});

it.each([403, 503])("keeps credential refusal and uncertainty distinct (%s)", async status => {
  const owner = operation(); api.store.mockResolvedValue({ok: false, status, error: "Storage unavailable"});
  const result = await createModel(REQUEST, DATA, owner);
  expect(result.stage).toBe(status === 403 ? "editing" : "storage_unconfirmed");
  expect(owner.callbacks.clearKey).toHaveBeenCalledTimes(status === 403 ? 0 : 1);
  expect(owner.callbacks.onSecretsChanged).toHaveBeenCalledTimes(status === 403 ? 0 : 1);
  expect(api.register).not.toHaveBeenCalled();
});

it("keeps a lost Server Action response unconfirmed and never replays credential storage", async () => {
  const owner = operation(); api.store.mockRejectedValueOnce(new Error("Connection lost"));
  const result = await createModel(REQUEST, DATA, owner);
  expect(result.stage).toBe("storage_unconfirmed");
  expect(await retryModel(result, owner)).toBe(result);
  expect(api.store).toHaveBeenCalledOnce(); expect(api.register).not.toHaveBeenCalled();
});

it("stops before registration when credential refresh replaces the owner", async () => {
  const owner = operation(); owner.callbacks.onSecretsChanged.mockImplementation(() => owner.controller.abort());
  expect(await createModel(REQUEST, DATA, owner)).toMatchObject({stage: "editing"});
  expect(api.store).toHaveBeenCalledOnce(); expect(api.register).not.toHaveBeenCalled();
});

it("drops registration responses after cancellation without starting activation", async () => {
  const owner = operation(); const pending = Promise.withResolvers<{ok: true; data: {id: string}}>(); api.register.mockReturnValue(pending.promise);
  const creating = createModel(REQUEST, undefined, owner); owner.controller.abort(); pending.resolve({ok: true, data: {id: "entry"}});
  expect(await creating).toMatchObject({stage: "editing"});
  expect(api.activate).not.toHaveBeenCalled(); expect(owner.callbacks.onCreated).not.toHaveBeenCalled();
});

it("retains the created model but starts no activation when its refresh unmounts the owner", async () => {
  const owner = operation(); owner.callbacks.onCreated.mockImplementation(() => owner.controller.abort());
  expect(await createModel(REQUEST, undefined, owner)).toMatchObject({stage: "editing"});
  expect(api.register).toHaveBeenCalledOnce(); expect(api.activate).not.toHaveBeenCalled();
});

it("drops an activation response after cancellation without completing a different form", async () => {
  const owner = operation(); const gate = Promise.withResolvers<{ok: true; data: {mode: string}}>(); api.activate.mockReturnValue(gate.promise);
  const creating = createModel(REQUEST, undefined, owner);
  await waitFor(() => expect(api.activate).toHaveBeenCalledOnce()); owner.controller.abort(); gate.resolve({ok: true, data: {mode: "self_managed"}});
  expect(await creating).toMatchObject({stage: "editing"}); expect(owner.callbacks.onComplete).not.toHaveBeenCalled();
});

it("offers activation alone after a complete read confirms the model is not active", async () => {
  const owner = operation(); api.read.mockResolvedValueOnce({ok: true, data: false});
  const previous = held("activation_unconfirmed"); const result = await retryModel(previous, owner);
  expect(result.stage).toBe("activation"); expect(owner.callbacks.onCreated).toHaveBeenCalledOnce();
  expect(api.register).not.toHaveBeenCalled(); expect(api.activate).not.toHaveBeenCalled();
  expect((await retryModel(result, owner)).stage).toBe("editing"); expect(api.activate).toHaveBeenCalledOnce();
});

it("keeps failed activation-status reads unconfirmed without a write", async () => {
  const owner = operation(); api.read.mockResolvedValueOnce({ok: false, status: 503, error: "Read unavailable"});
  expect((await retryModel(held("activation_unconfirmed"), owner)).stage).toBe("activation_unconfirmed");
  expect(api.activate).not.toHaveBeenCalled();
});

it.each(["registration_unconfirmed", "activation_unconfirmed"] as const)("drops a cancelled %s reconciliation", async stage => {
  const owner = operation(); const gate = Promise.withResolvers<{ok: true; data: null}>();
  (stage === "registration_unconfirmed" ? api.find : api.read).mockReturnValueOnce(gate.promise);
  const retrying = retryModel(held(stage), owner); owner.controller.abort(); gate.resolve({ok: true, data: null});
  expect((await retrying).stage).toBe("editing"); expect(api.activate).not.toHaveBeenCalled(); expect(api.register).not.toHaveBeenCalled();
});

it("leaves editing alone and prevents a new create from replaying a retained credential", async () => {
  const owner = operation(); expect(await retryModel({stage: "editing", error: null}, owner)).toEqual({stage: "editing", error: null});
  api.register.mockResolvedValue({ok: false, status: 403, error: "Register refused"});
  const pendingChange = vi.fn(); const hook = renderHook(() => useModelCreation(owner.callbacks, pendingChange));
  await act(async () => hook.result.current.create(REQUEST, DATA));
  expect(hook.result.current.state.stage).toBe("registration");
  await act(async () => hook.result.current.create(REQUEST, DATA));
  expect(api.store).toHaveBeenCalledOnce(); expect(api.register).toHaveBeenCalledOnce();
  await act(async () => hook.result.current.reset()); expect(hook.result.current.state.stage).toBe("editing");
});

it("stops a React Action after unmount while preserving its already-dispatched storage request", async () => {
  const owner = operation(); const pendingChange = vi.fn(); const hook = renderHook(() => useModelCreation(owner.callbacks, pendingChange));
  const gate = Promise.withResolvers<{ok: true; data: {name: string}}>(); api.store.mockReturnValue(gate.promise);
  act(() => {hook.result.current.create(REQUEST, DATA);});
  await act(async () => {hook.unmount(); gate.resolve({ok: true, data: {name: REQUEST.secretRef}});});
  expect(api.store).toHaveBeenCalledOnce(); expect(api.register).not.toHaveBeenCalled();
  expect(pendingChange).toHaveBeenLastCalledWith(false);
});

it("retries only a refused activation after model registration succeeds", async () => {
  const owner = operation(); api.activate.mockResolvedValueOnce({ok: false, status: 403, error: "Activation refused"});
  const created = await createModel(REQUEST, undefined, owner); expect(created.stage).toBe("activation");
  expect(owner.callbacks.onCreated).toHaveBeenCalledOnce(); expect(owner.callbacks.onComplete).not.toHaveBeenCalled();
  expect((await retryModel(created, owner)).stage).toBe("editing");
  expect(api.register).toHaveBeenCalledOnce(); expect(api.store).not.toHaveBeenCalled(); expect(api.activate).toHaveBeenCalledTimes(2);
});

it("excludes same-tick React Action dispatches before pending is painted", async () => {
  const owner = operation(); const gate = Promise.withResolvers<{ok: true; data: {name: string}}>();
  api.store.mockReturnValueOnce(gate.promise);
  const pendingChange = vi.fn(); const hook = renderHook(() => useModelCreation(owner.callbacks, pendingChange));
  act(() => {hook.result.current.create(REQUEST, DATA); hook.result.current.create(REQUEST, DATA);});
  expect(api.store).toHaveBeenCalledOnce();
  await act(async () => {gate.resolve({ok: true, data: {name: REQUEST.secretRef}});});
  expect(api.register).toHaveBeenCalledOnce(); expect(api.activate).toHaveBeenCalledOnce();
});

it("continues an owned save when the parent replaces its pending-status callback", async () => {
  const owner = operation(); const gate = Promise.withResolvers<{ok: true; data: {name: string}}>();
  api.store.mockReturnValueOnce(gate.promise);
  const hook = renderHook(({pendingChange}) => useModelCreation(owner.callbacks, pendingChange), {initialProps: {pendingChange: vi.fn()}});
  act(() => {hook.result.current.create(REQUEST, DATA);});
  hook.rerender({pendingChange: vi.fn()});
  await act(async () => {gate.resolve({ok: true, data: {name: REQUEST.secretRef}});});
  expect(api.register).toHaveBeenCalledOnce(); expect(api.activate).toHaveBeenCalledOnce();
  expect(owner.callbacks.onComplete).toHaveBeenCalledOnce();
});
