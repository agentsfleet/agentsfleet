// Line-coverage backfill for src/errors/auth.ts. The two error factories
// (baseError / reqIdError) and the shared baseMessage helper are reached by
// constructing the 10 Data.TaggedError-derived classes the module exports and
// reading their `message` getter. These assertions exercise the factory
// bodies and the message composition so the module-init line range is
// credited under bun's JSC coverage.

import { describe, expect, test } from "bun:test";
import {
  DecryptError,
  ExpiredSessionError,
  InterruptedError,
  InvalidSessionError,
  MeValidationError,
  RateLimitedError,
  SessionAbortedError,
  SessionConsumedError,
  TimeoutError,
  VerificationFailedError,
} from "../src/errors/auth.ts";

const DETAIL = "session token rejected by /auth/sessions";
const SUGGESTION = "run `zombiectl login` again";
const REQ_ID = "req-abc123";
const SUGGESTION_PREFIX = "Suggestion: ";

// Variants built via reqIdError() — carry an optional requestId field.
type ReqIdCtor = new (fields: {
  detail: string;
  suggestion: string;
  requestId?: string | null;
}) => { _tag: string; message: string; requestId?: string | null };

// Variants built via baseError() — { detail, suggestion } only.
type BaseCtor = new (fields: { detail: string; suggestion: string }) => {
  _tag: string;
  message: string;
};

const reqIdCases: ReadonlyArray<{ tag: string; Ctor: ReqIdCtor }> = [
  { tag: "InvalidSessionError", Ctor: InvalidSessionError },
  { tag: "ExpiredSessionError", Ctor: ExpiredSessionError },
  { tag: "RateLimitedError", Ctor: RateLimitedError },
  { tag: "VerificationFailedError", Ctor: VerificationFailedError },
  { tag: "SessionAbortedError", Ctor: SessionAbortedError },
  { tag: "SessionConsumedError", Ctor: SessionConsumedError },
  { tag: "MeValidationError", Ctor: MeValidationError },
];

const baseCases: ReadonlyArray<{ tag: string; Ctor: BaseCtor }> = [
  { tag: "TimeoutError", Ctor: TimeoutError },
  { tag: "InterruptedError", Ctor: InterruptedError },
  { tag: "DecryptError", Ctor: DecryptError },
];

describe("auth.ts reqIdError factory", () => {
  for (const { tag, Ctor } of reqIdCases) {
    test(`${tag} tags correctly and composes message from detail + suggestion`, () => {
      const err = new Ctor({
        detail: DETAIL,
        suggestion: SUGGESTION,
        requestId: REQ_ID,
      });
      expect(err._tag).toBe(tag);
      // baseMessage(): `${detail}\n  Suggestion: ${suggestion}`
      expect(err.message).toBe(`${DETAIL}\n  ${SUGGESTION_PREFIX}${SUGGESTION}`);
      expect(err.requestId).toBe(REQ_ID);
    });
  }

  test("requestId is optional and absent when omitted", () => {
    const err = new InvalidSessionError({
      detail: DETAIL,
      suggestion: SUGGESTION,
    });
    expect(err.requestId).toBeUndefined();
    expect(err.message).toContain(DETAIL);
  });

  test("explicit null requestId is preserved (re-wrap path)", () => {
    const err = new ExpiredSessionError({
      detail: DETAIL,
      suggestion: SUGGESTION,
      requestId: null,
    });
    expect(err.requestId).toBeNull();
  });
});

describe("auth.ts baseError factory", () => {
  for (const { tag, Ctor } of baseCases) {
    test(`${tag} tags correctly and composes message from detail + suggestion`, () => {
      const err = new Ctor({ detail: DETAIL, suggestion: SUGGESTION });
      expect(err._tag).toBe(tag);
      expect(err.message).toBe(`${DETAIL}\n  ${SUGGESTION_PREFIX}${SUGGESTION}`);
      // baseError variants carry no requestId property at all.
      expect((err as Record<string, unknown>).requestId).toBeUndefined();
    });
  }
});

describe("auth.ts message getter wiring", () => {
  test("differing fields flow through baseMessage independently", () => {
    const a = new TimeoutError({ detail: "first", suggestion: "retry" });
    const b = new DecryptError({ detail: "second", suggestion: "rotate key" });
    expect(a.message).toBe("first\n  Suggestion: retry");
    expect(b.message).toBe("second\n  Suggestion: rotate key");
    expect(a.message).not.toBe(b.message);
  });
});
