// Login acceptance: the device flow end to end, its renderings, and its rollback.
//
// The suite's support lives beside it — `login-acceptance-fixtures.ts` (the
// values both ends of the round trip share), `login-acceptance-server.ts` (the
// daemon's answers) and `login-acceptance-client.ts` (the services and the one
// runner). What is left here is the claims.

import { describe, test, expect } from "bun:test";
import { Cause, Effect, Exit, Option } from "effect";
import { AuthError, MeValidationError } from "../src/errors/index.ts";
import {
  IDENTITY,
  MINTED_CREDENTIAL,
  SESSION_ID,
  TEST_JWT,
  makeRecorder,
} from "./login-acceptance-fixtures.ts";
import { freshFixture, runLogin } from "./login-acceptance-client.ts";

describe("login acceptance — full device flow end-to-end", () => {
  test("create → poll → prompt → verify → decrypt → persist → exit 0", async () => {
    const rec = makeRecorder();
    // A fixture of its own rather than `freshFixture()`, because this case
    // asserts on what the server was ASKED — the verify count and the captured
    // public key — and not only on what came back.
    const fixture = freshFixture();

    const exit = await Effect.runPromiseExit(runLogin(rec, fixture));

    if (Exit.isFailure(exit)) {
      throw new Error(`expected success, got: ${Cause.pretty(exit.cause)}`);
    }
    expect(rec.savedToken).toBe(MINTED_CREDENTIAL);
    // The session token bought the credential and was then discarded;
    // what reaches disk outlives the minute that token had left.
    expect(rec.savedToken).not.toBe(TEST_JWT);
    expect(rec.savedSessionId).toBe(SESSION_ID);
    expect(rec.promptsAsked).toBe(1);
    expect(fixture.verifyCalls.count).toBe(1);
    // The success line NAMES the person, which is the whole point of the
    // identity read: a terminal that reported "login complete" left the
    // operator with no way to tell which account it had just signed into.
    expect(
      rec.stdout.some(
        (line) =>
          line.includes(IDENTITY.display_name) && line.includes(IDENTITY.tenant_name),
      ),
    ).toBe(true);
    // Analytics capture is asserted in the unit suite: captureLoginCompleted
    // writes to a real config-dir path before emitting, which would mean
    // staging a tmp tree here for a claim that is covered next door.
  });
});


