// Deterministic helper tests for the device-flow login surface. The
// keypair-gen + create-session + poll + verify-with-retry paths that
// require a live Effect runtime + HttpClient mock + Input fake land in
// the dimension batch alongside D20/D22/D24 — this file pins the pure
// functions a downstream change might silently regress: platform-keyed
// token-name defaults, login-URL composition, and the wrong-code 400 →
// AuthError translation.

import { describe, test, expect } from "bun:test";
import {
  buildLoginUrl,
  defaultTokenName,
  mapVerifyFailure,
} from "../src/commands/login-device-flow.ts";
import {
  AuthError,
  NetworkError,
  ServerError,
  ValidationError,
} from "../src/errors/index.ts";
import { AUTH_CODE_VERIFICATION_FAILED } from "../src/lib/auth-error-codes.ts";

describe("defaultTokenName", () => {
  test("maps darwin → macos-cli", () => {
    expect(defaultTokenName("darwin")).toBe("macos-cli");
  });
  test("maps linux → linux-cli", () => {
    expect(defaultTokenName("linux")).toBe("linux-cli");
  });
  test("maps win32 → windows-cli", () => {
    expect(defaultTokenName("win32")).toBe("windows-cli");
  });
  test("maps freebsd → freebsd-cli", () => {
    expect(defaultTokenName("freebsd")).toBe("freebsd-cli");
  });
  test("falls back to generic cli for unknown platforms (no hostname leak)", () => {
    expect(defaultTokenName("openbsd" as NodeJS.Platform)).toBe("cli");
  });
});

describe("buildLoginUrl", () => {
  test("appends /cli-auth/{session_id} to the dashboard URL", () => {
    expect(buildLoginUrl("https://app.usezombie.com", "sess_123")).toBe(
      "https://app.usezombie.com/cli-auth/sess_123",
    );
  });
  test("strips a trailing slash on the dashboard URL", () => {
    expect(buildLoginUrl("https://app.usezombie.com/", "abc")).toBe(
      "https://app.usezombie.com/cli-auth/abc",
    );
  });
  test("URL-encodes the session_id (defense-in-depth even though UUIDv7s don't need it)", () => {
    expect(buildLoginUrl("https://app.usezombie.com", "a/b?c")).toBe(
      "https://app.usezombie.com/cli-auth/a%2Fb%3Fc",
    );
  });
});

describe("mapVerifyFailure", () => {
  test("translates a 400 ServerError to a VerificationFailed AuthError", () => {
    const err = new ServerError({
      detail: "verification failed",
      suggestion: "retry",
      code: "UZ-AUTH-010",
      status: 400,
      requestId: "req_abc",
    });
    const mapped = mapVerifyFailure(err);
    expect(mapped).toBeInstanceOf(AuthError);
    expect((mapped as AuthError).code).toBe(AUTH_CODE_VERIFICATION_FAILED);
    expect((mapped as AuthError).requestId).toBe("req_abc");
  });
  test("leaves non-400 ServerErrors untouched (caller decides)", () => {
    const err = new ServerError({
      detail: "session aborted",
      suggestion: "retry",
      code: "UZ-AUTH-005",
      status: 410,
      requestId: null,
    });
    expect(mapVerifyFailure(err)).toBe(err);
  });
  test("leaves NetworkError untouched", () => {
    const err = new NetworkError({
      detail: "fetch failed",
      suggestion: "check connection",
      url: "https://api.test/v1/auth/sessions/x/verify",
    });
    expect(mapVerifyFailure(err)).toBe(err);
  });
  test("leaves an existing AuthError untouched", () => {
    const err = new AuthError({
      detail: "x",
      suggestion: "y",
      code: "OTHER",
    });
    expect(mapVerifyFailure(err)).toBe(err);
  });
  test("leaves ValidationError untouched", () => {
    const err = new ValidationError({ detail: "bad arg", suggestion: "fix" });
    expect(mapVerifyFailure(err)).toBe(err);
  });
});
