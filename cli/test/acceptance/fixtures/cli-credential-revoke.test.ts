import { afterEach, beforeEach, describe, expect, it } from "bun:test";

import {
  CliCredentialRevokeFailed,
  MAX_REVOKE_ATTEMPTS,
  revokeCliCredential,
} from "./cli-credential-revoke.ts";

const API_URL = "https://api.test";
const CLI_CREDENTIAL_ID = "01989abc-def0-7123-8abc-def012345678";
const CLI_CREDENTIAL = `afc_${"a".repeat(64)}`;
const MINTED = { id: CLI_CREDENTIAL_ID, credential: CLI_CREDENTIAL };
const ANSWER_DETAIL = "temporary failure";
const RETRY_AFTER_HEADER = "Retry-After"; // pin test: literal is the contract
const TIMEOUT_ERROR_NAME = "TimeoutError"; // pin test: literal is the contract
const HTTP_NO_CONTENT = 204;
const HTTP_UNAUTHORIZED = 401;
const HTTP_NOT_FOUND = 404;
const HTTP_REQUEST_TIMEOUT = 408;
const HTTP_TOO_EARLY = 425;
const HTTP_TOO_MANY_REQUESTS = 429;
const HTTP_INTERNAL_SERVER_ERROR = 500;
const HTTP_BAD_GATEWAY = 502;
const HTTP_SERVICE_UNAVAILABLE = 503;
const HTTP_GATEWAY_TIMEOUT = 504;
/** `random` is centred on 0.5: the schedule with no jitter applied. */
const NO_JITTER = (): number => 0.5;
const FULL_JITTER_UP = (): number => 1;
const FULL_JITTER_DOWN = (): number => 0;

type Answer = number | Error | Response;

interface Probe {
  readonly attempts: () => number;
  readonly signals: ReadonlyArray<AbortSignal | null | undefined>;
}

interface Clock {
  readonly sleep: (ms: number) => Promise<void>;
  readonly delays: ReadonlyArray<number>;
}

let originalFetch: typeof globalThis.fetch;

function answerResponse(status: number): Response {
  return status === HTTP_NO_CONTENT
    ? new Response(null, { status })
    : new Response(ANSWER_DETAIL, { status });
}

function retryAfterAnswer(status: number, seconds: string): Response {
  return new Response(ANSWER_DETAIL, { status, headers: { [RETRY_AFTER_HEADER]: seconds } });
}

/** Answers each DELETE from `answers` in order, the last one repeating. */
function installAnswers(answers: ReadonlyArray<Answer>): Probe {
  let attempts = 0;
  const signals: Array<AbortSignal | null | undefined> = [];
  globalThis.fetch = Object.assign(
    async (_input: string | URL | Request, init?: RequestInit): Promise<Response> => {
      const answer = answers[Math.min(attempts, answers.length - 1)];
      attempts += 1;
      signals.push(init?.signal);
      if (answer === undefined) throw new Error("no answer scripted");
      if (answer instanceof Error) throw answer;
      return typeof answer === "number" ? answerResponse(answer) : answer;
    },
    { preconnect: originalFetch.preconnect },
  );
  return { attempts: () => attempts, signals };
}

function recordingClock(): Clock {
  const delays: number[] = [];
  return {
    delays,
    sleep: async (ms: number): Promise<void> => {
      delays.push(ms);
    },
  };
}

async function revoke(clock: Clock, random: () => number = NO_JITTER): Promise<void> {
  await revokeCliCredential(API_URL, MINTED, { sleep: clock.sleep, random });
}

async function revokeFailure(clock: Clock): Promise<CliCredentialRevokeFailed> {
  try {
    await revoke(clock);
  } catch (error: unknown) {
    if (error instanceof CliCredentialRevokeFailed) return error;
    throw error;
  }
  throw new Error("revoke resolved");
}

beforeEach(() => {
  originalFetch = globalThis.fetch;
});

afterEach(() => {
  globalThis.fetch = originalFetch;
});

