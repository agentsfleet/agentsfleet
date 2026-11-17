import { describe, expect, it } from "vitest";
import { NO_MODEL_LABELS, modelLabel, modelLabelsOf } from "./model-labels";

const model = (id: string, display_name: string) => ({ id, display_name });

describe("shared model labels", () => {
  it("labels an identifier only when every provider listing it agrees on a name", () => {
    expect(modelLabelsOf([
      model("known", "Known"), model("shared", "Shared"), model("shared", "Shared"),
      model("disputed", "Alpha"), model("disputed", "Beta"), model("disputed", "Alpha"),
      model("blank", " "), model("late-blank", "Named"), model("late-blank", ""),
    ])).toEqual({ known: "Known", shared: "Shared" });
    expect(modelLabelsOf([])).toEqual({});
  });

  it("keeps prototype-named identifiers as ordinary own keys", () => {
    const labels = modelLabelsOf([model("constructor", "A model named constructor"), model("__proto__", "Proto model")]);
    expect(modelLabel(labels, "constructor")).toBe("A model named constructor");
    expect(modelLabel(labels, "__proto__")).toBe("Proto model");
    expect(Object.getPrototypeOf(labels)).toBe(Object.prototype);
  });

  it("unknown_model_shows_recorded_identifier: never a blank or inherited value", () => {
    const labels = { "known-model": "Known model" };
    expect(modelLabel(labels, "known-model")).toBe("Known model");
    for (const recorded of ["retired-model", "constructor", "toString", "__proto__", "hasOwnProperty"]) {
      expect(modelLabel(labels, recorded)).toBe(recorded);
    }
    expect(modelLabel(NO_MODEL_LABELS, "known-model")).toBe("known-model");
  });
});
