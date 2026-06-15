import { describe, test, expect } from "bun:test";
import { InvalidArgumentError } from "commander";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import {
  parseStringOption,
  parseIntOption,
  parseFloatOption,
  parseIdOption,
  parseEnumOption,
  parsePathOption,
  parseDurationOption,
  parseJsonObjectOption,
} from "../src/program/validators.ts";

const VALID_UUIDV7 = "0192a3b4-c5d6-7e8f-9012-345678901234";
const UUIDV4 = "550e8400-e29b-41d4-a716-446655440000";
const SECONDS_PER_HOUR = 3600 as const;
const ONE_THOUSAND_VALUE = 1000 as const;

describe("parseStringOption", () => {
  test("trims surrounding whitespace and returns string", () => {
    expect(parseStringOption("  hello  ")).toBe("hello");
  });

  test("returns plain string unchanged when no whitespace", () => {
    expect(parseStringOption("agent")).toBe("agent");
  });

  test("empty string throws InvalidArgumentError", () => {
    expect(() => parseStringOption("")).toThrow(InvalidArgumentError);
    expect(() => parseStringOption("")).toThrow("must be a non-empty string");
  });

  test("whitespace-only string throws", () => {
    expect(() => parseStringOption("   ")).toThrow("must be a non-empty string");
  });

  test("non-string input throws", () => {
    expect(() => parseStringOption(42)).toThrow("must be a non-empty string");
  });
});

describe("parseIntOption", () => {
  test("parses positive integer", () => {
    expect(parseIntOption()("42")).toBe(42);
  });

  test("parses negative integer", () => {
    expect(parseIntOption()("-7")).toBe(-7);
  });

  test("trims surrounding whitespace", () => {
    expect(parseIntOption()("  100  ")).toBe(100);
  });

  test("rejects floating point", () => {
    expect(() => parseIntOption()("3.14")).toThrow(InvalidArgumentError);
    expect(() => parseIntOption()("3.14")).toThrow("must be an integer");
  });

  test("rejects integer with trailing garbage (no silent truncate)", () => {
    expect(() => parseIntOption()("42abc")).toThrow("must be an integer");
  });

  test("rejects non-numeric string", () => {
    expect(() => parseIntOption()("nope")).toThrow("must be an integer");
  });

  test("enforces min bound", () => {
    const parse = parseIntOption({ min: 1 });
    expect(parse("1")).toBe(1);
    expect(() => parse("0")).toThrow("must be ≥ 1");
  });

  test("enforces max bound", () => {
    const parse = parseIntOption({ max: 100 });
    expect(parse("100")).toBe(100);
    expect(() => parse("101")).toThrow("must be ≤ 100");
  });

  test("enforces both min and max", () => {
    const parse = parseIntOption({ min: 1, max: SECONDS_PER_HOUR });
    expect(parse("300")).toBe(300);
    expect(() => parse("0")).toThrow("must be ≥ 1");
    expect(() => parse("3601")).toThrow(`must be ≤ ${SECONDS_PER_HOUR}`);
  });
});

describe("parseFloatOption", () => {
  test("parses integer-shaped float", () => {
    expect(parseFloatOption("42")).toBe(42);
  });

  test("parses decimal", () => {
    expect(parseFloatOption("3.14")).toBe(3.14);
  });

  test("parses scientific notation", () => {
    expect(parseFloatOption("1e3")).toBe(ONE_THOUSAND_VALUE);
  });

  test("parses negative decimal", () => {
    expect(parseFloatOption("-0.5")).toBe(-0.5);
  });

  test("rejects non-numeric string", () => {
    expect(() => parseFloatOption("nope")).toThrow(InvalidArgumentError);
    expect(() => parseFloatOption("nope")).toThrow("must be a number");
  });

  test("rejects trailing garbage", () => {
    expect(() => parseFloatOption("3.14xyz")).toThrow("must be a number");
  });
});

describe("parseIdOption", () => {
  test("accepts valid uuidv7", () => {
    expect(parseIdOption(VALID_UUIDV7)).toBe(VALID_UUIDV7);
  });

  test("rejects empty string", () => {
    expect(() => parseIdOption("")).toThrow(InvalidArgumentError);
    expect(() => parseIdOption("")).toThrow("required");
  });

  test("rejects uuidv4 (uuidv7 only)", () => {
    expect(() => parseIdOption(UUIDV4)).toThrow("expected uuidv7 format");
  });

  test("rejects malformed uuid", () => {
    expect(() => parseIdOption("not-a-uuid")).toThrow("expected uuidv7 format");
  });

  test("error message includes example uuidv7", () => {
    try {
      parseIdOption("invalid");
      throw new Error("should have thrown");
    } catch (err) {
      expect(err).toBeInstanceOf(InvalidArgumentError);
      if (err instanceof Error) {
        expect(err.message).toContain("0192a3b4-c5d6-7e8f-9012-345678901234");
      }
    }
  });
});

