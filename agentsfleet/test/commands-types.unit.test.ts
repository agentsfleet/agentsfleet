// Coverage for the narrowing helpers in src/commands/types.ts — pure
// utility functions that were under-tested. Each branch hit.

import { describe, test, expect } from "bun:test";
import {
  readBoolean,
  readNumber,
  readString,
} from "../src/commands/types.ts";
import type { ParsedArgs } from "../src/commands/types.ts";

const options = (record: Record<string, ParsedArgs["options"][string]>): ParsedArgs["options"] =>
  record;

describe("readString", () => {
  test("returns the string when present and non-empty", () => {
    expect(readString(options({ x: "hello" }), "x")).toBe("hello");
  });
  test("returns null for empty string", () => {
    expect(readString(options({ x: "" }), "x")).toBeNull();
  });
  test("returns null for non-string values", () => {
    expect(readString(options({ x: 42 }), "x")).toBeNull();
    expect(readString(options({ x: true }), "x")).toBeNull();
    expect(readString(options({}), "missing")).toBeNull();
  });
});

describe("readBoolean", () => {
  test("returns true for truthy values", () => {
    expect(readBoolean(options({ x: true }), "x")).toBe(true);
    expect(readBoolean(options({ x: "yes" }), "x")).toBe(true);
    expect(readBoolean(options({ x: 1 }), "x")).toBe(true);
  });
  test("returns false for falsy values + missing keys", () => {
    expect(readBoolean(options({ x: false }), "x")).toBe(false);
    expect(readBoolean(options({ x: "" }), "x")).toBe(false);
    expect(readBoolean(options({}), "missing")).toBe(false);
  });
});

describe("readNumber", () => {
  test("returns finite numbers as-is", () => {
    expect(readNumber(options({ x: 42 }), "x")).toBe(42);
    expect(readNumber(options({ x: 0 }), "x")).toBe(0);
  });
  test("parses numeric strings", () => {
    expect(readNumber(options({ x: "100" }), "x")).toBe(100);
  });
  test("returns null for non-numeric strings and missing keys", () => {
    expect(readNumber(options({ x: "abc" }), "x")).toBeNull();
    expect(readNumber(options({ x: "" }), "x")).toBeNull();
    expect(readNumber(options({}), "missing")).toBeNull();
  });
  test("returns null for non-finite numbers", () => {
    expect(readNumber(options({ x: NaN }), "x")).toBeNull();
    expect(readNumber(options({ x: Infinity }), "x")).toBeNull();
  });
});
