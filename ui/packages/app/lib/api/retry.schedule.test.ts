import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { fetchFailed } from "@/tests/helpers/fetch-failed";
import { ApiError, RequestCancelledError } from "./errors";
import {
  RETRY_CODE_CANCELLED,
  RETRY_DEFAULTS,
  runWithRetry,
  type AttemptInfo,
  type RetryInfo,
} from "./retry";

// The schedule's own guarantees, proved over a bare thunk: the deadline, the
// Retry-After cap, full jitter, provenance-gated replay, interruption, and
// truthful telemetry. The loop cases the previous policy proved stay in
// retry.test.ts; the transport's use of the seam is in client.retry.test.ts.

const TRANSIENT_STATUS = 503;
const RATE_LIMITED_STATUS = 429;
const TOO_EARLY_STATUS = 425;
const TIMEOUT_STATUS = 408;
const NO_CONTENT_STATUS = 204;
const MS_PER_SECOND = 1000;
const LONG_RETRY_AFTER_MS = 120 * MS_PER_SECOND;
const SHORT_RETRY_AFTER_MS = 5 * MS_PER_SECOND;
const ATTEMPT_MS = 400;
const DEADLINE_MS = 1 * MS_PER_SECOND;
const BASE_DELAY_MS = 100;
const WIDE_CAP_MS = 10_000;
const BARE_DELAY = () => 1;
const NO_DELAY = () => 0;
const CONFIG_INVALID = "CONFIG_INVALID";

function transient(status = TRANSIENT_STATUS, retryAfterMs: number | null = null): ApiError {
  return new ApiError("svc", status, "X", undefined, retryAfterMs);
}


function recordingSleep() {
  const delays: number[] = [];
  const sleepImpl = async (ms: number) => {
    delays.push(ms);
  };
  return { delays, sleepImpl };
}

/** A small deterministic generator, so a jitter assertion is repeatable. */
function seededRandom(seed: number): () => number {
  let state = seed;
  return () => {
    state = (state * 1_664_525 + 1_013_904_223) % 4_294_967_296;
    return state / 4_294_967_296;
  };
}

beforeEach(() => {
  vi.stubEnv("AGENTSFLEET_NO_RETRY", "");
});
afterEach(() => {
  vi.unstubAllEnvs();
  vi.restoreAllMocks();
});