describe("parseEnumOption", () => {
  test("accepts a value in the allowed set", () => {
    expect(parseEnumOption(["a", "b", "c"])("b")).toBe("b");
  });

  test("rejects a value outside the allowed set with all options listed", () => {
    const parse = parseEnumOption(["dev", "prod"]);
    expect(() => parse("staging")).toThrow(InvalidArgumentError);
    expect(() => parse("staging")).toThrow("must be one of: dev, prod");
  });

  test("constructor rejects empty allowed list", () => {
    expect(() => parseEnumOption([])).toThrow("non-empty allowed array");
  });

  test("constructor rejects non-array allowed", () => {
    expect(() => parseEnumOption("not-array" as unknown as readonly string[])).toThrow("non-empty allowed array");
  });
});

describe("parsePathOption", () => {
  test("resolves relative path to absolute (mustExist=false default)", () => {
    const result = parsePathOption()("./samples");
    expect(path.isAbsolute(result)).toBe(true);
    expect(result.endsWith("samples")).toBe(true);
  });

  test("accepts existing path when mustExist=true", () => {
    const tmpFile = path.join(os.tmpdir(), `validators-test-${Date.now()}.json`);
    fs.writeFileSync(tmpFile, "{}");
    try {
      const result = parsePathOption({ mustExist: true })(tmpFile);
      expect(result).toBe(tmpFile);
    } finally {
      fs.unlinkSync(tmpFile);
    }
  });

  test("rejects missing path when mustExist=true", () => {
    const missing = path.join(os.tmpdir(), `definitely-not-real-${Date.now()}.xyz`);
    expect(() => parsePathOption({ mustExist: true })(missing)).toThrow(InvalidArgumentError);
    expect(() => parsePathOption({ mustExist: true })(missing)).toThrow("path does not exist");
  });

  test("rejects empty string", () => {
    expect(() => parsePathOption()("")).toThrow("required");
  });
});

describe("parseDurationOption", () => {
  test("parses milliseconds suffix", () => {
    expect(parseDurationOption("500ms")).toBe(500);
  });

  test("parses seconds suffix", () => {
    expect(parseDurationOption("10s")).toBe(10_000);
  });

  test("parses minutes suffix", () => {
    expect(parseDurationOption("30m")).toBe(1_800_000);
  });

  test("parses hours suffix", () => {
    expect(parseDurationOption("2h")).toBe(7_200_000);
  });

  test("trims surrounding whitespace", () => {
    expect(parseDurationOption("  30m  ")).toBe(1_800_000);
  });

  test("rejects missing suffix", () => {
    expect(() => parseDurationOption("30")).toThrow(InvalidArgumentError);
    expect(() => parseDurationOption("30")).toThrow("expected a duration like");
  });

  test("rejects unknown suffix", () => {
    expect(() => parseDurationOption("30d")).toThrow("expected a duration like");
  });

  test("rejects zero duration", () => {
    expect(() => parseDurationOption("0s")).toThrow("duration must be positive");
  });

  test("rejects negative-shaped input", () => {
    expect(() => parseDurationOption("-1s")).toThrow("expected a duration like");
  });
});

describe("parseJsonObjectOption", () => {
  test("parses a valid JSON object", () => {
    expect(parseJsonObjectOption()('{"a":1}')).toEqual({ a: 1 });
  });

  test("preserves nested structure", () => {
    expect(parseJsonObjectOption()('{"a":{"b":[1,2]}}')).toEqual({ a: { b: [1, 2] } });
  });

  test("rejects invalid JSON", () => {
    expect(() => parseJsonObjectOption()("{not-json}")).toThrow(InvalidArgumentError);
    expect(() => parseJsonObjectOption()("{not-json}")).toThrow("must be valid JSON");
  });

  test("rejects JSON array (not an object)", () => {
    expect(() => parseJsonObjectOption()("[1,2,3]")).toThrow("must be a JSON object");
  });

  test("rejects JSON primitive (not an object)", () => {
    expect(() => parseJsonObjectOption()('"string"')).toThrow("must be a JSON object");
  });

  test("rejects null (not an object)", () => {
    expect(() => parseJsonObjectOption()("null")).toThrow("must be a JSON object");
  });

  test("enforces default maxBytes (4096)", () => {
    const big = JSON.stringify({ data: "x".repeat(5000) });
    expect(() => parseJsonObjectOption()(big)).toThrow("payload must be ≤ 4096 bytes");
  });

  test("honors custom maxBytes", () => {
    expect(() => parseJsonObjectOption({ maxBytes: 8 })('{"a":1234}')).toThrow("payload must be ≤ 8 bytes");
  });

  test("accepts payload at exactly maxBytes", () => {
    const payload = '{"a":1}'; // 7 bytes
    expect(parseJsonObjectOption({ maxBytes: 7 })(payload)).toEqual({ a: 1 });
  });
});
