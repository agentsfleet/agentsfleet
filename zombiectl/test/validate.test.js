import { describe, test, expect } from "bun:test";
import { isValidId, validateRequiredId } from "../src/program/validate.js";

describe("isValidId", () => {
  test("valid UUID passes", () => {
    expect(isValidId("550e8400-e29b-41d4-a716-446655440000")).toBe(true);
  });

  test("uppercase UUID passes", () => {
    expect(isValidId("550E8400-E29B-41D4-A716-446655440000")).toBe(true);
  });

  test("empty string fails", () => {
    expect(isValidId("")).toBe(false);
  });

  test("null fails", () => {
    expect(isValidId(null)).toBe(false);
  });

  test("undefined fails", () => {
    expect(isValidId(undefined)).toBe(false);
  });

  test("too short fails (3 chars)", () => {
    expect(isValidId("abc")).toBe(false);
  });

  test("valid non-UUID ID (alphanumeric) passes", () => {
    expect(isValidId("ws_123456789abc")).toBe(true);
  });

  test("valid non-UUID with dashes passes", () => {
    expect(isValidId("my-workspace-id")).toBe(true);
  });

  test("special characters (spaces) fail", () => {
    expect(isValidId("has spaces")).toBe(false);
  });

  test("special characters (dots) fail", () => {
    expect(isValidId("has.dots")).toBe(false);
  });

  test("4-char minimum passes", () => {
    expect(isValidId("abcd")).toBe(true);
  });

  test("128-char maximum passes", () => {
    expect(isValidId("a".repeat(128))).toBe(true);
  });

  test("129-char string fails", () => {
    expect(isValidId("a".repeat(129))).toBe(false);
  });
});

describe("validateRequiredId", () => {
  test("valid UUID returns ok", () => {
    const result = validateRequiredId("550e8400-e29b-41d4-a716-446655440000", "workspace_id");
    expect(result.ok).toBe(true);
  });

  test("empty string returns error with name", () => {
    const result = validateRequiredId("", "workspace_id");
    expect(result.ok).toBe(false);
    expect(result.message).toContain("workspace_id");
  });

  test("invalid format returns helpful message", () => {
    const result = validateRequiredId("!@#", "run_id");
    expect(result.ok).toBe(false);
    expect(result.message).toContain("run_id");
    expect(result.message).toContain("UUID");
  });

  test("valid non-UUID ID returns ok", () => {
    const result = validateRequiredId("ws_123456789abc", "workspace_id");
    expect(result.ok).toBe(true);
  });
});
