import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

// These provider server actions are thin forwarders: each export wraps
// withToken((t) => apiFn(args, t)). We mock the token wrapper and the API
// client so the only thing under test is the forwarding shape — argument
// order and the wrapped {ok,data} envelope (the real round-trip lives in the
// api client suite + the backend integration suite).

// vi.mock is hoisted above the static actions import, so the mock fns must be
// created via vi.hoisted() to exist when the factories run.
const {
  withTokenMock,
  setTenantProviderSelfManagedMock,
  resetTenantProviderMock,
  rotateSecretMock,
  listSecretsMock,
  listTenantModelEntriesMock,
  createTenantModelEntryMock,
  updateTenantModelEntryMock,
  deleteTenantModelEntryMock,
  getModelLibraryMock,
} = vi.hoisted(() => ({
  withTokenMock: vi.fn(),
  setTenantProviderSelfManagedMock: vi.fn(),
  resetTenantProviderMock: vi.fn(),
  rotateSecretMock: vi.fn(),
  listSecretsMock: vi.fn(),
  listTenantModelEntriesMock: vi.fn(),
  createTenantModelEntryMock: vi.fn(),
  updateTenantModelEntryMock: vi.fn(),
  deleteTenantModelEntryMock: vi.fn(),
  getModelLibraryMock: vi.fn(),
}));

vi.mock("@/lib/actions/with-token", () => ({ withToken: withTokenMock }));
vi.mock("@/lib/api/tenant_provider", () => ({
  setTenantProviderSelfManaged: setTenantProviderSelfManagedMock,
  resetTenantProvider: resetTenantProviderMock,
}));
vi.mock("@/lib/api/secrets", () => ({ rotateSecret: rotateSecretMock, listSecrets: listSecretsMock }));
vi.mock("@/lib/api/tenant_model_entries", () => ({
  listTenantModelEntries: listTenantModelEntriesMock,
  createTenantModelEntry: createTenantModelEntryMock,
  updateTenantModelEntry: updateTenantModelEntryMock,
  deleteTenantModelEntry: deleteTenantModelEntryMock,
}));
vi.mock("@/lib/api/model_library", () => ({ getModelLibrary: getModelLibraryMock }));

import {
  setProviderSelfManagedAction,
  resetProviderAction,
  rotateSecretAction,
  listSecretsAction,
  listModelEntriesAction,
  createModelEntryAction,
  updateModelEntryAction,
  deleteModelEntryAction,
  getModelLibraryAction,
} from "@/app/(dashboard)/w/[workspaceId]/settings/models/actions";

beforeEach(() => {
  vi.clearAllMocks();
  // withToken forwards a resolved token to its callback for the happy path.
  withTokenMock.mockImplementation(async (fn: (t: string) => Promise<unknown>) => ({
    ok: true,
    data: await fn("tok"),
  }));
});
afterEach(() => vi.resetAllMocks());

