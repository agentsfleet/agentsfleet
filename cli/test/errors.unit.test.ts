import { describe, test, expect } from "bun:test";
import {
  AuthError,
  ConfigError,
  DecryptError,
  EXIT_CODE,
  ExpiredSessionError,
  InterruptedError,
  InvalidSessionError,
  MeValidationError,
  NetworkError,
  RateLimitedError,
  ServerError,
  SessionAbortedError,
  SessionConsumedError,
  TimeoutError,
  UnexpectedError,
  ValidationError,
  VerificationFailedError,
  type CliError,
} from "../src/errors/index.ts";

const detail = "thing broke";
const suggestion = "try again";

describe("CliError variants", () => {
  test("AuthError carries _tag, code, and renders detail + suggestion in message", () => {
    const err = new AuthError({ detail, suggestion, code: "UZ-AUTH-001" });
    expect(err._tag).toBe("AuthError");
    expect(err.code).toBe("UZ-AUTH-001");
    expect(err.message).toContain(detail);
    expect(err.message).toContain(suggestion);
  });
  test("NetworkError carries url + tag", () => {
    const err = new NetworkError({ detail, suggestion, url: "https://x.test" });
    expect(err._tag).toBe("NetworkError");
    expect(err.url).toBe("https://x.test");
    expect(err.message).toContain("Suggestion:");
  });
  test("ServerError carries code/status/requestId", () => {
    const err = new ServerError({
      detail,
      suggestion,
      code: "UZ-AUTH-002",
      status: 401,
      requestId: "req-1",
    });
    expect(err._tag).toBe("ServerError");
    expect(err.status).toBe(401);
    expect(err.requestId).toBe("req-1");
    // Hits ServerError's `override get message()` getter (errors/index.ts
    // lines 48-49) — the existing assertions only read scalar fields.
    expect(err.message).toContain(detail);
    expect(err.message).toContain("Suggestion: " + suggestion);
  });
  test("ValidationError shape", () => {
    const err = new ValidationError({ detail, suggestion });
    expect(err._tag).toBe("ValidationError");
    expect(err.message).toContain(detail);
  });
  test("ConfigError shape", () => {
    const err = new ConfigError({ detail, suggestion });
    expect(err._tag).toBe("ConfigError");
    // Hits ConfigError's message getter (errors/index.ts lines 66-67).
    expect(err.message).toContain(detail);
    expect(err.message).toContain("Suggestion: " + suggestion);
  });
  test("UnexpectedError shape", () => {
    const err = new UnexpectedError({ detail, suggestion });
    expect(err._tag).toBe("UnexpectedError");
    // Hits UnexpectedError's message getter (errors/index.ts lines 75-76).
    expect(err.message).toContain(detail);
    expect(err.message).toContain("Suggestion: " + suggestion);
  });
});

describe("EXIT_CODE map", () => {
  test("every CliError._tag has a numeric exit code", () => {
    const tags: ReadonlyArray<CliError["_tag"]> = [
      "AuthError",
      "NetworkError",
      "ServerError",
      "ValidationError",
      "ConfigError",
      "UnexpectedError",
    ];
    for (const tag of tags) {
      expect(typeof EXIT_CODE[tag]).toBe("number");
      expect(EXIT_CODE[tag]).toBeGreaterThan(0);
    }
  });
  test("AuthError and UnexpectedError both map to 1 (cli convention)", () => {
    expect(EXIT_CODE.AuthError).toBe(1);
    expect(EXIT_CODE.UnexpectedError).toBe(1);
  });
  test("NetworkError, ServerError, ValidationError, ConfigError have distinct codes", () => {
    const codes = new Set([
      EXIT_CODE.NetworkError,
      EXIT_CODE.ServerError,
      EXIT_CODE.ValidationError,
      EXIT_CODE.ConfigError,
    ]);
    expect(codes.size).toBe(4);
  });
  test("auth-flow specialization exit codes are 1, except RateLimitedError=2 and InterruptedError=130", () => {
    expect(EXIT_CODE.InvalidSessionError).toBe(1);
    expect(EXIT_CODE.ExpiredSessionError).toBe(1);
    expect(EXIT_CODE.RateLimitedError).toBe(2);
    expect(EXIT_CODE.TimeoutError).toBe(1);
    expect(EXIT_CODE.InterruptedError).toBe(130);
    expect(EXIT_CODE.VerificationFailedError).toBe(1);
    expect(EXIT_CODE.DecryptError).toBe(1);
    expect(EXIT_CODE.SessionAbortedError).toBe(1);
    expect(EXIT_CODE.SessionConsumedError).toBe(1);
    expect(EXIT_CODE.MeValidationError).toBe(1);
  });
});

