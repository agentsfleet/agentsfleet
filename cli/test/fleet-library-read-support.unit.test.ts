import { expect, test } from "bun:test";
import { Cause, Effect, Exit, Option } from "effect";

import { CREATE_USAGE, listBundleSupportFiles } from "../src/commands/fleet_library_create.ts";
import { ValidationError } from "../src/errors/index.ts";

test("an unreadable bundle directory returns a validation error", async () => {
  const exit = await Effect.runPromiseExit(
    listBundleSupportFiles("/bundle", () => { throw new Error("EACCES"); }),
  );
  expect(Exit.isFailure(exit)).toBe(true);
  if (Exit.isFailure(exit)) {
    const error = Option.getOrThrow(Cause.findErrorOption(exit.cause));
    expect(error).toBeInstanceOf(ValidationError);
    expect(error).toMatchObject({
      detail: "the bundle directory could not be listed",
      suggestion: CREATE_USAGE,
    });
  }
});
