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
