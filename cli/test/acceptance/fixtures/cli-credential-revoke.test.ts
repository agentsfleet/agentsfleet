import { afterEach, beforeEach, describe, expect, it } from "bun:test";

import {
  CliCredentialRevokeFailed,
  MAX_REVOKE_ATTEMPTS,
  revokeCliCredential,
} from "./cli-credential-revoke.ts";

// The one teardown check kept in the suite: a revoke that gives up must name
// the credential's id and never print the minted secret into CI logs. The
// retry schedule itself shows up as live-lane teardown failures.
const API_URL = "https://api.test";
const CLI_CREDENTIAL_ID = "01989abc-def0-7123-8abc-def012345678";
const CLI_CREDENTIAL = `afc_${"a".repeat(64)}`;
const MINTED = { id: CLI_CREDENTIAL_ID, credential: CLI_CREDENTIAL };
const ANSWER_DETAIL = "temporary failure";
const HTTP_SERVICE_UNAVAILABLE = 503;
/** `random` is centred on 0.5: the schedule with no jitter applied. */
const NO_JITTER = (): number => 0.5;

interface Probe {
  readonly attempts: () => number;
}

interface Clock {
  readonly sleep: (ms: number) => Promise<void>;
  readonly delays: ReadonlyArray<number>;
}

let originalFetch: typeof globalThis.fetch;

/** Answers every DELETE with the same status. */
function installAnswers(status: number): Probe {
  let attempts = 0;
  globalThis.fetch = Object.assign(
    async (): Promise<Response> => {
      attempts += 1;
      return new Response(ANSWER_DETAIL, { status });
    },
    { preconnect: originalFetch.preconnect },
  );
  return { attempts: () => attempts };
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

async function revokeFailure(clock: Clock): Promise<CliCredentialRevokeFailed> {
  try {
    await revokeCliCredential(API_URL, MINTED, { sleep: clock.sleep, random: NO_JITTER });
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
  it("gives up at the attempt cap, naming the id and every attempt, never the secret", async () => {
    const probe = installAnswers(HTTP_SERVICE_UNAVAILABLE);
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
});
