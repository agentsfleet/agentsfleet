import { ADMIN_MODEL, METADATA, PRICE } from "@/tests/helpers/catalogue-fixtures";
import { readCatalogueBody, stringifyCatalogueBody } from "./catalogue-json";
import { afterEach, describe, expect, it, vi } from "vitest";
import { listAdminModels, createAdminModel, updateAdminModel, deleteAdminModel, setPlatformDefault, listPlatformKeys, activePlatformDefault, type PlatformKey } from "./admin_model_library";

const fetchMock = vi.fn();
vi.stubGlobal("fetch", fetchMock);
afterEach(() => fetchMock.mockReset());

function okJson(body: unknown) {
  return new Response(stringifyCatalogueBody(body), { status: 200 });
}

// ── Authed admin client: correct verb + path + body to the wire contract ─────
describe("admin model catalogue client", () => {
  it("should GET the admin catalogue with a bearer token", async () => {
    fetchMock.mockResolvedValue(okJson({ models: [] }));
    await listAdminModels("tok_abc");
    const [url, init] = fetchMock.mock.calls[0]!;
    expect(String(url)).toContain("/v1/admin/models");
    expect(init.method).toBe("GET");
    expect(init.headers.Authorization).toBe("Bearer tok_abc");
  });

  it("should POST a new catalogue row as JSON", async () => {
    fetchMock.mockResolvedValue(okJson(ADMIN_MODEL));
    const body = { provider: ADMIN_MODEL.provider, model_id: ADMIN_MODEL.model_id, metadata: METADATA, prices: [PRICE] };
    await createAdminModel("tok", body);
    const [url, init] = fetchMock.mock.calls[0]!;
    expect(String(url)).toContain("/v1/admin/models");
    expect(init.method).toBe("POST");
    expect(await readCatalogueBody(new Response(init.body))).toEqual(body);
  });

  it("should PATCH caps/rates by id (identity stays immutable)", async () => {
    fetchMock.mockResolvedValue(okJson({ id: "u1", updated: true }));
    await updateAdminModel("tok", "u1", { metadata: METADATA, prices: [PRICE] });
    const [url, init] = fetchMock.mock.calls[0]!;
    expect(String(url)).toContain("/v1/admin/models/u1");
    expect(init.method).toBe("PATCH");
  });

  it("should DELETE a catalogue row by id", async () => {
    fetchMock.mockResolvedValue({ ok: true, status: 204, statusText: "No Content", json: async () => ({}) });
    await deleteAdminModel("tok", "u1");
    const [url, init] = fetchMock.mock.calls[0]!;
    expect(String(url)).toContain("/v1/admin/models/u1");
    expect(init.method).toBe("DELETE");
  });

  it("should PUT the platform default with provider/model/workspace", async () => {
    fetchMock.mockResolvedValue(okJson({ provider: "fireworks", model: "glm-5.2", active: true }));
    await setPlatformDefault("tok", {
      provider: "fireworks",
      source_workspace_id: "ws1",
      model: "glm-5.2",
    });
    const [url, init] = fetchMock.mock.calls[0]!;
    expect(String(url)).toContain("/v1/admin/platform-keys");
    expect(init.method).toBe("PUT");
    expect(JSON.parse(init.body)).toMatchObject({ provider: "fireworks", model: "glm-5.2", source_workspace_id: "ws1" });
  });

  it("should url-encode a id with reserved characters in the path", async () => {
    fetchMock.mockResolvedValue({ ok: true, status: 204, statusText: "No Content", json: async () => ({}) });
    await deleteAdminModel("tok", "a/b");
    const [url] = fetchMock.mock.calls[0]!;
    expect(String(url)).toContain("/v1/admin/models/a%2Fb");
  });

  it("should GET the platform-keys list with a bearer token", async () => {
    fetchMock.mockResolvedValue(okJson({ keys: [] }));
    await listPlatformKeys("tok_abc");
    const [url, init] = fetchMock.mock.calls[0]!;
    expect(String(url)).toContain("/v1/admin/platform-keys");
    expect(init.method).toBe("GET");
    expect(init.headers.Authorization).toBe("Bearer tok_abc");
  });
});

// ── Active-default selection: the one active row (or null) drives the badge ───
describe("activePlatformDefault", () => {
  const active: PlatformKey = { provider: "fireworks", source_workspace_id: "ws1", model: "glm-5.2", active: true, updated_at: "1970-01-01T00:00:00.002Z" };
  const inactive: PlatformKey = { provider: "anthropic", source_workspace_id: "ws1", model: null, active: false, updated_at: "1970-01-01T00:00:00.001Z" };

  it("returns the single active row when one is set", () => {
    expect(activePlatformDefault({ keys: [inactive, active] })).toBe(active);
  });

  it("returns null when no row is active", () => {
    expect(activePlatformDefault({ keys: [inactive] })).toBeNull();
    expect(activePlatformDefault({ keys: [] })).toBeNull();
  });
});

it("returns complete metadata and preserves nanos beyond JavaScript's safe integer range", async () => {
  const model = { ...ADMIN_MODEL, prices: [{ ...PRICE, input_nanos_per_mtok: "9223372036854775807" }] };
  fetchMock.mockResolvedValue(okJson({ models: [model] }));
  expect((await listAdminModels("token")).models[0]).toEqual(model);
});
it("refuses malformed read rows", async () => {
  fetchMock.mockResolvedValue(okJson({ models: [{ id: "invalid" }] }));
  await expect(listAdminModels("token")).rejects.toThrow();
});
it("leaves the server's schedule refusal visible to the editor", async () => {
  const detail = "date ranges for one tier overlap";
  fetchMock.mockResolvedValue(new Response(JSON.stringify({ detail, error_code: "UZ-LIBRARY-003" }), { status: 400 }));
  await expect(updateAdminModel("token", "u1", { metadata: METADATA, prices: [PRICE] })).rejects.toMatchObject({ message: detail, status: 400 });
});

it.each([{ updated: true }, { id: "u1", updated: "yes" }])("refuses a malformed PATCH result %j", async (body) => {
  fetchMock.mockResolvedValue(okJson(body));
  await expect(updateAdminModel("token", "u1", { metadata: METADATA, prices: [PRICE] })).rejects.toThrow();
});
it("sends exact int64 write amounts and preserves nulls on PATCH", async () => {
  fetchMock.mockResolvedValue(okJson({ id: "u1", updated: true }));
  const prices = [{ ...PRICE, input_nanos_per_mtok: "9223372036854775807", cache_write_5m_nanos_per_mtok: "1" }];
  await updateAdminModel("token", "a/b", { metadata: METADATA, prices });
  const [url, init] = fetchMock.mock.calls[0]!;
  expect(url).toContain("/a%2Fb");
  expect(init.body).toContain('"input_nanos_per_mtok":9223372036854775807');
  expect(init.body).toContain('"cache_write_5m_nanos_per_mtok":1');
  expect(init.body).toContain('"cache_write_nanos_per_mtok":null');
});
