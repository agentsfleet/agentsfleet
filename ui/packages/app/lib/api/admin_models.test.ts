import { afterEach, describe, expect, it, vi } from "vitest";
import {
  nanosToUsdPerMtok,
  usdPerMtokToNanos,
  NANOS_PER_USD,
  listAdminModels,
  createAdminModel,
  updateAdminModel,
  deleteAdminModel,
  setPlatformDefault,
} from "./admin_models";

const fetchMock = vi.fn();
vi.stubGlobal("fetch", fetchMock);
afterEach(() => fetchMock.mockReset());

function okJson(body: unknown) {
  return { ok: true, status: 200, statusText: "OK", json: async () => body };
}

// ── Rate conversion: $/1M ⇄ integer nanos (billing spine lives in integers) ──
describe("nanos ⇄ $/1M conversion", () => {
  it("should render stored nanos as $/1M tokens", () => {
    expect(nanosToUsdPerMtok(3_000_000_000)).toBe(3); // $3/M input (Sonnet)
    expect(nanosToUsdPerMtok(550_000_000)).toBe(0.55); // $0.55/M
    expect(nanosToUsdPerMtok(0)).toBe(0); // self-managed-only model
  });

  it("should convert $/1M entry to integer nanos at submit", () => {
    expect(usdPerMtokToNanos(0.55)).toBe(550_000_000);
    expect(usdPerMtokToNanos(2.19)).toBe(2_190_000_000);
    expect(usdPerMtokToNanos(0)).toBe(0);
  });

  it("should round to an integer nanos (no fractional nanos reach the wire)", () => {
    // 0.000000001 $/M = 1 nano exactly; a sub-nano entry rounds, never NaN/float.
    expect(Number.isInteger(usdPerMtokToNanos(2.191))).toBe(true);
    expect(usdPerMtokToNanos(1 / 3)).toBe(Math.round((1 / 3) * NANOS_PER_USD));
  });

  it("should round-trip a catalogue rate within a nano", () => {
    for (const usd of [0, 0.1, 0.95, 2.19, 15, 75]) {
      expect(nanosToUsdPerMtok(usdPerMtokToNanos(usd))).toBeCloseTo(usd, 6);
    }
  });
});

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
    fetchMock.mockResolvedValue(okJson({ uid: "u1" }));
    const body = {
      provider: "fireworks",
      model_id: "glm-5.2",
      context_cap_tokens: 128000,
      input_nanos_per_mtok: 550_000_000,
      cached_input_nanos_per_mtok: 140_000_000,
      output_nanos_per_mtok: 2_190_000_000,
    };
    await createAdminModel("tok", body);
    const [url, init] = fetchMock.mock.calls[0]!;
    expect(String(url)).toContain("/v1/admin/models");
    expect(init.method).toBe("POST");
    expect(JSON.parse(init.body)).toEqual(body);
  });

  it("should PATCH caps/rates by uid (identity stays immutable)", async () => {
    fetchMock.mockResolvedValue(okJson({ uid: "u1", updated: true }));
    await updateAdminModel("tok", "u1", {
      context_cap_tokens: 200000,
      input_nanos_per_mtok: 1,
      cached_input_nanos_per_mtok: 0,
      output_nanos_per_mtok: 2,
    });
    const [url, init] = fetchMock.mock.calls[0]!;
    expect(String(url)).toContain("/v1/admin/models/u1");
    expect(init.method).toBe("PATCH");
  });

  it("should DELETE a catalogue row by uid", async () => {
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

  it("should url-encode a uid with reserved characters in the path", async () => {
    fetchMock.mockResolvedValue({ ok: true, status: 204, statusText: "No Content", json: async () => ({}) });
    await deleteAdminModel("tok", "a/b");
    const [url] = fetchMock.mock.calls[0]!;
    expect(String(url)).toContain("/v1/admin/models/a%2Fb");
  });
});