describe("the deadline and the cap", () => {
  it("a read gives up at the deadline, not at the attempt count", async () => {
    let now = 0;
    vi.spyOn(Date, "now").mockImplementation(() => now);
    const attempt = vi.fn(async () => {
      now += ATTEMPT_MS;
      throw transient();
    });
    const { sleepImpl } = recordingSleep();
    await expect(
      runWithRetry(attempt, "GET", {
        maxAttempts: RETRY_DEFAULTS.hardCap,
        deadlineMs: DEADLINE_MS,
        sleepImpl,
        randomFn: NO_DELAY,
      }),
    ).rejects.toMatchObject({ status: TRANSIENT_STATUS });
    // Three attempts fit: the third fails past the deadline and no fourth begins.
    expect(attempt).toHaveBeenCalledTimes(3);
    expect(now).toBe(3 * ATTEMPT_MS);
  });

  it("a sleep that would end past the deadline is not taken", async () => {
    let now = 0;
    vi.spyOn(Date, "now").mockImplementation(() => now);
    const attempt = vi.fn(async () => {
      now += ATTEMPT_MS;
      throw transient(RATE_LIMITED_STATUS, DEADLINE_MS);
    });
    const { delays, sleepImpl } = recordingSleep();
    await expect(
      runWithRetry(attempt, "GET", { deadlineMs: DEADLINE_MS, retryAfterCapMs: WIDE_CAP_MS, sleepImpl }),
    ).rejects.toMatchObject({ status: RATE_LIMITED_STATUS });
    expect(attempt).toHaveBeenCalledTimes(1);
    expect(delays).toEqual([]);
  });

  it("the cap clamps exponential growth", async () => {
    const { delays, sleepImpl } = recordingSleep();
    const attempt = vi.fn().mockRejectedValue(transient());
    await runWithRetry(attempt, "GET", {
      maxAttempts: 4,
      baseDelayMs: 2 * BASE_DELAY_MS,
      capDelayMs: 3 * BASE_DELAY_MS,
      sleepImpl,
      randomFn: BARE_DELAY,
    }).catch(() => undefined);
    expect(delays).toEqual([2 * BASE_DELAY_MS, 3 * BASE_DELAY_MS, 3 * BASE_DELAY_MS]);
  });

  it("a Retry-After beyond the cap fails fast", async () => {
    const attempt = vi.fn().mockRejectedValue(transient(RATE_LIMITED_STATUS, LONG_RETRY_AFTER_MS));
    const { delays, sleepImpl } = recordingSleep();
    const onRetry = vi.fn();
    await expect(runWithRetry(attempt, "GET", { sleepImpl, onRetry })).rejects.toMatchObject({
      status: RATE_LIMITED_STATUS,
      retryAfterMs: LONG_RETRY_AFTER_MS,
    });
    expect(attempt).toHaveBeenCalledTimes(1);
    expect(delays).toEqual([]);
    expect(onRetry).not.toHaveBeenCalled();
  });

  it("a Retry-After within the cap is the sleep, exactly, and the cap is the caller's to raise", async () => {
    const { delays, sleepImpl } = recordingSleep();
    const attempt = vi.fn().mockRejectedValueOnce(transient(RATE_LIMITED_STATUS, SHORT_RETRY_AFTER_MS)).mockResolvedValue("ok");
    expect(await runWithRetry(attempt, "GET", { sleepImpl, randomFn: BARE_DELAY })).toBe("ok");
    expect(delays).toEqual([SHORT_RETRY_AFTER_MS]);

    const raised = recordingSleep();
    const slow = vi.fn().mockRejectedValueOnce(transient(RATE_LIMITED_STATUS, LONG_RETRY_AFTER_MS)).mockResolvedValue("ok");
    // Raising the cap alone is not enough: the deadline still bounds the wait.
    expect(
      await runWithRetry(slow, "GET", {
        sleepImpl: raised.sleepImpl,
        retryAfterCapMs: LONG_RETRY_AFTER_MS,
        deadlineMs: 2 * LONG_RETRY_AFTER_MS,
      }),
    ).toBe("ok");
    expect(raised.delays).toEqual([LONG_RETRY_AFTER_MS]);
  });

  it("a Retry-After of zero is no floor: the schedule's own delay applies", async () => {
    const { delays, sleepImpl } = recordingSleep();
    const attempt = vi.fn().mockRejectedValueOnce(transient(RATE_LIMITED_STATUS, 0)).mockResolvedValue("ok");
    await runWithRetry(attempt, "GET", { sleepImpl, randomFn: BARE_DELAY, baseDelayMs: BASE_DELAY_MS });
    expect(delays).toEqual([BASE_DELAY_MS]);
  });
});

describe("full jitter", () => {
  it("full jitter never returns the bare delay twice in a row", async () => {
    const { delays, sleepImpl } = recordingSleep();
    const attempt = vi.fn().mockRejectedValue(transient());
    await expect(
      runWithRetry(attempt, "GET", {
        maxAttempts: 6,
        baseDelayMs: BASE_DELAY_MS,
        capDelayMs: WIDE_CAP_MS,
        sleepImpl,
        randomFn: seededRandom(7),
      }),
    ).rejects.toBeInstanceOf(ApiError);
    expect(delays).toHaveLength(5);
    delays.forEach((delay, index) => {
      const bare = BASE_DELAY_MS * 2 ** index;
      expect(delay).toBeGreaterThanOrEqual(0);
      expect(delay).toBeLessThanOrEqual(bare);
    });
    const bareDelays = delays.filter((delay, index) => delay === BASE_DELAY_MS * 2 ** index);
    expect(bareDelays).toHaveLength(0);
  });

  it("two schedules seeded differently disperse", async () => {
    const runs = await Promise.all(
      [1, 2].map(async (seed) => {
        const { delays, sleepImpl } = recordingSleep();
        const attempt = vi.fn().mockRejectedValue(transient());
        await runWithRetry(attempt, "GET", { sleepImpl, randomFn: seededRandom(seed) }).catch(() => undefined);
        return delays;
      }),
    );
    expect(runs[0]).not.toEqual(runs[1]);
  });
});

