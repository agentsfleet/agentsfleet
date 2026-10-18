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
  replaceSecretMock,
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
  replaceSecretMock: vi.fn(),
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
vi.mock("@/lib/api/secrets", () => ({ replaceSecret: replaceSecretMock, listSecrets: listSecretsMock }));
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
  replaceSecretAction,
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
  it.each([undefined, false, true])("getModelLibraryAction forwards advanced=%s with the server-side token", async (advanced) => {
    const library = { version: "2026-04-29", models: [] };
    getModelLibraryMock.mockResolvedValueOnce(library);
    const r = await getModelLibraryAction(advanced);
    expect(r).toEqual({ ok: true, data: library });
    expect(getModelLibraryMock).toHaveBeenCalledWith("tok", advanced ?? false);
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

  it("replaceSecretAction forwards (workspaceId, name, data) then token to the client", async () => {
    replaceSecretMock.mockResolvedValueOnce({ name: "anthropic-prod" });
    const body = { provider: "anthropic", api_key: "sk-ant-replaced" };
    const r = await replaceSecretAction("ws_1", "anthropic-prod", body);
    expect(r).toEqual({ ok: true, data: { name: "anthropic-prod" } });
    expect(replaceSecretMock).toHaveBeenCalledWith("ws_1", "anthropic-prod", body, "tok");
    expect(setTenantProviderSelfManagedMock).not.toHaveBeenCalled();
    expect(resetTenantProviderMock).not.toHaveBeenCalled();
  });

  it("listModelEntriesAction forwards only the token through withToken to the client", async () => {
    const registry = { models: [], platform_default_available: true };
    listTenantModelEntriesMock.mockResolvedValueOnce(registry);
    const r = await listModelEntriesAction();
    expect(r).toEqual({ ok: true, data: registry });
    // The action forwards the page cursor too — null is "first page".
    expect(listTenantModelEntriesMock).toHaveBeenCalledWith("tok", null);
  });

  it("createModelEntryAction forwards the body then token through withToken to the client", async () => {
    const created = { id: "e1", model_id: "m1", secret_ref: "s1", created_at: "1970-01-01T00:00:00.001Z" };
    createTenantModelEntryMock.mockResolvedValueOnce(created);
    const body = { model_id: "m1", secret_ref: "s1" };
    const r = await createModelEntryAction(body);
    expect(r).toEqual({ ok: true, data: created });
    expect(createTenantModelEntryMock).toHaveBeenCalledWith(body, "tok");
  });

  it("updateModelEntryAction forwards (id, body) then token through withToken to the client", async () => {
    const updated = { id: "e1", model_id: "m2", secret_ref: "s1", created_at: "1970-01-01T00:00:00.001Z" };
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
    const list = { secrets: [{ kind: "provider_key", name: "anthropic-prod", provider: "anthropic", created_at: "1970-01-01T00:00:00.001Z" }] };
    listSecretsMock.mockResolvedValueOnce(list);
    const r = await listSecretsAction("ws_1");
    expect(r).toEqual({ ok: true, data: list });
    expect(listSecretsMock).toHaveBeenCalledWith("ws_1", "tok");
  });

});