// ── Auth-flow tagged specializations ─────────────────────────────────────
// These exercise every `override get message()` getter on the 10
// `Data.TaggedError`-derived classes in `errors/auth.ts`. Without this
// block patch coverage on auth.ts hovers at 44%; with it the message
// getters are exercised and patch coverage clears the 89% gate.

describe("auth-flow tagged error specializations", () => {
  const cases: ReadonlyArray<{
    name: CliError["_tag"];
    build: () => CliError;
    expectRequestId: boolean;
  }> = [
    {
      name: "InvalidSessionError",
      build: () =>
        new InvalidSessionError({ detail, suggestion, requestId: "req-inv" }),
      expectRequestId: true,
    },
    {
      name: "ExpiredSessionError",
      build: () =>
        new ExpiredSessionError({ detail, suggestion, requestId: "req-exp" }),
      expectRequestId: true,
    },
    {
      name: "RateLimitedError",
      build: () =>
        new RateLimitedError({ detail, suggestion, requestId: "req-rl" }),
      expectRequestId: true,
    },
    {
      name: "TimeoutError",
      build: () => new TimeoutError({ detail, suggestion }),
      expectRequestId: false,
    },
    {
      name: "InterruptedError",
      build: () => new InterruptedError({ detail, suggestion }),
      expectRequestId: false,
    },
    {
      name: "VerificationFailedError",
      build: () =>
        new VerificationFailedError({ detail, suggestion, requestId: "req-vf" }),
      expectRequestId: true,
    },
    {
      name: "DecryptError",
      build: () => new DecryptError({ detail, suggestion }),
      expectRequestId: false,
    },
    {
      name: "SessionAbortedError",
      build: () =>
        new SessionAbortedError({ detail, suggestion, requestId: "req-sa" }),
      expectRequestId: true,
    },
    {
      name: "SessionConsumedError",
      build: () =>
        new SessionConsumedError({ detail, suggestion, requestId: "req-sc" }),
      expectRequestId: true,
    },
    {
      name: "MeValidationError",
      build: () =>
        new MeValidationError({ detail, suggestion, requestId: "req-me" }),
      expectRequestId: true,
    },
  ];

  for (const { name, build, expectRequestId } of cases) {
    test(`${name} carries _tag, renders detail+suggestion via message getter`, () => {
      const err = build();
      expect(err._tag).toBe(name);
      // Hits the `override get message()` getter — the line range that
      // codecov flagged uncovered.
      expect(err.message).toContain(detail);
      expect(err.message).toContain("Suggestion: " + suggestion);
      if (expectRequestId) {
        // requestId-carrying variants surface the id on the instance for
        // the dispatcher's `request_id:` rendering.
        expect((err as { requestId?: string | null }).requestId).toBeTruthy();
      }
    });
  }

  test("omitting requestId on WithRequestId variants leaves it undefined", () => {
    const err = new InvalidSessionError({ detail, suggestion });
    expect(err.message).toContain(detail);
    expect((err as { requestId?: string | null }).requestId).toBeFalsy();
  });

  test("explicit null requestId is accepted (re-wrapping path)", () => {
    const err = new ExpiredSessionError({ detail, suggestion, requestId: null });
    expect((err as { requestId?: string | null }).requestId).toBeNull();
  });
});
