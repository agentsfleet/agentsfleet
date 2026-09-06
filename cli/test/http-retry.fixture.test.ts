// The parity table both runtimes replay. The dashboard suite
// (ui/packages/app/lib/api/retry.fixture.test.ts) reads the same file; a case
// that passes there and fails here is a disagreement between the two
// policies, which is what the table exists to catch.

import { describe, expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import path from "node:path";
import { PRE_SEND_CODES, apiRequestWithRetry } from "../src/lib/http-retry.ts";
import { asFetchImpl, type ResponseLike } from "./helpers.ts";

const MS_PER_SECOND = 1000;

const FIXTURE_PATH = path.resolve(import.meta.dir, "..", "..", "samples", "fixtures", "retry-policy", "cases.json");
const URL = "https://api.example.test/v1/thing";
const OK_BODY = '{"ok":true}';
const ERROR_TIMEOUT = "timeout";
const NO_SLEEP = async (): Promise<void> => undefined;

interface Answer { readonly status: number; readonly retryAfterSeconds?: number }
interface Failure { readonly error: string }
type Step = Answer | Failure;
interface Case { readonly name: string; readonly method: string; readonly script: ReadonlyArray<Step>; readonly attempts: number }
interface Fixture { readonly policy: { readonly maxAttempts: number; readonly retryAfterCapSeconds: number }; readonly preSendCodes: ReadonlyArray<string>; readonly cases: ReadonlyArray<Case> }

const fixture = JSON.parse(readFileSync(FIXTURE_PATH, "utf8")) as Fixture;

function isAnswer(step: Step): step is Answer {
  return "status" in step;
}

function answer(step: Answer): ResponseLike {
  const retryAfter = step.retryAfterSeconds === undefined ? null : String(step.retryAfterSeconds);
  return {
    ok: step.status >= 200 && step.status < 300,
    status: step.status,
    statusText: String(step.status),
    headers: { get: (k: string) => (k.toLowerCase() === "retry-after" ? retryAfter : null) },
    text: async () => OK_BODY,
  };
}

/** What this transport's fetch rejects with for each scripted failure. */
function failure(step: Failure): Error {
  if (step.error === ERROR_TIMEOUT) return Object.assign(new Error("aborted"), { name: "AbortError" });
  return new TypeError("fetch failed", { cause: Object.assign(new Error(step.error), { code: step.error }) });
}

function scripted(steps: ReadonlyArray<Step>) {
  let calls = 0;
  const fetchImpl = asFetchImpl(async () => {
    const step = steps[Math.min(calls, steps.length - 1)] as Step;
    calls += 1;
    if (isAnswer(step)) return answer(step);
    throw failure(step);
  });
  return { fetchImpl, calls: () => calls };
}

describe("both runtimes read the same table", () => {
  test("the codes that prove a request never left are the fixture's", () => {
    expect([...PRE_SEND_CODES].sort()).toEqual([...fixture.preSendCodes].sort());
  });
});

describe("both runtimes agree on every fixture case", () => {
  for (const c of fixture.cases) {
    test(c.name, async () => {
      const server = scripted(c.script);
      const settled = await apiRequestWithRetry(URL, {
        method: c.method,
        fetchImpl: server.fetchImpl,
        sleepImpl: NO_SLEEP,
        randomFn: () => 0,
        retry: { maxAttempts: fixture.policy.maxAttempts, retryAfterCapMs: fixture.policy.retryAfterCapSeconds * MS_PER_SECOND },
        env: {},
      }).then(
        () => "answered",
        () => "failed",
      );
      expect(server.calls()).toBe(c.attempts);
      const last = c.script[Math.min(c.attempts, c.script.length) - 1] as Step;
      expect(settled).toBe(isAnswer(last) && last.status < 300 ? "answered" : "failed");
    });
  }
});