describe("revoking a CLI credential the API cannot answer for yet", () => {
  it("retries a transient 503 with doubling backoff", async () => {
    const probe = installAnswers([HTTP_SERVICE_UNAVAILABLE, HTTP_SERVICE_UNAVAILABLE, HTTP_NO_CONTENT]);
    const clock = recordingClock();

    await revoke(clock);

    expect(probe.attempts()).toBe(3);
    expect(clock.delays).toEqual([500, 1000]); // pin test: literal is the contract
  });

  it("gives up at the attempt cap, naming the id and every attempt, never the secret", async () => {
    const probe = installAnswers([HTTP_SERVICE_UNAVAILABLE]);
    const clock = recordingClock();

    const failure = await revokeFailure(clock);

    expect(probe.attempts()).toBe(MAX_REVOKE_ATTEMPTS);
    expect(clock.delays).toEqual([500, 1000, 2000, 4000]); // pin test: literal is the contract
    expect(failure.name).toBe("CliCredentialRevokeFailed"); // pin test: literal is the contract
    expect(failure.message).toContain(`answered ${HTTP_SERVICE_UNAVAILABLE}: ${ANSWER_DETAIL}`);
    expect(failure.message).toContain(`gave up after ${MAX_REVOKE_ATTEMPTS} attempts`);
    expect(failure.message).toContain(CLI_CREDENTIAL_ID);
    expect(failure.message).not.toContain(CLI_CREDENTIAL);
    expect(failure.retryable).toBe(true);
    expect(failure.cause).toBeInstanceOf(AggregateError);
    expect((failure.cause as AggregateError).errors).toHaveLength(MAX_REVOKE_ATTEMPTS);
  });

  it.each([
    HTTP_REQUEST_TIMEOUT,
    HTTP_TOO_EARLY,
    HTTP_TOO_MANY_REQUESTS,
    HTTP_BAD_GATEWAY,
    HTTP_GATEWAY_TIMEOUT,
  ])("retries %d once the API answers", async (status) => {
    const probe = installAnswers([status, HTTP_NO_CONTENT]);
    const clock = recordingClock();

    await revoke(clock);

    expect(probe.attempts()).toBe(2);
    expect(clock.delays).toEqual([500]); // pin test: literal is the contract
  });

  it("does not retry an answer the API decided", async () => {
    const probe = installAnswers([HTTP_INTERNAL_SERVER_ERROR]);
    const clock = recordingClock();

    const failure = await revokeFailure(clock);

    expect(probe.attempts()).toBe(1);
    expect(clock.delays).toEqual([]);
    expect(failure.retryable).toBe(false);
    expect(failure.message).toContain(`answered ${HTTP_INTERNAL_SERVER_ERROR}`);
    expect(failure.cause).toBeUndefined();
  });

  it.each([HTTP_UNAUTHORIZED, HTTP_NOT_FOUND])("treats %d as a row that is already gone", async (status) => {
    const probe = installAnswers([status]);
    const clock = recordingClock();

    await revoke(clock);

    expect(probe.attempts()).toBe(1);
    expect(clock.delays).toEqual([]);
  });

  it("treats a lost answer followed by a refused bearer as revoked", async () => {
    const probe = installAnswers([new DOMException("timed out", TIMEOUT_ERROR_NAME), HTTP_UNAUTHORIZED]);
    const clock = recordingClock();

    await revoke(clock);

    expect(probe.attempts()).toBe(2);
    expect(clock.delays).toEqual([500]); // pin test: literal is the contract
  });

  it("retries a connection that failed", async () => {
    const probe = installAnswers([new TypeError("Unable to connect"), HTTP_NO_CONTENT]);
    const clock = recordingClock();

    await revoke(clock);

    expect(probe.attempts()).toBe(2);
  });

  it("does not retry a fault that is not the transport's", async () => {
    const cause = new Error("fetch double misconfigured");
    const probe = installAnswers([cause]);
    const clock = recordingClock();

    const failure = await revokeFailure(clock);

    expect(probe.attempts()).toBe(1);
    expect(failure.retryable).toBe(false);
    expect(failure.message).toContain("never reached the API");
    expect(failure.cause).toBe(cause);
  });

  it("honours Retry-After up to its cap", async () => {
    installAnswers([
      retryAfterAnswer(HTTP_SERVICE_UNAVAILABLE, "2"),
      retryAfterAnswer(HTTP_TOO_MANY_REQUESTS, "60"),
      retryAfterAnswer(HTTP_SERVICE_UNAVAILABLE, "not-a-number"),
      HTTP_NO_CONTENT,
    ]);
    const clock = recordingClock();

    await revoke(clock);

    expect(clock.delays).toEqual([2000, 10_000, 2000]); // pin test: literal is the contract
  });

  it("jitters the schedule by a fifth either way", async () => {
    installAnswers([HTTP_SERVICE_UNAVAILABLE, HTTP_NO_CONTENT]);
    const up = recordingClock();
    await revoke(up, FULL_JITTER_UP);
    installAnswers([HTTP_SERVICE_UNAVAILABLE, HTTP_NO_CONTENT]);
    const down = recordingClock();
    await revoke(down, FULL_JITTER_DOWN);

    expect(up.delays).toEqual([600]); // pin test: literal is the contract
    expect(down.delays).toEqual([400]); // pin test: literal is the contract
  });

  it("bounds every attempt with an abort signal", async () => {
    const probe = installAnswers([HTTP_SERVICE_UNAVAILABLE, HTTP_NO_CONTENT]);

    await revoke(recordingClock());

    expect(probe.signals).toHaveLength(2);
    for (const signal of probe.signals) expect(signal).toBeInstanceOf(AbortSignal);
  });

  it("lets a rejecting sleep propagate untouched", async () => {
    const probe = installAnswers([HTTP_SERVICE_UNAVAILABLE]);
    const clockFault = new Error("clock");

    await expect(revokeCliCredential(API_URL, MINTED, {
      sleep: async () => { throw clockFault; },
      random: NO_JITTER,
    })).rejects.toBe(clockFault);
    expect(probe.attempts()).toBe(1);
  });
});
