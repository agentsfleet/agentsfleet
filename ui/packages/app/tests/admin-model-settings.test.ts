import { describe, expect, it } from "vitest";
import type { AdminModel } from "@/lib/api/admin_model_library";
import { INFERENCE_API } from "@/lib/api/admin-model-library-types";
import { SETTINGS_DEFAULTS, modelSettings, settingsSchema, valuesFromModel } from "@/app/(dashboard)/admin/models/components/model-settings";

const MODEL: AdminModel = {
  id: "model", provider: "workspace-provider", model_id: "workspace/model-next",
  display_name: "Workspace model", advanced: false,
  // pin test: literal is the contract
  context_cap_tokens: 100000, input_nanos_per_mtok: 500000000,
  cached_input_nanos_per_mtok: 125000000, output_nanos_per_mtok: 2000000000,
  inference: { api: INFERENCE_API.messages, base_url: "https://models.example.com/v1", max_output_tokens: 12345, dialect: "anthropic" },
};
const VALUES = valuesFromModel(MODEL);

describe("persisted model settings", () => {
  it("round trips endpoint, API, model-specific limits, dialect and prices", () => {
    const { id: _id, provider: _provider, model_id: _model, ...settings } = MODEL;
    expect(modelSettings(settingsSchema.parse(VALUES))).toEqual(settings);
  });

  it("leaves unset settings empty until the operator supplies them", () => {
    const values = valuesFromModel({ ...MODEL, inference: undefined });
    expect(values).toMatchObject({ api: INFERENCE_API.chat, base_url: "", dialect: "", max_output_tokens: "" });
    expect(settingsSchema.safeParse(values).success).toBe(false);
    expect(settingsSchema.safeParse(SETTINGS_DEFAULTS).success).toBe(false);
  });

  it("omits a blank dialect and accepts all explicit API formats", () => {
    for (const api of Object.values(INFERENCE_API)) {
      const settings = modelSettings(settingsSchema.parse({ ...VALUES, api, dialect: " " }));
      expect(settings.inference).toEqual({ api, base_url: MODEL.inference!.base_url, max_output_tokens: 12345 });
    }
  });

  it.each(["", "not a URL", "http://models.example.com", "https://user:secret@models.example.com", "https://models.example.com?key=secret", "https://models.example.com#secret"])("rejects an unsafe or malformed endpoint %s without throwing", (base_url) => {
    expect(settingsSchema.safeParse({ ...VALUES, base_url }).success).toBe(false);
  });

  it.each(["", "0", "-1", "1.5", "NaN", "Infinity", "9007199254740992"])("rejects invalid token limits %s", (value) => {
    for (const field of ["context_cap_tokens", "max_output_tokens"]) {
      expect(settingsSchema.safeParse({ ...VALUES, [field]: value }).success).toBe(false);
    }
  });

  it.each(["", "-1", "NaN", "Infinity"])("rejects invalid prices %s", (value) => {
    for (const field of ["input_usd", "cached_usd", "output_usd"]) {
      expect(settingsSchema.safeParse({ ...VALUES, [field]: value }).success).toBe(false);
    }
  });

  it("requires the output ceiling to fit the context and bounds dialect names", () => {
    expect(settingsSchema.safeParse({ ...VALUES, max_output_tokens: "100001" }).success).toBe(false);
    // pin test: literal is the contract
    expect(settingsSchema.safeParse({ ...VALUES, max_output_tokens: "100000" }).success).toBe(true);
    expect(settingsSchema.safeParse({ ...VALUES, dialect: "x".repeat(65) }).success).toBe(false);
    expect(settingsSchema.safeParse({ ...VALUES, api: "unknown" }).success).toBe(false);
  });
});
