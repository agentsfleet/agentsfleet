import { describe, expect, it } from "vitest";
import {
  isHttpsUrl,
  BASE_URL_NOT_HTTPS,
} from "@/app/(dashboard)/settings/models/lib/custom-endpoint";

// Covers the shared custom-endpoint client validation extracted for the
// consolidated Models & Keys forms. The server re-validates and additionally
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
