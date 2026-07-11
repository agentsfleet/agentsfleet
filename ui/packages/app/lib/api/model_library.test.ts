import { describe, expect, it, vi } from "vitest";

const requestMock = vi.hoisted(() => vi.fn());
vi.mock("./client", () => ({ request: requestMock }));

import { getModelLibrary, modelsForProvider, providerLabel, uniqueModelIds, uniqueProviders, type ModelLibrary } from "./model_library";

// Mirrors the GET /v1/models wire shape from
// src/agentsfleetd/http/handlers/model_library.zig.
const LIBRARY_OK: ModelLibrary = {
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
};

describe("getModelLibrary", () => {
  it("GETs /v1/models through the authed request helper and returns the library", async () => {
    requestMock.mockResolvedValue(LIBRARY_OK);
    const res = await getModelLibrary("token_abc");

    expect(requestMock).toHaveBeenCalledTimes(1);
    const [path, init, token] = requestMock.mock.calls[0]!;
    // pin test: literal is the contract — the wire path shared verbatim with
    // MODEL_LIBRARY_PATH in the Zig handler and this client.
    expect(path).toBe("/v1/models");
    expect((init as { method: string }).method).toBe("GET");
    // The route is bearer-authed — the token threads through to request(),
    // which owns the Authorization header.
    expect(token).toBe("token_abc");

    // Round-trips the wire body verbatim.
    expect(res).toEqual(LIBRARY_OK);
  });

  it("propagates a request failure so the caller can fall back to a catalogue-free path", async () => {
    requestMock.mockRejectedValue(new Error("503 Service Unavailable"));
    await expect(getModelLibrary("token_abc")).rejects.toThrow(/503/);
  });
});

const model = (id: string, provider: string) => ({
  id,
  provider,
  context_cap_tokens: 1,
  input_nanos_per_mtok: 1,
  cached_input_nanos_per_mtok: 1,
  output_nanos_per_mtok: 1,
});

describe("uniqueModelIds", () => {
  it("dedupes by model id, last occurrence wins", () => {
    const out = uniqueModelIds([model("m1", "anthropic"), model("m2", "openai"), model("m1", "openrouter")]);
    expect(out.map((m) => m.id)).toEqual(["m1", "m2"]);
    // last-write-wins: the openrouter m1 replaces the anthropic m1.
    expect(out.find((m) => m.id === "m1")!.provider).toBe("openrouter");
  });
});

describe("modelsForProvider", () => {
  it("keeps only the models of the requested provider", () => {
    const out = modelsForProvider([model("m1", "anthropic"), model("m2", "openai"), model("m3", "anthropic")], "anthropic");
    expect(out.map((m) => m.id)).toEqual(["m1", "m3"]);
  });
});

describe("uniqueProviders", () => {
  it("returns distinct provider ids in first-occurrence order", () => {
    const out = uniqueProviders([
      model("m1", "anthropic"),
      model("m2", "openai"),
      model("m3", "anthropic"),
      model("m4", "openai-compatible"),
    ]);
    expect(out).toEqual(["anthropic", "openai", "openai-compatible"]);
  });

  it("returns an empty list for an empty catalogue", () => {
    expect(uniqueProviders([])).toEqual([]);
  });
});

describe("providerLabel", () => {
  it("maps the known provider ids to human labels", () => {
    expect(providerLabel("anthropic")).toBe("Anthropic");
    expect(providerLabel("openai")).toBe("OpenAI");
    expect(providerLabel("openai-compatible")).toBe("Custom — OpenAI-compatible");
  });

  it("falls back to the raw slug for an unknown provider id", () => {
    expect(providerLabel("fireworks")).toBe("fireworks");
  });
});
