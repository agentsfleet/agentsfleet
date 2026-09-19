// The caller-identity read: what it decodes, and how it fails.
//
// Two claims per test file's worth of behaviour in one, because they are one
// call. `readIdentity` is both the post-login proof that a freshly minted
// credential authenticates and the source of the person `whoami` prints, so a
// failure mapping that regressed would break a login and a render together.

import { describe, expect, test } from "bun:test";
import { Cause, Effect, Exit, Layer, Option, Redacted } from "effect";
import { decodeIdentity, readIdentity } from "../src/lib/me-ping.ts";
import { USERS_ME_PATH } from "../src/lib/api-paths.ts";
import { HttpClient } from "../src/services/http-client.ts";
import { NetworkError, ServerError, UnexpectedError } from "../src/errors/index.ts";

interface RequestRecord {
  readonly path: string;
}

const httpLayer = (
  responder: () => Effect.Effect<unknown, NetworkError | ServerError>,
  seen: RequestRecord[] = [],
): Layer.Layer<HttpClient> =>
  Layer.succeed(HttpClient, {
    request: (input: { path: string }) => {
      seen.push({ path: input.path });
      return responder() as Effect.Effect<never, NetworkError | ServerError>;
    },
  } as unknown as HttpClient);

const tok = Redacted.make("tok_test");

const BODY = {
  user_id: "0193c5e1-0000-7000-8000-00000000abcd",
  email: "ada@example.com",
  display_name: "Ada Lovelace",
  tenant_id: "0193c5e0-0000-7000-8000-000000001234",
  tenant_name: "Ada's Workshop",
  credential: "cli_credential",
  scopes: ["fleet:read", "fleet:write"],
} as const;

const findFailure = (exit: Exit.Exit<unknown, unknown>): unknown => {
  if (!Exit.isFailure(exit)) return null;
  return Option.getOrNull(Cause.findErrorOption(exit.cause));
};

const failing = (
  error: NetworkError | ServerError,
): Layer.Layer<HttpClient> => httpLayer(() => Effect.fail(error));

const runWith = (layer: Layer.Layer<HttpClient>) =>
  Effect.runPromiseExit(readIdentity(tok).pipe(Effect.provide(layer)));

describe("decodeIdentity", () => {
  test("a full body decodes every field", () => {
    const identity = decodeIdentity(BODY);
    expect(identity?.email).toBe("ada@example.com");
    expect(identity?.displayName).toBe("Ada Lovelace");
    expect(identity?.tenantName).toBe("Ada's Workshop");
    expect(identity?.credential).toBe("cli_credential");
    expect(identity?.scopes).toEqual(["fleet:read", "fleet:write"]);
  });

  test("an omitted display name decodes as null, not as a guess", () => {
    const { display_name: _omitted, ...rest } = BODY;
    expect(decodeIdentity(rest)?.displayName).toBeNull();
  });

  test("an omitted scopes key decodes as empty, never undefined", () => {
    const { scopes: _omitted, ...rest } = BODY;
    expect(decodeIdentity(rest)?.scopes).toEqual([]);
  });

  test("a body short a required field decodes to null", () => {
    for (const key of ["user_id", "email", "tenant_id", "tenant_name", "credential"]) {
      const partial: Record<string, unknown> = { ...BODY };
      delete partial[key];
      expect(decodeIdentity(partial)).toBeNull();
    }
  });

  test("a non-object body decodes to null", () => {
    expect(decodeIdentity(null)).toBeNull();
    expect(decodeIdentity("who")).toBeNull();
  });
});

describe("readIdentity", () => {
  test("reads the scope-free identity route, not the billing snapshot", async () => {
    const seen: RequestRecord[] = [];
    await Effect.runPromiseExit(
      readIdentity(tok).pipe(
        Effect.provide(httpLayer(() => Effect.succeed(BODY), seen)),
      ),
    );
    expect(seen.map((record) => record.path)).toEqual([USERS_ME_PATH]);
  });

  test("a 200 answers the decoded identity", async () => {
    const exit = await runWith(httpLayer(() => Effect.succeed(BODY)));
    expect(Exit.isSuccess(exit)).toBe(true);
  });

  test("a 200 carrying no readable identity is a typed failure", async () => {
    const exit = await runWith(httpLayer(() => Effect.succeed({})));
    // Not a refusal: the credential worked and the ANSWER did not parse, which
    // is never a reason to tell somebody to sign in again.
    expect(findFailure(exit)).toBeInstanceOf(UnexpectedError);
  });

  test("a refusal travels out unmapped, carrying the code and request id", async () => {
    const exit = await runWith(
      failing(
        new ServerError({
          detail: "unauthorized",
          suggestion: "login",
          code: "UZ-AUTH-002",
          status: 401,
          requestId: "req_abc",
        }),
      ),
    );
    // Unmapped on purpose: each caller means something different by it, so the
    // sentence is chosen at the call site and the server's own code survives.
    const fail = findFailure(exit) as InstanceType<typeof ServerError>;
    expect(fail).toBeInstanceOf(ServerError);
    expect(fail.code).toBe("UZ-AUTH-002");
    expect(fail.requestId).toBe("req_abc");
  });

  test("a network failure fails loud rather than passing as an identity", async () => {
    const exit = await runWith(
      failing(
        new NetworkError({
          detail: "fetch failed",
          suggestion: "check network",
          url: `https://api.test${USERS_ME_PATH}`,
        }),
      ),
    );
    expect(findFailure(exit)).toBeInstanceOf(NetworkError);
  });

  test("a 503 fails loud too — an outage never reads as a good credential", async () => {
    const exit = await runWith(
      failing(
        new ServerError({
          detail: "boom",
          suggestion: "later",
          code: "UZ-INTERNAL-001",
          status: 503,
          requestId: null,
        }),
      ),
    );
    expect(findFailure(exit)).toBeInstanceOf(ServerError);
  });
});