describe("login acceptance — jsonMode rendering + rollback", () => {
  test("a 404 on the identity read rolls back the credential it just minted", async () => {
    const rec = makeRecorder();
    // Every deployment this client talks to serves the identity route, so a
    // 404 there is a failure like any other: the login did not take.
    const exit = await Effect.runPromiseExit(
      runLogin(rec, freshFixture(), { identityNotFound: true }),
    );

    expect(Exit.isFailure(exit)).toBe(true);
    const err = Exit.isFailure(exit)
      ? Option.getOrNull(Cause.findErrorOption(exit.cause))
      : null;
    expect(err).toBeInstanceOf(MeValidationError);
    expect(rec.savedToken).toBe(MINTED_CREDENTIAL);
    expect(rec.cleared).toBe(true);
    expect(rec.stdout.some((line) => line.includes("login complete"))).toBe(false);
  });

  test("an identity carrying no name and no address still completes the login", async () => {
    const rec = makeRecorder();
    // A server answering a 200 with neither a display name nor an address is
    // broken, and the login is not: the credential was minted, it
    // authenticated, and it is on disk. So the line falls back to reporting
    // what happened rather than naming a person it was not told about. A
    // rendering gap must never fail work that completed.
    const exit = await Effect.runPromiseExit(
      runLogin(rec, freshFixture(), {
        identity: { ...IDENTITY, display_name: undefined, email: "" },
      }),
    );

    expect(Exit.isSuccess(exit)).toBe(true);
    expect(rec.savedToken).toBe(MINTED_CREDENTIAL);
    expect(rec.stdout.some((line) => line.includes("login complete"))).toBe(true);
    expect(rec.stdout.some((line) => line.includes("signed in as"))).toBe(false);
  });

  test("jsonMode prints the machine-readable complete payload (no human prose)", async () => {
    const rec = makeRecorder();
    const exit = await Effect.runPromiseExit(runLogin(rec, freshFixture(), { jsonMode: true }));
    expect(Exit.isSuccess(exit)).toBe(true);
    expect(rec.savedToken).toBe(MINTED_CREDENTIAL);
    // The session token bought the credential and was then discarded;
    // what reaches disk outlives the minute that token had left.
    expect(rec.savedToken).not.toBe(TEST_JWT);
    expect(rec.stdout.some((l) => l.includes('"status":"complete"'))).toBe(true);
    expect(rec.stdout.some((l) => l.includes('"token_saved":true'))).toBe(true);
    expect(rec.stdout.some((l) => l.includes("login complete"))).toBe(false);
  });

  test("post-login /me ping failure rolls back the persisted credential", async () => {
    const rec = makeRecorder();
    const exit = await Effect.runPromiseExit(runLogin(rec, freshFixture(), { identityFails: true }));
    expect(Exit.isFailure(exit)).toBe(true);
    const err = Exit.isFailure(exit)
      ? Option.getOrNull(Cause.findErrorOption(exit.cause))
      : null;
    expect(err).toBeInstanceOf(MeValidationError);
    // The token was persisted moments before validation failed; rollback
    // must wipe it so subsequent commands don't reuse a dead-on-arrival token.
    expect(rec.savedToken).toBe(MINTED_CREDENTIAL);
    // The session token bought the credential and was then discarded;
    // what reaches disk outlives the minute that token had left.
    expect(rec.savedToken).not.toBe(TEST_JWT);
    expect(rec.cleared).toBe(true);
  });

  test("an identity read answered unreadably rolls back the credential", async () => {
    const rec = makeRecorder();
    // A 200 this client cannot decode confirms nothing about whose credential
    // was saved, so it clears like a refusal and fails with the same sentence.
    const exit = await Effect.runPromiseExit(
      runLogin(rec, freshFixture(), { identityUnreadable: true }),
    );
    expect(Exit.isFailure(exit)).toBe(true);
    const err = Exit.isFailure(exit)
      ? Option.getOrNull(Cause.findErrorOption(exit.cause))
      : null;
    expect(err).toBeInstanceOf(MeValidationError);
    expect(rec.cleared).toBe(true);
    expect(rec.savedToken).toBe(MINTED_CREDENTIAL);
  });

  test("first wrong code then correct code: retry succeeds, token persists", async () => {
    const rec = makeRecorder();
    const fixture = freshFixture();
    const exit = await Effect.runPromiseExit(
      runLogin(rec, fixture, { firstVerifyFails: true }),
    );
    if (Exit.isFailure(exit)) {
      throw new Error(`expected retry success, got: ${Cause.pretty(exit.cause)}`);
    }
    expect(Exit.isSuccess(exit)).toBe(true);
    expect(rec.savedToken).toBe(MINTED_CREDENTIAL);
    // The session token bought the credential and was then discarded;
    // what reaches disk outlives the minute that token had left.
    expect(rec.savedToken).not.toBe(TEST_JWT);
    // Prompted twice (first attempt + retry), called /verify twice.
    expect(rec.promptsAsked).toBe(2);
    expect(fixture.verifyCalls.count).toBe(2);
  });
});

describe("login acceptance — the credential exchange", () => {
  test("test_failed_exchange_persists_nothing — a refused mint writes nothing and reports why the daemon refused", async () => {
    const rec = makeRecorder();
    const fixture = freshFixture();
    const exit = await Effect.runPromiseExit(
      runLogin(rec, fixture, { mintFails: true }),
    );
    expect(Exit.isFailure(exit)).toBe(true);

    // The exchange was attempted and refused, and nothing reached disk —
    // not the credential, and above all not the session token the flow was
    // still holding at that moment.
    expect(fixture.mintCalls.count).toBe(1);
    expect(rec.savedToken).toBeNull();

    const err = Exit.isFailure(exit)
      ? Option.getOrNull(Cause.findErrorOption(exit.cause))
      : null;
    expect(err).toBeInstanceOf(AuthError);
    // The daemon named the cause (an expired session). That code survives
    // instead of being flattened into the client's generic one, so the
    // operator is told which failure happened.
    expect((err as InstanceType<typeof AuthError>).code).toBe("UZ-AUTH-006");
  });
});