describe("provenance decides replay", () => {
  it("a write replays only when it provably never left", async () => {
    const { sleepImpl } = recordingSleep();
    const refused = vi.fn().mockRejectedValueOnce(fetchFailed("ECONNREFUSED")).mockResolvedValue("sent once");
    expect(await runWithRetry(refused, "POST", { sleepImpl })).toBe("sent once");
    expect(refused).toHaveBeenCalledTimes(2);

    const reset = fetchFailed("ECONNRESET");
    const interrupted = vi.fn().mockRejectedValue(reset);
    await expect(runWithRetry(interrupted, "POST", { sleepImpl })).rejects.toBe(reset);
    expect(interrupted).toHaveBeenCalledTimes(1);

    const read = vi.fn().mockRejectedValueOnce(fetchFailed("ECONNRESET")).mockResolvedValue("read again");
    expect(await runWithRetry(read, "GET", { sleepImpl })).toBe("read again");
    expect(read).toHaveBeenCalledTimes(2);
  });
});

describe("interruption", () => {
  it("an abort before the first attempt makes none", async () => {
    const attempt = vi.fn().mockResolvedValue("never");
    const onAttempt = vi.fn();
    const named = new AbortController();
    named.abort();
    await expect(
      runWithRetry(attempt, "GET", { signal: named.signal, onAttempt, cancelled: () => new RequestCancelledError("/v1/x") }),
    ).rejects.toBeInstanceOf(RequestCancelledError);

    const reasoned = new AbortController();
    const reason = new Error("the page moved on");
    reasoned.abort(reason);
    await expect(runWithRetry(attempt, "GET", { signal: reasoned.signal, onAttempt })).rejects.toBe(reason);

    const bare = new AbortController();
    bare.abort("gone");
    await expect(runWithRetry(attempt, "GET", { signal: bare.signal, onAttempt })).rejects.toMatchObject({
      code: RETRY_CODE_CANCELLED,
    });

    expect(attempt).not.toHaveBeenCalled();
    expect(onAttempt).not.toHaveBeenCalled();
  });

  it("an abort during a seam sleep abandons the sleep and reports the attempt it followed", async () => {
    const controller = new AbortController();
    const onAttempt = vi.fn();
    const attempt = vi.fn().mockRejectedValue(transient());
    const sleepImpl = () =>
      new Promise<void>(() => {
        // Never resolves: the abort has to end the run, not the sleep.
        controller.abort();
      });
    const err = await runWithRetry(attempt, "GET", {
      signal: controller.signal,
      sleepImpl,
      onAttempt,
      randomFn: BARE_DELAY,
    }).catch((e: unknown) => e);
    expect(err).toBe(controller.signal.reason);
    expect(attempt).toHaveBeenCalledTimes(1);
    expect(onAttempt).toHaveBeenCalledTimes(1);
    expect(onAttempt.mock.calls[0]?.[0]).toMatchObject({ attempt: 1, status: TRANSIENT_STATUS, terminal: true });
  });

  it("a cancel that lands while the attempt is answering does not lose the answer", async () => {
    const controller = new AbortController();
    const attempt = vi.fn(async () => {
      controller.abort();
      return "answer";
    });
    expect(await runWithRetry(attempt, "GET", { signal: controller.signal })).toBe("answer");
    expect(attempt).toHaveBeenCalledTimes(1);
  });

  it("a seam sleep that rejects surfaces its own error rather than hanging the run", async () => {
    const broken = new Error("the seam broke");
    const attempt = vi.fn().mockRejectedValue(transient());
    await expect(runWithRetry(attempt, "GET", { sleepImpl: () => Promise.reject(broken) })).rejects.toBe(broken);
    expect(attempt).toHaveBeenCalledTimes(1);
  });
});

