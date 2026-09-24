import { describe, expect, test } from "bun:test";
import { gradeLcov } from "../scripts/lcov-grade.mjs";

const block = (source: string, functions: ReadonlyArray<string>, hit: number): string => [
  `SF:${source}`,
  ...functions.map((name, index) => `FN:${index + 1},${name}`),
  ...functions.map((name, index) => `FNDA:${index < hit ? 1 : 0},${name}`),
  `FNF:${functions.length}`,
  `FNH:${hit}`,
  "LF:1",
  "LH:1",
  "end_of_record",
].join("\n");

describe("LCOV function grading", () => {
  test("a mixed named and aggregate run counts the missed aggregate function", () => {
    const raw = `${block("named.ts", ["called"], 1)}\nSF:aggregate.ts\nFNF:1\nFNH:0\nLF:1\nLH:1\nend_of_record`;
    expect(gradeLcov(raw).fn).toBe(50);
  });

  test("a complete named set wins over an inconsistent FNH summary", () => {
    const raw = block("named.ts", ["called"], 1).replace("FNH:1", "FNH:0");
    expect(gradeLcov(raw).fn).toBe(100);
  });

  test("an incomplete named set uses the block totals", () => {
    const raw = block("mixed.ts", ["called"], 1).replace("FNF:1", "FNF:2");
    expect(gradeLcov(raw).fn).toBe(50);
  });

  test("missing function records fail closed", () => {
    expect(() => gradeLcov("SF:none.ts\nLF:1\nLH:1")).toThrow("no function records");
  });

  test("uncovered line records retain their source for the failure report", () => {
    const raw = block("answer.ts", ["called"], 1).replace(
      "end_of_record",
      "DA:2,0\nend_of_record",
    );
    expect(gradeLcov(raw).uncovered).toEqual(["answer.ts:2"]);
  });

  test("impossible hit counts fail instead of producing a coverage percentage", () => {
    const raw = block("answer.ts", ["called"], 1).replace("LH:1", "LH:2");
    expect(() => gradeLcov(raw)).toThrow("hit counts greater than found counts");
  });

  test("nonnumeric and negative totals fail instead of producing NaN coverage", () => {
    const raw = block("answer.ts", ["called"], 1);
    expect(() => gradeLcov(raw.replace("FNF:1", "FNF:xyz"))).toThrow("invalid LCOV count");
    expect(() => gradeLcov(raw.replace("LF:1", "LF:-1"))).toThrow("invalid LCOV count");
    expect(() => gradeLcov(raw.replace("FNH:1", "FNH:"))).toThrow("invalid LCOV count");
  });

  test("summed totals beyond safe integers fail closed", () => {
    const first = block("first.ts", ["called"], 1).replace("LF:1", `LF:${Number.MAX_SAFE_INTEGER}`);
    const second = block("second.ts", ["called"], 1);
    expect(() => gradeLcov(`${first}\n${second}`)).toThrow("count sum exceeded");
  });
});
