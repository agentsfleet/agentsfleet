import { describe, expect, it } from "vitest";
import {
  isHttpsUrl,
  BASE_URL_NOT_HTTPS,
} from "@/app/(dashboard)/w/[workspaceId]/settings/models/lib/custom-endpoint";
import { LOCAL_RUNTIME_PROVIDERS, isLocalRuntime, OPENAI_COMPATIBLE_PROVIDER } from "@/lib/types";

// Covers the shared custom-endpoint client validation extracted for the
// consolidated Models forms. The server re-validates and additionally
// blocks SSRF-unsafe hosts — this is only the cheap inline https gate.
describe("isHttpsUrl", () => {
  it("accepts a well-formed https URL", () => {
    expect(isHttpsUrl("https://vllm.corp/v1")).toBe(true);
  });

  it("trims surrounding whitespace before checking", () => {
    expect(isHttpsUrl("  https://vllm.corp/v1  ")).toBe(true);
  });

  it("rejects a plain http URL", () => {
    expect(isHttpsUrl("http://vllm.corp/v1")).toBe(false);
  });

  it("rejects a value that does not start with the https scheme prefix", () => {
    expect(isHttpsUrl("ftp://vllm.corp")).toBe(false);
    expect(isHttpsUrl("vllm.corp/v1")).toBe(false);
  });

  it("rejects a malformed value that passes the prefix test but fails URL parsing", () => {
    // Starts with "https://" so the cheap prefix gate passes, but `new URL`
    // throws on the empty host — the try/catch returns false.
    expect(isHttpsUrl("https://")).toBe(false);
  });

  it("rejects empty / whitespace-only input", () => {
    expect(isHttpsUrl("")).toBe(false);
    expect(isHttpsUrl("   ")).toBe(false);
  });
});

describe("BASE_URL_NOT_HTTPS", () => {
  it("is the inline https-required hint", () => {
    expect(BASE_URL_NOT_HTTPS).toBe("Use https:// for the Base URL.");
  });
});

// The dashboard's mirror of the server's local-runtime set. Both credential
// carve-outs (no api_key, no catalogue membership) key off this predicate, so a
// narrowed implementation — `provider === "ollama"` — would silently strip both
// exemptions from the other eight while every dialog test, which drives ollama
// alone, stayed green. The parity gate scrapes the ARRAY, never this function.
describe("isLocalRuntime", () => {
  it("recognises every provider in the list", () => {
    for (const p of LOCAL_RUNTIME_PROVIDERS) expect(isLocalRuntime(p)).toBe(true);
    expect(LOCAL_RUNTIME_PROVIDERS.length).toBe(9);
  });

  it("names the nine literally, so a dropped member fails here and not only in a scrape", () => {
    for (const p of ["litellm", "llama.cpp", "llamacpp", "lm-studio", "lmstudio", "ollama", "osaurus", "sglang", "vllm"]) {
      expect(isLocalRuntime(p)).toBe(true);
    }
  });

  it("is exact-match — a hosted provider or a near miss buys no exemption", () => {
    for (const p of ["", "anthropic", "openai", "fireworks", "Ollama", "VLLM", "vllm2", "xvllm", "lm-studio ", "llama.cp", "llama.cppx", OPENAI_COMPATIBLE_PROVIDER]) {
      expect(isLocalRuntime(p)).toBe(false);
    }
  });
});
