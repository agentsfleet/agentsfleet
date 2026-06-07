import { describe, expect, it } from "vitest";
import {
  detectProviderFromKey,
  PROVIDER_KEY_PREFIXES,
} from "../app/(dashboard)/settings/models/lib/detect-provider";

// NOTE: this covers ONLY the paste-to-fill key-format hint. The provider list,
// model list, and defaults are catalogue-driven (model-caps API) and are NOT
// asserted here — there are no static provider/model strings to test.
describe("detectProviderFromKey — paste-to-fill key-format hint", () => {
  it("detects anthropic from sk-ant-", () => {
    expect(detectProviderFromKey("sk-ant-api03-xyz")).toBe("anthropic");
  });
  it("detects fireworks from fw_", () => {
    expect(detectProviderFromKey("fw_LIVE_abcdef")).toBe("fireworks");
  });
  it("prefers the more specific sk-or- (openrouter) over bare sk-", () => {
    expect(detectProviderFromKey("sk-or-v1-deadbeef")).toBe("openrouter");
  });
  it("detects groq from gsk_", () => {
    expect(detectProviderFromKey("gsk_AbC123")).toBe("groq");
  });
  it("falls back to openai for a bare sk- key", () => {
    expect(detectProviderFromKey("sk-proj-AbC123")).toBe("openai");
    expect(detectProviderFromKey("sk-AbC123")).toBe("openai");
  });
  it("returns null for an unknown prefix (caller uses the catalogue picker)", () => {
    expect(detectProviderFromKey("xyz-unknown")).toBeNull();
    expect(detectProviderFromKey("ghp_github_token")).toBeNull();
  });
  it("returns null for empty or whitespace-only input", () => {
    expect(detectProviderFromKey("")).toBeNull();
    expect(detectProviderFromKey("   ")).toBeNull();
  });
  it("trims paste whitespace before matching", () => {
    expect(detectProviderFromKey("  sk-ant-api03-xyz  ")).toBe("anthropic");
  });
  it("is case-sensitive — an upper-cased prefix does not match", () => {
    expect(detectProviderFromKey("SK-ANT-api03-xyz")).toBeNull();
  });
  it("orders sk-ant-/sk-or- before bare sk- so they are not mis-detected", () => {
    const idx = (p: string) => PROVIDER_KEY_PREFIXES.findIndex(([x]) => x === p);
    expect(idx("sk-ant-")).toBeLessThan(idx("sk-"));
    expect(idx("sk-or-")).toBeLessThan(idx("sk-"));
  });
});