describe("telemetry says what happened", () => {
  it("telemetry reports the status and delay that occurred", async () => {
    const { delays, sleepImpl } = recordingSleep();
    const attempts: AttemptInfo[] = [];
    const retries: RetryInfo[] = [];
    const attempt = vi.fn().mockRejectedValueOnce(transient()).mockResolvedValue({ status: NO_CONTENT_STATUS });
    await runWithRetry(attempt, "GET", {
      sleepImpl,
      randomFn: BARE_DELAY,
      baseDelayMs: BASE_DELAY_MS,
      onAttempt: (info) => attempts.push(info),
      onRetry: (info) => retries.push(info),
      statusOf: (value) => (value as { status: number }).status,
    });
    expect(attempts).toEqual([{ attempt: 2, status: NO_CONTENT_STATUS, durationMs: expect.any(Number), retryCount: 1, terminal: true }]);
    expect(retries).toHaveLength(1);
    expect(retries[0]).toMatchObject({ attempt: 1, status: TRANSIENT_STATUS, reason: "5xx", delayMs: delays[0] });
    expect(delays).toEqual([BASE_DELAY_MS]);
  });

  it.each([
    [TIMEOUT_STATUS, "timeout"],
    [TOO_EARLY_STATUS, "425"],
    [RATE_LIMITED_STATUS, "429"],
    [TRANSIENT_STATUS, "5xx"],
  ])("a %i names its own reason", async (status, reason) => {
    const { sleepImpl } = recordingSleep();
    const onRetry = vi.fn();
    const attempt = vi.fn().mockRejectedValueOnce(transient(status)).mockResolvedValue("ok");
    await runWithRetry(attempt, "GET", { sleepImpl, onRetry });
    expect(onRetry.mock.calls[0]?.[0]).toMatchObject({ status, reason });
  });

  it("a telemetry hook that throws surfaces its own error, not the failure it was told about", async () => {
    const { sleepImpl } = recordingSleep();
    const broken = new Error("hook broke");
    const attempt = vi.fn().mockRejectedValue(transient());
    await expect(
      runWithRetry(attempt, "GET", {
        sleepImpl,
        onRetry: () => {
          throw broken;
        },
      }),
    ).rejects.toBe(broken);
    expect(attempt).toHaveBeenCalledTimes(1);
  });
});

describe("configuration", () => {
  it.each([
    ["baseDelayMs", { baseDelayMs: Number.NaN }],
    ["capDelayMs", { capDelayMs: -1 }],
    ["deadlineMs", { deadlineMs: Number.POSITIVE_INFINITY }],
    ["retryAfterCapMs", { retryAfterCapMs: -SHORT_RETRY_AFTER_MS }],
  ])("a NaN delay never becomes a hot loop: %s", async (_name, options) => {
    const attempt = vi.fn().mockResolvedValue("never");
    await expect(runWithRetry(attempt, "GET", options)).rejects.toMatchObject({ code: CONFIG_INVALID });
    expect(attempt).not.toHaveBeenCalled();
  });

  it("the defaults carry the deadline and the cap", () => {
    expect(RETRY_DEFAULTS.deadlineMs).toBe(20 * MS_PER_SECOND);
    expect(RETRY_DEFAULTS.retryAfterCapMs).toBe(10 * MS_PER_SECOND);
  });
});
