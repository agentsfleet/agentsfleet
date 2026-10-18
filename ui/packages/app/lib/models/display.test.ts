import { describe, expect, it } from "vitest";
import { usdPerMtokToNanos } from "@/lib/api/admin-model-library-types";
import { EMPTY_VALUE, compareModels, formatContextTokens, formatRatesPerMtok, modelLabel } from "./display";

describe("model display", () => {
  it("uses the catalogue label without inferring words from the identifier", () => {
    // pin test: literal is the contract
    expect(modelLabel("accounts/fireworks/models/glm-5p3-flash", "GLM 5.3 Flash")).toBe("GLM 5.3 Flash");
    expect(modelLabel("custom/provider-name:latest")).toBe("custom/provider-name:latest");
  });

  it("sorts catalogue names numerically, with exact identifiers breaking ties", () => {
    const models = [
      { model_id: "a", display_name: "Model 120" },
      { model_id: "z", display_name: "Model 20" },
      { model_id: "b", display_name: "Model 20" },
      { model_id: "Custom" },
    ];
    expect([...models].sort(compareModels).map((model) => model.model_id)).toEqual(["Custom", "b", "z", "a"]);
  });

  it("formats exact context integers and marks absent values", () => {
    // pin test: literal is the contract
    expect(formatContextTokens(1_048_576)).toBe("1,048,576");
    expect(formatContextTokens(200_000)).toBe("200,000");
    expect(formatContextTokens(512)).toBe("512");
    expect(formatContextTokens(undefined)).toBe(EMPTY_VALUE);
    expect(formatRatesPerMtok(null)).toBe(EMPTY_VALUE);
  });

  it.each(["0.006", "0.015", "0.0375", "0.075", "0.125", "0.1875", "0.000000001"])(
    "keeps the exact sub-cent rate %s visible", (rate) => {
      // pin test: literal is the contract
      expect(formatRatesPerMtok({
        input_nanos_per_mtok: usdPerMtokToNanos(0.15),
        cached_input_nanos_per_mtok: usdPerMtokToNanos(Number(rate)),
        output_nanos_per_mtok: usdPerMtokToNanos(0.5),
      })).toBe(`0.15 / ${rate} / 0.50`);
    },
  );
});
