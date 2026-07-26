import { afterEach, describe, expect, it, vi } from "vitest";

const fetchMock = vi.fn();
vi.stubGlobal("fetch", fetchMock);

afterEach(() => fetchMock.mockReset());

function jsonResponse(body: unknown) {
  return { ok: true, status: 200, json: async () => body };
}

describe("listTenantModelEntries", () => {
  it("GETs /v1/tenants/me/models with bearer, asking for a full page", async () => {
    fetchMock.mockResolvedValue(
      jsonResponse({ models: [], platform_default_available: true, total: null, next_cursor: null }),
    );
    const { listTenantModelEntries } = await import("./tenant_model_entries");
    const res = await listTenantModelEntries("tok");
    expect(fetchMock).toHaveBeenCalledWith(
      expect.stringContaining("/v1/tenants/me/models"),
      expect.objectContaining({ method: "GET", headers: expect.objectContaining({ Authorization: "Bearer tok" }) }),
    );
    // The endpoint pages at 50 unless asked otherwise, so the limit is not
    // decoration: without it a tenant's 51st entry never reaches the page.
    expect(fetchMock).toHaveBeenCalledTimes(1);
    const url = fetchMock.mock.calls[0]![0] as string;
    expect(url).toContain("limit=100");
    expect(url).not.toContain("starting_after");
    expect(res.platform_default_available).toBe(true);
  });

  it("follows next_cursor to exhaustion so entries past the first page survive", async () => {
    fetchMock
      .mockResolvedValueOnce(
        jsonResponse({
          models: [{ id: "e1" }, { id: "e2" }],
          platform_default_available: false,
          total: null,
          next_cursor: "cur_2",
        }),
      )
      .mockResolvedValueOnce(
        jsonResponse({
          models: [{ id: "e3" }],
          platform_default_available: true,
          total: null,
          next_cursor: null,
        }),
      );
    const { listTenantModelEntries } = await import("./tenant_model_entries");
    const res = await listTenantModelEntries("tok");

    expect(fetchMock).toHaveBeenCalledTimes(2);
    expect(fetchMock.mock.calls[0]![0] as string).not.toContain("starting_after");
    expect(fetchMock.mock.calls[1]![0] as string).toContain("starting_after=cur_2");
    // Concatenated in page order, not just the first page's two rows.
    expect(res.models.map((m) => m.id)).toEqual(["e1", "e2", "e3"]);
    // Tenant-wide, so it comes from the page that ended the walk.
    expect(res.platform_default_available).toBe(true);
  });

  it("throws instead of returning duplicates when the cursor never advances", async () => {
    // A server that repeats a cursor would otherwise have its one page
    // collected 50 times and rendered as 50 distinct registry entries.
    fetchMock.mockResolvedValue(
      jsonResponse({
        models: [{ id: "e1" }],
        platform_default_available: false,
        total: null,
        next_cursor: "stuck",
      }),
    );
    const { listTenantModelEntries } = await import("./tenant_model_entries");
    await expect(listTenantModelEntries("tok")).rejects.toThrow(/did not terminate/);
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
