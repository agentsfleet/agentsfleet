// Dispatcher tests. The dispatcher provides MainLayer at the boundary,
// so a black-box test (no layer override) exercises every internal
// path: success Exit → 0, typed-failure Exit → variant exit code +
// renderered error to stderr, die Exit → UnexpectedError formatting.
//
// The Analytics service inside MainLayer wraps the real posthog client
// loader; ZOMBIE_POSTHOG_KEY="" + DISABLE_TELEMETRY=true (the default)
// causes createCliAnalytics to return null, so analytics.capture is a
// silent no-op. That keeps these tests offline and deterministic.

import { describe, test, expect, beforeEach, afterEach, beforeAll } from "bun:test";
import { Effect } from "effect";
import { runEffect } from "../src/lib/run-effect.ts";
import {
  AuthError,
  ConfigError,
  NetworkError,
  ServerError,
  UnexpectedError,
  ValidationError,
} from "../src/errors/index.ts";

interface CapturedStream {
  readonly write: (chunk: string) => boolean;
  readonly buf: string[];
}

const captureStream = (target: NodeJS.WritableStream): CapturedStream => {
  const buf: string[] = [];
  const original = target.write.bind(target);
  target.write = ((chunk: string | Uint8Array) => {
    buf.push(typeof chunk === "string" ? chunk : Buffer.from(chunk).toString());
    return true;
  }) as typeof target.write;
  return {
    buf,
    write: (chunk) => original(chunk),
  };
};

let stderrCapture: CapturedStream = { buf: [], write: () => true };
let stdoutCapture: CapturedStream = { buf: [], write: () => true };
let originalStderrWrite: typeof process.stderr.write = process.stderr.write.bind(process.stderr);
let originalStdoutWrite: typeof process.stdout.write = process.stdout.write.bind(process.stdout);

beforeAll(() => {
  // Telemetry off across the suite so MainLayer's posthog loader stays a
  // no-op. Tests never hit the network.
  process.env.DISABLE_TELEMETRY = "true";
});

beforeEach(() => {
  originalStderrWrite = process.stderr.write.bind(process.stderr);
  originalStdoutWrite = process.stdout.write.bind(process.stdout);
  stderrCapture = captureStream(process.stderr);
  stdoutCapture = captureStream(process.stdout);
});

afterEach(() => {
  process.stderr.write = originalStderrWrite as typeof process.stderr.write;
  process.stdout.write = originalStdoutWrite as typeof process.stdout.write;
});

describe("runEffect — success path", () => {
  test("returns exit code 0 when Effect succeeds", async () => {
    const exit = await runEffect({
      name: "test.noop",
      effect: Effect.void,
    });
    expect(exit).toBe(0);
  });
  test("threads telemetry session/device ids through", async () => {
    const exit = await runEffect({
      name: "test.with-telemetry",
      effect: Effect.void,
      telemetry: { sessionId: "sess-123", deviceId: "dev-456" },
    });
    expect(exit).toBe(0);
  });
});

describe("runEffect — typed failure exit codes", () => {
  test("AuthError → exit 1, rendered to stderr", async () => {
    const exit = await runEffect({
      name: "test.auth-fail",
      effect: Effect.fail(
        new AuthError({ detail: "unauthorized", suggestion: "login", code: "UZ-AUTH-002" }),
      ),
    });
    expect(exit).toBe(1);
    const stderr = stderrCapture.buf.join("");
    expect(stderr).toContain("unauthorized");
    expect(stderr).toContain("Suggestion: login");
  });
  test("NetworkError → exit 2", async () => {
    const exit = await runEffect({
      name: "test.net-fail",
      effect: Effect.fail(
        new NetworkError({ detail: "dns", suggestion: "check network", url: "https://x" }),
      ),
    });
    expect(exit).toBe(2);
  });
  test("ServerError → exit 3", async () => {
    const exit = await runEffect({
      name: "test.srv-fail",
      effect: Effect.fail(
        new ServerError({
          detail: "500",
          suggestion: "retry",
          code: "UZ-INTERNAL-001",
          status: 500,
          requestId: "req-x",
        }),
      ),
    });
    expect(exit).toBe(3);
  });
  test("ValidationError → exit 4", async () => {
    const exit = await runEffect({
      name: "test.val-fail",
      effect: Effect.fail(
        new ValidationError({ detail: "bad arg", suggestion: "fix it" }),
      ),
    });
    expect(exit).toBe(4);
  });
  test("ConfigError → exit 5", async () => {
    const exit = await runEffect({
      name: "test.cfg-fail",
      effect: Effect.fail(
        new ConfigError({ detail: "missing", suggestion: "set ZOMBIE_API_URL" }),
      ),
    });
    expect(exit).toBe(5);
  });
  test("UnexpectedError → exit 1", async () => {
    const exit = await runEffect({
      name: "test.unexp-fail",
      effect: Effect.fail(
        new UnexpectedError({ detail: "boom", suggestion: "report" }),
      ),
    });
    expect(exit).toBe(1);
  });
});

describe("runEffect — die path", () => {
  test("uncaught throw inside Effect.sync routes to UnexpectedError + exit 1", async () => {
    const exit = await runEffect({
      name: "test.die",
      effect: Effect.sync(() => {
        throw new Error("kapow");
      }),
    });
    expect(exit).toBe(1);
    const stderr = stderrCapture.buf.join("");
    expect(stderr.toLowerCase()).toContain("kapow");
  });
});

// Silence "stdout captured but never asserted" lint hint — the capture
// is symmetrical for future commits that exercise success-mode stdout.
void stdoutCapture;
