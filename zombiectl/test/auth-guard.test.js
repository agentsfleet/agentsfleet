import { describe, test, expect } from "bun:test";
import { requireAuth, AUTH_FAIL_MESSAGE } from "../src/program/auth-guard.js";

describe("requireAuth", () => {
  test("token present returns ok", () => {
    const result = requireAuth({ token: "header.payload.sig", apiKey: null });
    expect(result.ok).toBe(true);
  });

  test("API key present returns ok", () => {
    const result = requireAuth({ token: null, apiKey: "sk-test-123" });
    expect(result.ok).toBe(true);
  });

  test("both present returns ok", () => {
    const result = requireAuth({ token: "tok", apiKey: "key" });
    expect(result.ok).toBe(true);
  });

  test("neither present returns fail", () => {
    const result = requireAuth({ token: null, apiKey: null });
    expect(result.ok).toBe(false);
  });

  test("empty strings treated as falsy", () => {
    const result = requireAuth({ token: "", apiKey: "" });
    expect(result.ok).toBe(false);
  });

  test("AUTH_FAIL_MESSAGE contains login instruction", () => {
    expect(AUTH_FAIL_MESSAGE).toContain("zombiectl login");
  });
});
