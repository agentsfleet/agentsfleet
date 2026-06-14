// D24 — pingMe maps any HTTP failure to MeValidationError so login can
// fail-loud after persisting a token that doesn't actually authenticate.

import { describe, expect, test } from "bun:test";
import { Cause, Effect, Exit, Layer, Option, Redacted } from "effect";
import { ME_PING_PATH, pingMe } from "../src/lib/me-ping.ts";
import { HttpClient } from "../src/services/http-client.ts";
import { MeValidationError, NetworkError, ServerError } from "../src/errors/index.ts";

const httpLayer = (
  responder: () => Effect.Effect<unknown, NetworkError | ServerError>,
): Layer.Layer<HttpClient> =>
  Layer.succeed(HttpClient, {
    request: () => responder() as Effect.Effect<never, NetworkError | ServerError>,
  });

const tok = Redacted.make("tok_test");

type MeValidationErrorInstance = InstanceType<typeof MeValidationError>;

const findFailure = (
  exit: Exit.Exit<true, MeValidationErrorInstance>,
): MeValidationErrorInstance | null => {
  if (!Exit.isFailure(exit)) return null;
  return Option.getOrNull(Cause.findErrorOption(exit.cause));
};

describe("pingMe", () => {
  test("calls the configured ping path", () => {
    expect(ME_PING_PATH).toBe("/v1/tenants/me/billing");
  });

  test("200 response → true", async () => {
    const program = pingMe(tok).pipe(
      Effect.provide(httpLayer(() => Effect.succeed({ balance_nanos: 0 }))),
    );
    const exit = await Effect.runPromiseExit(program);
    expect(Exit.isSuccess(exit)).toBe(true);
  });

  test("401 → MeValidationError carrying requestId", async () => {
    const program = pingMe(tok).pipe(
      Effect.provide(
        httpLayer(() =>
          Effect.fail(
            new ServerError({
              detail: "unauthorized",
              suggestion: "login",
              code: "UZ-AUTH-002",
              status: 401,
              requestId: "req_abc",
            }),
          ),
        ),
      ),
    );
    const exit = await Effect.runPromiseExit(program);
    const fail = findFailure(exit);
    expect(fail).toBeInstanceOf(MeValidationError);
    expect(fail?.requestId).toBe("req_abc");
  });

  test("network failure → MeValidationError", async () => {
    const program = pingMe(tok).pipe(
      Effect.provide(
        httpLayer(() =>
          Effect.fail(
            new NetworkError({
              detail: "fetch failed",
              suggestion: "check network",
              url: "https://api.test/v1/tenants/me/billing",
            }),
          ),
        ),
      ),
    );
    const exit = await Effect.runPromiseExit(program);
    const fail = findFailure(exit);
    expect(fail).toBeInstanceOf(MeValidationError);
  });

  test("5xx ServerError → MeValidationError (still fail-loud)", async () => {
    const program = pingMe(tok).pipe(
      Effect.provide(
        httpLayer(() =>
          Effect.fail(
            new ServerError({
              detail: "boom",
              suggestion: "later",
              code: "UZ-INTERNAL-001",
              status: 503,
              requestId: null,
            }),
          ),
        ),
      ),
    );
    const exit = await Effect.runPromiseExit(program);
    expect(findFailure(exit)).toBeInstanceOf(MeValidationError);
  });
});
