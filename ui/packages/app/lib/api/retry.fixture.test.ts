import { readFileSync } from "node:fs";
import path from "node:path";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { fetchFailed } from "@/tests/helpers/fetch-failed";
import { requestWithRetry } from "./client";
import { PRE_SEND_CODES } from "./retry-classify";

// The parity table both runtimes replay. The CLI suite reads the same file
// (cli/test/http-retry.fixture.test.ts); a case that passes here and fails
// there is a disagreement between the two policies, which is what the table
// exists to catch.

// Resolved from this file, so the suite reads the same table wherever vitest is run from.
const FIXTURE_PATH = path.resolve(__dirname, "../../../../../samples/fixtures/retry-policy/cases.json");
const TOKEN = "tok";
const PATH = "/v1/thing";
const MS_PER_SECOND = 1000;
const OK_BODY = { ok: true };
const ERROR_TIMEOUT = "timeout";

type Answer = { status: number; retryAfterSeconds?: number };
type Failure = { error: string };
type Step = Answer | Failure;
type Case = { name: string; method: string; script: Step[]; attempts: number };
type Fixture = { policy: { maxAttempts: number; retryAfterCapSeconds: number }; preSendCodes: string[]; cases: Case[] };

const fixture = JSON.parse(readFileSync(FIXTURE_PATH, "utf8")) as Fixture;

const fetchMock = vi.fn();
vi.stubGlobal("fetch", fetchMock);

beforeEach(() => {
  fetchMock.mockReset();
  vi.stubEnv("AGENTSFLEET_NO_RETRY", "");
});
afterEach(() => {
  vi.unstubAllEnvs();
});

function isAnswer(step: Step): step is Answer {
  return "status" in step;
}

function answer(step: Answer) {
  const retryAfter = step.retryAfterSeconds === undefined ? null : String(step.retryAfterSeconds);
  return {
    ok: step.status >= 200 && step.status < 300,
    status: step.status,
    headers: { get: (k: string) => (k.toLowerCase() === "retry-after" ? retryAfter : null) },
    json: async () => OK_BODY,
  };
}

/** What this transport's fetch rejects with for each scripted failure. */
function failure(step: Failure): Error {
  if (step.error === ERROR_TIMEOUT) return new DOMException("signal timed out", "TimeoutError");
  return fetchFailed(step.error);
}

function script(steps: Step[]): void {
  fetchMock.mockImplementation(() => {
    const index = Math.min(fetchMock.mock.calls.length - 1, steps.length - 1);
    const step = steps[index] as Step;
    return isAnswer(step) ? Promise.resolve(answer(step)) : Promise.reject(failure(step));
  });
}

describe("both runtimes read the same table", () => {
  it("the codes that prove a request never left are the fixture's", () => {
    expect([...PRE_SEND_CODES].sort()).toEqual([...fixture.preSendCodes].sort());
  });
});

describe("both runtimes agree on every fixture case", () => {
  it.each(fixture.cases.map((c) => [c.name, c] as const))("%s", async (_name, c) => {
    script(c.script);
    const settled = await requestWithRetry(PATH, { method: c.method }, TOKEN, {
      maxAttempts: fixture.policy.maxAttempts,
      sleepImpl: async () => undefined,
      retryAfterCapMs: fixture.policy.retryAfterCapSeconds * MS_PER_SECOND,
    }).then(
      () => "answered",
      () => "failed",
    );
    expect(fetchMock).toHaveBeenCalledTimes(c.attempts);
    const last = c.script[Math.min(c.attempts, c.script.length) - 1] as Step;
    expect(settled).toBe(isAnswer(last) && last.status < 300 ? "answered" : "failed");
  });
});
