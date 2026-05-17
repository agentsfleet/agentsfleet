// Credentials service tests — exercise getAccessToken / getSavedAt /
// getSessionId / getApiUrl / saveAccessToken / clearAccessToken
// against a tempdir-backed state store. ZOMBIE_STATE_DIR is set per
// test so concurrent runs don't share files.

import { afterEach, beforeEach, describe, expect, test } from "bun:test";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { Effect, Option, Redacted } from "effect";
import { Credentials, CredentialsLive } from "../src/services/credentials.ts";

let tempDir: string;
let originalStateDir: string | undefined;

beforeEach(() => {
  tempDir = mkdtempSync(join(tmpdir(), "zombiectl-creds-test-"));
  originalStateDir = process.env.ZOMBIE_STATE_DIR;
  process.env.ZOMBIE_STATE_DIR = tempDir;
});

afterEach(() => {
  if (originalStateDir === undefined) delete process.env.ZOMBIE_STATE_DIR;
  else process.env.ZOMBIE_STATE_DIR = originalStateDir;
  rmSync(tempDir, { recursive: true, force: true });
});

const provideEffect = async <A, E>(
  effect: Effect.Effect<A, E, Credentials>,
): Promise<A> => Effect.runPromise(Effect.provide(effect, CredentialsLive));

describe("Credentials service", () => {
  test("getAccessToken returns Option.none on empty store", async () => {
    const result = await provideEffect(
      Effect.gen(function* () {
        const c = yield* Credentials;
        return yield* c.getAccessToken;
      }),
    );
    expect(Option.isNone(result)).toBe(true);
  });
  test("saveAccessToken then getAccessToken roundtrips a Redacted token", async () => {
    const result = await provideEffect(
      Effect.gen(function* () {
        const c = yield* Credentials;
        yield* c.saveAccessToken({
          token: Redacted.make("tok-1"),
          sessionId: "sess-1",
          apiUrl: "https://api.test.local",
        });
        return yield* c.getAccessToken;
      }),
    );
    expect(Option.isSome(result)).toBe(true);
    if (Option.isSome(result)) {
      expect(Redacted.value(result.value)).toBe("tok-1");
    }
  });
  test("getSavedAt + getSessionId + getApiUrl return persisted values", async () => {
    const { savedAt, sessionId, apiUrl } = await provideEffect(
      Effect.gen(function* () {
        const c = yield* Credentials;
        yield* c.saveAccessToken({
          token: Redacted.make("tok-1"),
          sessionId: "sess-1",
          apiUrl: "https://api.test.local",
        });
        return {
          savedAt: yield* c.getSavedAt,
          sessionId: yield* c.getSessionId,
          apiUrl: yield* c.getApiUrl,
        };
      }),
    );
    expect(typeof savedAt).toBe("number");
    expect(sessionId).toBe("sess-1");
    expect(apiUrl).toBe("https://api.test.local");
  });
  test("clearAccessToken clears token + sessionId", async () => {
    const { tokenAfter, sessionAfter } = await provideEffect(
      Effect.gen(function* () {
        const c = yield* Credentials;
        yield* c.saveAccessToken({
          token: Redacted.make("tok-2"),
          sessionId: "sess-2",
          apiUrl: "https://x",
        });
        yield* c.clearAccessToken;
        return {
          tokenAfter: yield* c.getAccessToken,
          sessionAfter: yield* c.getSessionId,
        };
      }),
    );
    expect(Option.isNone(tokenAfter)).toBe(true);
    expect(sessionAfter).toBeNull();
  });
  test("saveAccessToken accepts apiUrl undefined", async () => {
    const result = await provideEffect(
      Effect.gen(function* () {
        const c = yield* Credentials;
        yield* c.saveAccessToken({
          token: Redacted.make("tok-3"),
          sessionId: null,
          apiUrl: undefined,
        });
        return yield* c.getApiUrl;
      }),
    );
    expect(result).toBeNull();
  });
});
