import { beforeEach, describe, expect, it, vi } from "vitest";

const requestMock = vi.hoisted(() => vi.fn());

// Module-level mock, per-test state: without the reset, call counts leak across
// tests and the suite only passes in file order (found by the shuffled run).
beforeEach(() => {
  requestMock.mockReset();
});
vi.mock("./client", () => ({ request: requestMock }));

import { getModelLibrary } from "./model_library";
import { modelsForProvider, providerLabel, uniqueModelIds, uniqueProviders, type ModelLibrary } from "./model-library-types";

// Mirrors the GET /v1/models wire shape from
// rustd/crates/afd_api_tenant/src/handler/tenant/models/mod.rs.
const LIBRARY_OK: ModelLibrary = {
  version: "2026-04-29",
  models: [
    {
      id: "claude-sonnet-4-6",
      provider: "anthropic",
      display_name: "Sonnet 4.6",
      advanced: false,
      context_cap_tokens: 256000,
      input_nanos_per_mtok: 3000000000,
      cached_input_nanos_per_mtok: 300000000,
      output_nanos_per_mtok: 15000000000,
    },
  ],
};

describe("getModelLibrary", () => {
  it("includes advanced models only when requested", async () => {
    requestMock.mockResolvedValue(LIBRARY_OK);
    await getModelLibrary("token_abc", true);
    // pin test: literal is the contract
    expect(requestMock).toHaveBeenCalledWith("/v1/models?advanced=true", { method: "GET" }, "token_abc");
  });

  it("GETs /v1/models through the authed request helper and returns the library", async () => {
    requestMock.mockResolvedValue(LIBRARY_OK);
    const res = await getModelLibrary("token_abc");

    expect(requestMock).toHaveBeenCalledTimes(1);
    const [path, init, token] = requestMock.mock.calls[0]!;
    // pin test: the literal is the wire path, shared verbatim with
    // `TenantRoute::ModelLibrary` in rustd/crates/afd_http/src/route/tenant.rs.
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
  display_name: id,
  advanced: false,
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
  it("maps the known provider ids to the vendor's own spelling", () => {
    expect(providerLabel("anthropic")).toBe("Anthropic");
    expect(providerLabel("openai")).toBe("OpenAI");
    expect(providerLabel("gemini")).toBe("Google");
    expect(providerLabel("openai-compatible")).toBe("Custom — OpenAI-compatible");
    expect(providerLabel("fireworks")).toBe("Fireworks");
    expect(providerLabel("openrouter")).toBe("OpenRouter");
    expect(providerLabel("nearai")).toBe("NEAR AI");
    expect(providerLabel("vercel-ai")).toBe("Vercel AI");
    expect(providerLabel("atlas-cloud")).toBe("Atlas Cloud");
  });

  it("spells the registry's providers the way each vendor does, where title case would not", () => {
    // pin test: literal is the contract — each is a vendor's own casing.
    expect(providerLabel("xai")).toBe("xAI");
    expect(providerLabel("deepseek")).toBe("DeepSeek");
    expect(providerLabel("huggingface")).toBe("Hugging Face");
    expect(providerLabel("minimax")).toBe("MiniMax");
    expect(providerLabel("nvidia")).toBe("NVIDIA");
    expect(providerLabel("siliconflow")).toBe("SiliconFlow");
    expect(providerLabel("ovhcloud")).toBe("OVHcloud");
    expect(providerLabel("glm")).toBe("GLM");
    expect(providerLabel("together")).toBe("Together AI");
  });

  it("reads only its own labels, so an inherited name falls back to title case", () => {
    expect(providerLabel("__proto__")).toBe("Proto");
    expect(providerLabel("constructor")).toBe("Constructor");
    expect(providerLabel("toString")).toBe("ToString");
  });

  it("title-cases an unknown provider id instead of showing the raw slug", () => {
    expect(providerLabel("lambda-labs")).toBe("Lambda Labs");
    expect(providerLabel("deep-infra")).toBe("Deep Infra");
    expect(providerLabel("some_vendor")).toBe("Some Vendor");
    expect(providerLabel("")).toBe("");
  });
});
