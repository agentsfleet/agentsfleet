import { describe, expect, test } from "bun:test";
import { Effect, Exit, Layer } from "effect";
import { exitToCliError, renderCliError } from "../src/lib/cli-error-render.ts";
import { Output } from "../src/services/output.ts";
import { ServerError, UnexpectedError, ValidationError } from "../src/errors/index.ts";

const recordingOutput = (sink: string[]) =>
  Layer.succeed(Output, {
    intro: () => Effect.void,
    info: () => Effect.void,
    success: () => Effect.void,
    warn: (msg) => Effect.sync(() => sink.push(msg)),
    error: (msg) => Effect.sync(() => sink.push(msg)),
    outro: () => Effect.void,
    printJson: () => Effect.void,
    printJsonErr: () => Effect.void,
    printKeyValue: () => Effect.void,
    printSection: () => Effect.void,
    printTable: () => Effect.void,
  });

describe("exitToCliError", () => {
  test("a success exit maps to an UnexpectedError", () => {
    const err = exitToCliError(Exit.succeed(undefined));
    expect(err._tag).toBe("UnexpectedError");
    expect(err.message).toContain("unexpected successful exit");
  });

  test("a typed failure is returned unchanged", () => {
    const original = new ValidationError({ detail: "bad arg", suggestion: "use --help" });
    expect(exitToCliError(Exit.fail(original))).toBe(original);
  });

  test("a defect (die) with no typed error maps to UnexpectedError carrying the pretty cause", () => {
    const err = exitToCliError(Exit.die(new Error("boom")));
    expect(err._tag).toBe("UnexpectedError");
    expect(err.message).toContain("boom");
  });
});

describe("renderCliError", () => {
  test("ServerError prints code + detail + suggestion + request_id", async () => {
    const sink: string[] = [];
    const err = new ServerError({ detail: "nope", suggestion: "retry", code: "UZ-X", status: 502, requestId: "req-1" });
    await Effect.runPromise(renderCliError(err).pipe(Effect.provide(recordingOutput(sink))));
    expect(sink[0]).toContain("UZ-X nope");
    expect(sink[0]).toContain("request_id: req-1");
  });

  test("ServerError without a request_id omits the request_id line", async () => {
    const sink: string[] = [];
    const err = new ServerError({ detail: "nope", suggestion: "retry", code: "UZ-X", status: 502, requestId: null });
    await Effect.runPromise(renderCliError(err).pipe(Effect.provide(recordingOutput(sink))));
    expect(sink[0]).not.toContain("request_id");
  });

  test("a non-server error prints its plain message", async () => {
    const sink: string[] = [];
    const err = new UnexpectedError({ detail: "kaboom", suggestion: "report this" });
    await Effect.runPromise(renderCliError(err).pipe(Effect.provide(recordingOutput(sink))));
    expect(sink[0]).toContain("kaboom");
  });
});
