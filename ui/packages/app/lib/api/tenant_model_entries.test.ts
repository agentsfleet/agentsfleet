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

  // A walk that followed next_cursor to exhaustion was replaced. These
  // three cases pin what replaced it: ONE request per call, the cursor
  // forwarded verbatim, and the disclosure fields surfaced rather than
  // swallowed — the last is what stops paging from hiding entries silently.
  it("issues exactly one request per call and does not follow next_cursor", async () => {
    fetchMock.mockResolvedValue(
      jsonResponse({
        models: [{ id: "e1" }, { id: "e2" }],
        platform_default_available: false,
        total: 7,
        next_cursor: "cur_2",
      }),
    );
    const { listTenantModelEntries } = await import("./tenant_model_entries");
    const res = await listTenantModelEntries("tok");

    // The walk would have issued a second request on seeing "cur_2".
    expect(fetchMock).toHaveBeenCalledTimes(1);
    expect(res.models.map((m) => m.id)).toEqual(["e1", "e2"]);
  });

  it("returns next_cursor and total so the caller can disclose what it has not loaded", async () => {
    // Invariant 5. Dropping either field would let the page render two rows
    // with no way to say seven exist — the silent truncation the walk existed
    // to prevent, reintroduced by the fix for it.
    fetchMock.mockResolvedValue(
      jsonResponse({
        models: [{ id: "e1" }],
        platform_default_available: false,
        total: 7,
        next_cursor: "cur_2",
      }),
    );
    const { listTenantModelEntries } = await import("./tenant_model_entries");
    const res = await listTenantModelEntries("tok");

    expect(res.next_cursor).toBe("cur_2");
    expect(res.total).toBe(7);
  });

  it("forwards starting_after when asked for a later page", async () => {
    fetchMock.mockResolvedValue(
      jsonResponse({
        models: [{ id: "e3" }],
        platform_default_available: true,
        total: 7,
        next_cursor: null,
      }),
    );
    const { listTenantModelEntries } = await import("./tenant_model_entries");
    const res = await listTenantModelEntries("tok", "cur_2");

    const url = fetchMock.mock.calls[0]![0] as string;
    expect(url).toContain("starting_after=cur_2");
    expect(url).toContain("limit=100");
    // A null next_cursor is the server saying this is the last page.
    expect(res.next_cursor).toBeNull();
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
