// Credentials service tests — exercise getAccessToken / getSavedAt /
// getSessionId / getApiUrl / saveAccessToken / clearAccessToken
// against a tempdir-backed state store. AGENTSFLEET_STATE_DIR is set per
// test so concurrent runs don't share files.

import { describe, expect, test } from "bun:test";
import { mkdirSync } from "node:fs";
import { join } from "node:path";
import { Cause, Effect, Exit, Option, Redacted } from "effect";
import { Credentials, credentialsLayer } from "../src/services/credentials.ts";
import { UnexpectedError } from "../src/errors/index.ts";
import { useFreshStateDir } from "./helpers-cli-state.ts";
import {
  CLI_CREDENTIAL_BODY_LEN,
  CLI_CREDENTIAL_PREFIX,
} from "../src/constants/cli-credential.ts";

// A value the load check accepts, so the roundtrip proves storage rather
// than tripping the shape refusal.
const ROUNDTRIP_CREDENTIAL = `${CLI_CREDENTIAL_PREFIX}${"d".repeat(CLI_CREDENTIAL_BODY_LEN)}`;

const stateDir = useFreshStateDir();

const provideEffect = async <A, E>(
  effect: Effect.Effect<A, E, Credentials>,
): Promise<A> => Effect.runPromise(Effect.provide(effect, credentialsLayer));

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
          token: Redacted.make(ROUNDTRIP_CREDENTIAL),
          sessionId: "sess-1",
          apiUrl: "https://api.test.local",
          credentialId: null,
        });
        return yield* c.getAccessToken;
      }),
    );
    expect(Option.isSome(result)).toBe(true);
    if (Option.isSome(result)) {
      expect(Redacted.value(result.value)).toBe(ROUNDTRIP_CREDENTIAL);
    }
  });
  test("getSavedAt + getSessionId + getApiUrl return persisted values", async () => {
    const { savedAt, sessionId, apiUrl } = await provideEffect(
      Effect.gen(function* () {
        const c = yield* Credentials;
        yield* c.saveAccessToken({
          token: Redacted.make(ROUNDTRIP_CREDENTIAL),
          sessionId: "sess-1",
          apiUrl: "https://api.test.local",
          credentialId: null,
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
          credentialId: null,
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
          credentialId: null,
        });
        return yield* c.getApiUrl;
      }),
    );
    expect(result).toBeNull();
  });
  // Deterministic load-error path: a directory where credentials.json is
  // expected makes readFile throw EISDIR (non-ENOENT, non-SyntaxError),
  // propagating through to the `unexpected("load")` inner closure
  // (credentials.ts:38-43) on every uid — unlike chmod, which root/CI can
  // still read past.
  test("getAccessToken surfaces UnexpectedError when credentials.json is a directory", async () => {
    mkdirSync(join(stateDir(), "credentials.json"));
    const exit = await Effect.runPromiseExit(
      Effect.provide(
        Effect.gen(function* () {
          const c = yield* Credentials;
          return yield* c.getAccessToken;
        }),
        credentialsLayer,
      ),
    );
    expect(Exit.isFailure(exit)).toBe(true);
    if (Exit.isFailure(exit)) {
      const err = Option.getOrNull(Cause.findErrorOption(exit.cause));
      expect(err).toBeInstanceOf(UnexpectedError);
      const ue = err as UnexpectedError;
      expect(ue.detail).toMatch(/credentials load failed/);
      expect(ue.suggestion).toMatch(/permissions/);
    }
  });
});
