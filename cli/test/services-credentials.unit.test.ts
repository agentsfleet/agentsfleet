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
  CLI_CREDENTIAL_TOTAL_LEN,
} from "../src/constants/cli-credential.ts";

// A value the load check accepts, so the roundtrip proves storage rather
// than tripping the shape refusal.
const ROUNDTRIP_CREDENTIAL = `${CLI_CREDENTIAL_PREFIX}${"d".repeat(CLI_CREDENTIAL_BODY_LEN)}`;

test("the mirrored total-length constant matches the shape it claims to mirror", () => {
  // CLI_CREDENTIAL_TOTAL_LEN mirrors the prefix-plus-body length
  // rustd/crates/afd_auth/src/authenticate.rs checks a presented credential
  // against; without this pin it is a drift obligation nothing enforces.
  expect(CLI_CREDENTIAL_TOTAL_LEN).toBe(ROUNDTRIP_CREDENTIAL.length);
});

const stateDir = useFreshStateDir();

const provideEffect = async <A, E>(
  effect: Effect.Effect<A, E, Credentials>,
): Promise<A> => Effect.runPromise(Effect.provide(effect, credentialsLayer(process.env)));

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
  // An unbound record still LOADS here: this module answers "is the token
  // well-shaped", not "may it be dialled at this target". The deployment
  // question belongs to program/auth-guard.ts, which sees the invocation.
  // Pinned so the two never quietly merge.
  test("an unbound record still loads — binding is the guard's question", async () => {
    const snap = await provideEffect(
      Effect.gen(function* () {
        const c = yield* Credentials;
        yield* c.saveAccessToken({
          token: Redacted.make(ROUNDTRIP_CREDENTIAL),
          sessionId: "sess-unbound",
          apiUrl: undefined, // the pre-binding record shape
          credentialId: "cred-unbound",
        });
        return yield* c.snapshot;
      }),
    );
    expect(Option.isSome(snap.accessToken)).toBe(true);
    expect(snap.apiUrl).toBeNull();
    // The record still names a row worth revoking, so `logout` can end it.
    expect(snap.credentialId).toBe("cred-unbound");
  });

  test("snapshot returns every persisted field from one read", async () => {
    const snap = await provideEffect(
      Effect.gen(function* () {
        const c = yield* Credentials;
        yield* c.saveAccessToken({
          token: Redacted.make(ROUNDTRIP_CREDENTIAL),
          sessionId: "sess-1",
          apiUrl: "https://api.test.local",
          credentialId: "cred-row-1",
        });
        return yield* c.snapshot;
      }),
    );
    expect(typeof snap.savedAt).toBe("number");
    expect(snap.sessionId).toBe("sess-1");
    expect(snap.credentialId).toBe("cred-row-1");
    expect(Option.isSome(snap.accessToken)).toBe(true);
    if (Option.isSome(snap.accessToken)) {
      expect(Redacted.value(snap.accessToken.value)).toBe(
        ROUNDTRIP_CREDENTIAL,
      );
    }
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
        const snap = yield* c.snapshot;
        return {
          tokenAfter: snap.accessToken,
          sessionAfter: snap.sessionId,
        };
      }),
    );
    expect(Option.isNone(tokenAfter)).toBe(true);
    expect(sessionAfter).toBeNull();
  });
  test("saveAccessToken accepts apiUrl undefined", async () => {
    const snap = await provideEffect(
      Effect.gen(function* () {
        const c = yield* Credentials;
        yield* c.saveAccessToken({
          token: Redacted.make("tok-3"),
          sessionId: null,
          apiUrl: undefined,
          credentialId: null,
        });
        return yield* c.snapshot;
      }),
    );
    expect(snap.sessionId).toBeNull();
    expect(snap.credentialId).toBeNull();
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
        credentialsLayer(process.env),
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
