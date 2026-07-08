import { afterEach, describe, expect, it, vi } from "vitest";

const fetchMock = vi.fn();
vi.stubGlobal("fetch", fetchMock);

afterEach(() => fetchMock.mockReset());

function jsonResponse(body: unknown) {
  return { ok: true, status: 200, json: async () => body };
}

describe("listTenantModelEntries", () => {
  it("GETs /v1/tenants/me/models with bearer", async () => {
    fetchMock.mockResolvedValue(jsonResponse({ models: [], platform_default_available: true }));
    const { listTenantModelEntries } = await import("./tenant_model_entries");
    const res = await listTenantModelEntries("tok");
    expect(fetchMock).toHaveBeenCalledWith(
      expect.stringContaining("/v1/tenants/me/models"),
      expect.objectContaining({ method: "GET", headers: expect.objectContaining({ Authorization: "Bearer tok" }) }),
    );
    expect(res.platform_default_available).toBe(true);
  });
});

describe("createTenantModelEntry", () => {
  it("POSTs {model_id, secret_ref}", async () => {
    fetchMock.mockResolvedValue(jsonResponse({ id: "e1", model_id: "m1", secret_ref: "s1", created_at: 1 }));
    const { createTenantModelEntry } = await import("./tenant_model_entries");
    await createTenantModelEntry({ model_id: "m1", secret_ref: "s1" }, "tok");
    const [, init] = fetchMock.mock.calls[0]!;
    expect(init).toMatchObject({ method: "POST" });
    expect(JSON.parse((init as { body: string }).body)).toEqual({ model_id: "m1", secret_ref: "s1" });
  });
});

describe("updateTenantModelEntry", () => {
  it("PATCHes /v1/tenants/me/models/{id} with the new model_id", async () => {
    fetchMock.mockResolvedValue(jsonResponse({ id: "e1", model_id: "m2", secret_ref: "s1", created_at: 1 }));
    const { updateTenantModelEntry } = await import("./tenant_model_entries");
    await updateTenantModelEntry("e1", { model_id: "m2" }, "tok");
    const [url, init] = fetchMock.mock.calls[0]!;
    expect(url).toContain("/v1/tenants/me/models/e1");
    expect(init).toMatchObject({ method: "PATCH" });
    expect(JSON.parse((init as { body: string }).body)).toEqual({ model_id: "m2" });
  });
});

describe("deleteTenantModelEntry", () => {
  it("DELETEs /v1/tenants/me/models/{id}", async () => {
    fetchMock.mockResolvedValue({ ok: true, status: 204, json: async () => ({}) });
    const { deleteTenantModelEntry } = await import("./tenant_model_entries");
    await deleteTenantModelEntry("e1", "tok");
    const [url, init] = fetchMock.mock.calls[0]!;
    expect(url).toContain("/v1/tenants/me/models/e1");
    expect(init).toMatchObject({ method: "DELETE" });
  });
});
