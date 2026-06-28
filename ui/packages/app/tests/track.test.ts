import { describe, expect, it, vi } from "vitest";
import { EVENTS } from "@/lib/analytics/events";

// The Models & Keys product-event helpers each forward a fixed shape to the
// fire-and-forget posthog helper and never throw.
const captureProductEvent = vi.hoisted(() => vi.fn());
vi.mock("@/lib/analytics/posthog", () => ({ captureProductEvent }));

import {
  captureModelActivated,
  captureModelChanged,
  captureKeyRotated,
  captureProviderReset,
} from "@/app/(dashboard)/settings/models/lib/track";

describe("captureModelActivated", () => {
  it("emits model_added with provider/mode/model", () => {
    captureModelActivated({ provider: "anthropic", mode: "self_managed", model: "claude-sonnet-4-6" });
    expect(captureProductEvent).toHaveBeenCalledWith(EVENTS.model_added, {
      provider: "anthropic",
      mode: "self_managed",
      model: "claude-sonnet-4-6",
    });
  });
});

describe("captureModelChanged", () => {
  it("emits model_changed with provider/model", () => {
    captureModelChanged({ provider: "anthropic", model: "claude-opus-4-6" });
    expect(captureProductEvent).toHaveBeenCalledWith(EVENTS.model_changed, {
      provider: "anthropic",
      model: "claude-opus-4-6",
    });
  });
});

describe("captureKeyRotated", () => {
  it("emits key_rotated with the provider id only (no secret)", () => {
    captureKeyRotated("anthropic");
    expect(captureProductEvent).toHaveBeenCalledWith(EVENTS.key_rotated, { provider: "anthropic" });
  });
});

describe("captureProviderReset", () => {
  it("emits provider_reset recording the provider left behind", () => {
    captureProviderReset("openai");
    expect(captureProductEvent).toHaveBeenCalledWith(EVENTS.provider_reset, { from_provider: "openai" });
  });
});
