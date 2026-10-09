import { describe, expect, it } from "vitest";
import { usdPerMtokToNanos } from "@/lib/api/admin-model-library-types";
import { EMPTY_VALUE, formatContextTokens, formatRatesPerMtok, modelLabel } from "./display";

describe("model display", () => {
  it("names a provider id the way its provider markets the model", () => {
    // Pin test: the literals are the names a reader sees.
    const cases: [string, string][] = [
      ["claude-fable-5", "Fable 5"],
      ["claude-fable-5-1", "Fable 5.1"],
      ["claude-opus-5-5", "Opus 5.5"],
      ["claude-haiku-4-5", "Haiku 4.5"],
      ["anthropic/claude-sonnet-5.5", "Sonnet 5.5"],
      ["accounts/fireworks/models/glm-5p3-flash", "GLM 5.3 Flash"],
      ["accounts/fireworks/models/kimi-k3", "Kimi K3"],
      ["accounts/fireworks/models/deepseek-v4p1-flash", "DeepSeek V4.1 Flash"],
      ["gpt-6.1-sol", "GPT-6.1 Sol"],
      ["gpt-6-astra", "GPT-6 Astra"],
      ["openai/gpt-oss-120b", "GPT OSS 120B"],
      ["deepseek-ai/DeepSeek-V3.2-Exp", "DeepSeek V3.2 Exp"],
      ["MiniMax-M3", "MiniMax M3"],
      ["qwen/qwen3.8-max", "Qwen3.8 Max"],
      ["deepseek-flash", "DeepSeek Flash"],
    ];
    for (const [id, name] of cases) expect(modelLabel(id), id).toBe(name);
  });

  it("keeps an id with no word structure as its last segment", () => {
    expect(modelLabel("syn:large:text")).toBe("syn:large:text");
    expect(modelLabel("sonar")).toBe("sonar");
  });

  it("formats context and rates the way the model library prints them", () => {
    // Pin test: the literals are the library's rendered cells.
    expect(formatContextTokens(1_048_576)).toBe("1,048,576");
    expect(formatContextTokens(200_000)).toBe("200,000");
    expect(formatContextTokens(512)).toBe("512");
    expect(formatContextTokens(undefined)).toBe(EMPTY_VALUE);
    expect(
      formatRatesPerMtok({
        input_nanos_per_mtok: usdPerMtokToNanos(0.15),
        cached_input_nanos_per_mtok: usdPerMtokToNanos(0.03),
        output_nanos_per_mtok: usdPerMtokToNanos(0.5),
      }),
    ).toBe("0.15 / 0.03 / 0.50");
    expect(formatRatesPerMtok(null)).toBe(EMPTY_VALUE);
  });
});
