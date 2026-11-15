import { describe, expect, it } from "vitest";
import { usdPerMtokToNanos } from "@/lib/api/admin-model-library-types";
import { EMPTY_VALUE, PRICE_UNKNOWN, formatRatePerMtok, formatContextTokens, formatRatesPerMtok, modelLabel } from "./display";

describe("model display", () => {
  it("keeps catalogue names and exact custom identifiers", () => {
    expect(modelLabel("accounts/fireworks/models/glm-5p3-flash", "GLM 5.3 Flash")).toBe("GLM 5.3 Flash");
    expect(modelLabel("custom/provider-name:latest")).toBe("custom/provider-name:latest");
    expect(modelLabel("exact-id", null)).toBe("exact-id");
  });
  it("formats exact context integers and marks absent values", () => {
    expect(formatContextTokens(1_048_576)).toBe("1,048,576");
    expect(formatContextTokens(200_000)).toBe("200,000");
    expect(formatContextTokens(512)).toBe("512");
    expect(formatContextTokens(undefined)).toBe(EMPTY_VALUE);
    expect(formatRatesPerMtok(null)).toBe(PRICE_UNKNOWN);
  });

  it.each(["0.006", "0.015", "0.0375", "0.075", "0.125", "0.1875", "0.000000001"])(
    "keeps the exact sub-cent rate %s visible", (rate) => {
        expect(formatRatesPerMtok({
        input_nanos_per_mtok: usdPerMtokToNanos("0.15"),
        cached_input_nanos_per_mtok: usdPerMtokToNanos(rate),
        output_nanos_per_mtok: usdPerMtokToNanos("0.5"),
      })).toBe(`0.15 / ${rate} / 0.50`);
    },
  );
});

it("keeps unknown and int64 prices exact", () => {
  expect(formatRatePerMtok(null)).toBe(PRICE_UNKNOWN);
  expect(formatRatePerMtok("9223372036854775807")).toBe("9223372036.854775807");
});
