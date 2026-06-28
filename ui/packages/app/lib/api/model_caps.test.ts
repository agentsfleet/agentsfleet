import { afterEach, describe, expect, it, vi } from "vitest";

const fetchMock = vi.fn();
vi.stubGlobal("fetch", fetchMock);

afterEach(() => fetchMock.mockReset());

// Mirrors the cap.json wire shape from src/agentsfleetd/http/handlers/model_caps.zig.
const CAP_JSON_OK = {
  version: "2026-04-29",
  models: [
    {
      id: "claude-sonnet-4-6",
      provider: "anthropic",
      context_cap_tokens: 256000,
      input_nanos_per_mtok: 3000000000,
      cached_input_nanos_per_mtok: 300000000,
      output_nanos_per_mtok: 15000000000,
    },
  ],
  rates: { run_nanos_per_sec: 100000, event_nanos: 0 }, // pin test: literal is the contract
  billing: { starter_credit_nanos: 5000000000, free_trial_end_ms: 1785542400000, free_trial_stage_nanos: 0 },
};

describe("getModelCaps", () => {
  it("GETs the public cap.json path unauthenticated and returns the catalogue", async () => {
    fetchMock.mockResolvedValue({
      ok: true,
      status: 200,
      statusText: "OK",
      json: async () => CAP_JSON_OK,
    });
    const { getModelCaps } = await import("./model_caps");
    const res = await getModelCaps();

    expect(fetchMock).toHaveBeenCalledTimes(1);
    const [url, init] = fetchMock.mock.calls[0]!;
    expect(url).toContain("/_um/");
    expect(url).toContain("/cap.json");
    expect((init as { method: string }).method).toBe("GET");
    // Public document — the catalogue carries no per-tenant data, so no bearer token.
    const headers = (init as { headers?: Record<string, string> }).headers ?? {};
    expect(headers).not.toHaveProperty("Authorization");

    // Round-trips the wire body verbatim (parsed deep-equals what the endpoint served).
    expect(res).toEqual(CAP_JSON_OK);
  });

  it("throws on a non-2xx response so the caller can fall back to a catalogue-free path", async () => {
    fetchMock.mockResolvedValue({
      ok: false,
      status: 503,
      statusText: "Service Unavailable",
      json: async () => ({}),
    });
    const { getModelCaps } = await import("./model_caps");
    await expect(getModelCaps()).rejects.toThrow(/503/);
  });
});

const cap = (id: string, provider: string) => ({
  id,
  provider,
  context_cap_tokens: 1,
  input_nanos_per_mtok: 1,
  cached_input_nanos_per_mtok: 1,
  output_nanos_per_mtok: 1,
});

describe("uniqueModelIds", () => {
  it("dedupes by model id, last occurrence wins", async () => {
    const { uniqueModelIds } = await import("./model_caps");
    const out = uniqueModelIds([cap("m1", "anthropic"), cap("m2", "openai"), cap("m1", "openrouter")]);
    expect(out.map((m) => m.id)).toEqual(["m1", "m2"]);
    // last-write-wins: the openrouter m1 replaces the anthropic m1.
    expect(out.find((m) => m.id === "m1")!.provider).toBe("openrouter");
  });
});

describe("modelsForProvider", () => {
  it("keeps only the models of the requested provider", async () => {
    const { modelsForProvider } = await import("./model_caps");
    const out = modelsForProvider([cap("m1", "anthropic"), cap("m2", "openai"), cap("m3", "anthropic")], "anthropic");
    expect(out.map((m) => m.id)).toEqual(["m1", "m3"]);
  });
});

describe("uniqueProviders", () => {
  it("returns distinct provider ids in first-occurrence order", async () => {
    const { uniqueProviders } = await import("./model_caps");
    const out = uniqueProviders([
      cap("m1", "anthropic"),
      cap("m2", "openai"),
      cap("m3", "anthropic"),
      cap("m4", "openai-compatible"),
    ]);
    expect(out).toEqual(["anthropic", "openai", "openai-compatible"]);
  });

  it("returns an empty list for an empty catalogue", async () => {
    const { uniqueProviders } = await import("./model_caps");
    expect(uniqueProviders([])).toEqual([]);
  });
});

describe("providerLabel", () => {
  it("maps the known provider ids to human labels", async () => {
    const { providerLabel } = await import("./model_caps");
    expect(providerLabel("anthropic")).toBe("Anthropic");
    expect(providerLabel("openai")).toBe("OpenAI");
    expect(providerLabel("openai-compatible")).toBe("Custom — OpenAI-compatible");
  });

  it("falls back to the raw slug for an unknown provider id", async () => {
    const { providerLabel } = await import("./model_caps");
    expect(providerLabel("fireworks")).toBe("fireworks");
  });
});