describe("provider server actions — thin forwarders", () => {
  it("getModelLibraryAction threads only the token through withToken to the library client", async () => {
    const library = { version: "2026-04-29", models: [] };
    getModelLibraryMock.mockResolvedValueOnce(library);
    const r = await getModelLibraryAction();
    expect(r).toEqual({ ok: true, data: library });
    expect(getModelLibraryMock).toHaveBeenCalledWith("tok");
  });

  it("getModelLibraryAction returns withToken's failure envelope untouched (degrade path input)", async () => {
    withTokenMock.mockResolvedValueOnce({ ok: false, error: "Not authenticated", status: 401 });
    const r = await getModelLibraryAction();
    expect(r).toEqual({ ok: false, error: "Not authenticated", status: 401 });
    expect(getModelLibraryMock).not.toHaveBeenCalled();
  });

  it("setProviderSelfManagedAction forwards the body then token through withToken to the client", async () => {
    const provider = { mode: "self_managed", model: "claude-opus" };
    setTenantProviderSelfManagedMock.mockResolvedValueOnce(provider);
    const body = { secret_ref: "vault://anthropic", model: "claude-opus" };
    const r = await setProviderSelfManagedAction(body);
    expect(r).toEqual({ ok: true, data: provider });
    expect(setTenantProviderSelfManagedMock).toHaveBeenCalledWith(body, "tok");
    expect(resetTenantProviderMock).not.toHaveBeenCalled();
  });

  it("setProviderSelfManagedAction forwards a body with only secret_ref (model omitted)", async () => {
    setTenantProviderSelfManagedMock.mockResolvedValueOnce({ mode: "self_managed" });
    const body = { secret_ref: "vault://openai" };
    const r = await setProviderSelfManagedAction(body);
    expect(r).toEqual({ ok: true, data: { mode: "self_managed" } });
    expect(setTenantProviderSelfManagedMock).toHaveBeenCalledWith(body, "tok");
  });

  it("resetProviderAction forwards only the token through withToken to the client", async () => {
    const provider = { mode: "managed" };
    resetTenantProviderMock.mockResolvedValueOnce(provider);
    const r = await resetProviderAction();
    expect(r).toEqual({ ok: true, data: provider });
    expect(resetTenantProviderMock).toHaveBeenCalledWith("tok");
    expect(setTenantProviderSelfManagedMock).not.toHaveBeenCalled();
  });

  it("both actions route through withToken exactly once", async () => {
    setTenantProviderSelfManagedMock.mockResolvedValueOnce({});
    resetTenantProviderMock.mockResolvedValueOnce({});
    await setProviderSelfManagedAction({ secret_ref: "vault://x" });
    await resetProviderAction();
    expect(withTokenMock).toHaveBeenCalledTimes(2);
  });

  it("rotateSecretAction forwards (workspaceId, name, apiKey) then token to the client", async () => {
    rotateSecretMock.mockResolvedValueOnce({ name: "anthropic-prod" });
    const r = await rotateSecretAction("ws_1", "anthropic-prod", "sk-ant-rotated");
    expect(r).toEqual({ ok: true, data: { name: "anthropic-prod" } });
    expect(rotateSecretMock).toHaveBeenCalledWith("ws_1", "anthropic-prod", "sk-ant-rotated", "tok");
    expect(setTenantProviderSelfManagedMock).not.toHaveBeenCalled();
    expect(resetTenantProviderMock).not.toHaveBeenCalled();
  });

  it("listModelEntriesAction forwards only the token through withToken to the client", async () => {
    const registry = { models: [], platform_default_available: true };
    listTenantModelEntriesMock.mockResolvedValueOnce(registry);
    const r = await listModelEntriesAction();
    expect(r).toEqual({ ok: true, data: registry });
    expect(listTenantModelEntriesMock).toHaveBeenCalledWith("tok");
  });

  it("createModelEntryAction forwards the body then token through withToken to the client", async () => {
    const created = { id: "e1", model_id: "m1", secret_ref: "s1", created_at: 1 };
    createTenantModelEntryMock.mockResolvedValueOnce(created);
    const body = { model_id: "m1", secret_ref: "s1" };
    const r = await createModelEntryAction(body);
    expect(r).toEqual({ ok: true, data: created });
    expect(createTenantModelEntryMock).toHaveBeenCalledWith(body, "tok");
  });

  it("updateModelEntryAction forwards (id, body) then token through withToken to the client", async () => {
    const updated = { id: "e1", model_id: "m2", secret_ref: "s1", created_at: 1 };
    updateTenantModelEntryMock.mockResolvedValueOnce(updated);
    const r = await updateModelEntryAction("e1", { model_id: "m2" });
    expect(r).toEqual({ ok: true, data: updated });
    expect(updateTenantModelEntryMock).toHaveBeenCalledWith("e1", { model_id: "m2" }, "tok");
  });

  it("deleteModelEntryAction forwards (id) then token through withToken to the client", async () => {
    deleteTenantModelEntryMock.mockResolvedValueOnce(undefined);
    const r = await deleteModelEntryAction("e1");
    expect(r).toEqual({ ok: true, data: undefined });
    expect(deleteTenantModelEntryMock).toHaveBeenCalledWith("e1", "tok");
  });

  it("listSecretsAction forwards (workspaceId) then token through withToken to the client", async () => {
    const list = { secrets: [{ kind: "provider_key", name: "anthropic-prod", provider: "anthropic", created_at: 1 }] };
    listSecretsMock.mockResolvedValueOnce(list);
    const r = await listSecretsAction("ws_1");
    expect(r).toEqual({ ok: true, data: list });
    expect(listSecretsMock).toHaveBeenCalledWith("ws_1", "tok");
  });

  it("every registry action routes through withToken exactly once", async () => {
    listTenantModelEntriesMock.mockResolvedValueOnce({ models: [], platform_default_available: true });
    createTenantModelEntryMock.mockResolvedValueOnce({});
    updateTenantModelEntryMock.mockResolvedValueOnce({});
    deleteTenantModelEntryMock.mockResolvedValueOnce(undefined);
    await listModelEntriesAction();
    await createModelEntryAction({ model_id: "m1", secret_ref: "s1" });
    await updateModelEntryAction("e1", { model_id: "m2" });
    await deleteModelEntryAction("e1");
    expect(withTokenMock).toHaveBeenCalledTimes(4);
  });
});
