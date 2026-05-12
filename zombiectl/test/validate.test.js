import { describe, test, expect } from "bun:test";
import { isValidId, validateRequiredId } from "../src/program/validate.js";

// Sample valid uuidv7 — backend's allocUuidV7 emits this shape; CLI
// validator must accept v7 (and ONLY v7, post-RULE-NLG swap).
const VALID_UUIDV7 = "0192a3b4-c5d6-7e8f-9012-345678901234";
// Sample uuidv4 — accepted by the old permissive regex; rejected after
// the uuidv7 tightening because version nibble (4th group, first char) is "4".
const UUIDV4_STRING = "550e8400-e29b-41d4-a716-446655440000";

describe("isValidId", () => {
  test("valid uuidv7 passes", () => {
    expect(isValidId(VALID_UUIDV7)).toBe(true);
  });

  test("uppercase uuidv7 passes", () => {
    expect(isValidId(VALID_UUIDV7.toUpperCase())).toBe(true);
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

  // Post-tightening: only uuidv7 is accepted. uuidv4 / arbitrary
  // alphanumeric strings / IDs with dashes used to pass via the old
  // permissive regex; they are now rejected. Backend's allocUuidV7
  // is the single source of truth — the CLI mirrors it.
  test("uuidv4 fails (only uuidv7 is accepted)", () => {
    expect(isValidId(UUIDV4_STRING)).toBe(false);
  });

  test("alphanumeric non-UUID fails", () => {
    expect(isValidId("ws_123456789abc")).toBe(false);
  });

  test("dashed non-UUID fails", () => {
    expect(isValidId("my-workspace-id")).toBe(false);
  });

  test("special characters (spaces) fail", () => {
    expect(isValidId("has spaces")).toBe(false);
  });

  test("special characters (dots) fail", () => {
    expect(isValidId("has.dots")).toBe(false);
  });

  test("malformed uuid fails", () => {
    expect(isValidId("0192a3b4-c5d6-7e8f-9012-34567890123")).toBe(false);
  });
});

describe("validateRequiredId", () => {
  test("valid uuidv7 returns ok", () => {
    const result = validateRequiredId(VALID_UUIDV7, "workspace_id");
    expect(result.ok).toBe(true);
  });

  test("empty string returns error with name", () => {
    const result = validateRequiredId("", "workspace_id");
    expect(result.ok).toBe(false);
    expect(result.message).toContain("workspace_id");
  });

  test("invalid format returns helpful message naming uuidv7", () => {
    const result = validateRequiredId("!@#", "run_id");
    expect(result.ok).toBe(false);
    expect(result.message).toContain("run_id");
    expect(result.message.toLowerCase()).toContain("uuidv7");
  });

  test("uuidv4 returns error (rejected post-tightening)", () => {
    const result = validateRequiredId(UUIDV4_STRING, "workspace_id");
    expect(result.ok).toBe(false);
  });
});
