// The factory exists to stop a double disagreeing with the test that built it.
// These assert the two properties that failure needed: the register follows
// what the test asked for, and a test's own override wins over the default.

import { describe, expect, test } from "bun:test";
import { Effect } from "effect";

import { OUTPUT_FORMAT } from "../src/services/output.ts";
import { outputDouble } from "./helpers-output-double.ts";

describe("outputDouble — the register follows the test's intent", () => {
  test("jsonMode true produces the json register", () => {
    expect(outputDouble({ jsonMode: true }).format).toBe(OUTPUT_FORMAT.json);
  });

  test("jsonMode false produces the text register", () => {
    expect(outputDouble({ jsonMode: false }).format).toBe(OUTPUT_FORMAT.text);
  });

  // The failure this factory exists for: a double that said `text` while its
  // test was about json mode, and passed for thirty-three tests.
  test("a test cannot ask for json and silently receive text", () => {
    expect(outputDouble({ jsonMode: true }).format).not.toBe(OUTPUT_FORMAT.text);
  });

  test("an explicit format wins, for a helper whose caller varies it", () => {
    expect(outputDouble({ jsonMode: true, format: OUTPUT_FORMAT.text }).format)
      .toBe(OUTPUT_FORMAT.text);
  });
});

describe("outputDouble — defaults and overrides", () => {
  test("a recorder is not a terminal unless the test says so", () => {
    expect(outputDouble().stdoutIsTty).toBe(false);
    expect(outputDouble({ stdoutIsTty: true }).stdoutIsTty).toBe(true);
  });

  test("every method of the service is present", async () => {
    const double = outputDouble();
    for (const key of [
      "intro", "info", "success", "warn", "error", "outro",
      "printJson", "printJsonErr", "printKeyValue", "printSection", "printTable",
      "printEntityTable",
    ] as const) {
      expect(typeof double[key]).toBe("function");
    }
  });

  test("a spread override replaces the default, and the rest survive", async () => {
    const seen: string[] = [];
    const double = {
      ...outputDouble(),
      info: (msg: string) => Effect.sync(() => { seen.push(msg); }),
    };
    await Effect.runPromise(double.info("recorded"));
    expect(seen).toEqual(["recorded"]);
    // Untouched methods still answer rather than being undefined.
    await Effect.runPromise(double.warn("ignored"));
  });
});
