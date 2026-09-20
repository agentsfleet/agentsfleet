// The id checks inside the handlers are defence in depth, not the first line.
// The tree's own argument validator refuses a malformed id at parse time, so
// the CLI can no longer reach these branches — but the effects are exported
// and callable directly, and an effect that trusts its caller's id would send
// a malformed one to the server. These tests call them the way a non-CLI
// caller would, which is the only way in.

import { describe, expect, test } from "bun:test";
import { Effect, Exit, Layer } from "effect";

import { Output, OUTPUT_FORMAT, type OutputShape } from "../src/services/output.ts";
import { HttpClient } from "../src/services/http-client.ts";
import { Credentials } from "../src/services/credentials.ts";
import { Workspaces } from "../src/services/workspaces.ts";

import { stopEffectFromId, resumeEffectFromId, killEffectFromId } from "../src/commands/fleet.ts";
import { updateEffectFromArgs } from "../src/commands/fleet_install.ts";

const MALFORMED = "not-a-uuid";
const WORKSPACE_ID = "0192a3b4-c5d6-7e8f-9012-3456789abcde";

// The refusal happens after Output is acquired and before anything is sent, so
// Output is the only service these runs need. Providing the rest would let a
// check that silently stopped firing reach the network instead of failing
// here, which is the opposite of what this file is for.
const silentOutput = Layer.succeed(Output, {
  stdoutIsTty: false,
  format: OUTPUT_FORMAT.text,
  intro: () => Effect.void,
  info: () => Effect.void,
  success: () => Effect.void,
  warn: () => Effect.void,
  error: () => Effect.void,
  outro: () => Effect.void,
  printJson: () => Effect.void,
  printJsonErr: () => Effect.void,
  printKeyValue: () => Effect.void,
  printSection: () => Effect.void,
  printTable: () => Effect.void,
} as OutputShape);

// The transport DIES rather than answering. The point of these tests is that
// a malformed id never reaches the wire, so a run that gets as far as asking
// for a request has already failed — and says so, instead of quietly passing
// against a stub that answered.
const unreachableTransport = Layer.succeed(HttpClient, {
  request: () => Effect.die(new Error("a malformed id reached the transport")),
} as never);

// Local reads that legitimately happen before the id is checked — resolving
// which workspace and credential the call WOULD use costs nothing and sends
// nothing. Only the transport is forbidden.
const localCredentials = Layer.succeed(Credentials, {
  getAccessToken: Effect.succeed({ _tag: "None" }),
  snapshot: Effect.succeed({
    accessToken: { _tag: "None" },
    savedAt: null,
    sessionId: null,
    apiUrl: null,
    credentialId: null,
  }),
  saveAccessToken: () => Effect.void,
  clearAccessToken: Effect.void,
} as never);

const localWorkspaces = Layer.succeed(Workspaces, {
  load: Effect.succeed({ current_workspace_id: WORKSPACE_ID, items: [] }),
  save: () => Effect.void,
} as never);

const services = Layer.mergeAll(
  silentOutput,
  unreachableTransport,
  localCredentials,
  localWorkspaces,
);

const refusalText = async (effect: Effect.Effect<unknown, unknown, never>): Promise<string> => {
  const exit = await Effect.runPromiseExit(
    (effect as Effect.Effect<unknown, unknown, never>).pipe(
      Effect.provide(services as never),
    ) as Effect.Effect<unknown, unknown, never>,
  );
  if (Exit.isSuccess(exit)) throw new Error("expected the id check to refuse");
  return String(exit.cause);
};

describe("handler id defence — a malformed id never reaches the wire", () => {
  test("stop refuses a malformed fleet_id", async () => {
    const text = await refusalText(
      stopEffectFromId(MALFORMED) as Effect.Effect<unknown, unknown, never>,
    );
    expect(text).toContain("fleet_id");
  });

  test("resume refuses a malformed fleet_id", async () => {
    const text = await refusalText(
      resumeEffectFromId(MALFORMED) as Effect.Effect<unknown, unknown, never>,
    );
    expect(text).toContain("fleet_id");
  });

  test("kill refuses a malformed fleet_id", async () => {
    const text = await refusalText(
      killEffectFromId(MALFORMED) as Effect.Effect<unknown, unknown, never>,
    );
    expect(text).toContain("fleet_id");
  });

  test("an absent fleet_id is refused as required, not as malformed", async () => {
    const text = await refusalText(
      stopEffectFromId(undefined) as Effect.Effect<unknown, unknown, never>,
    );
    expect(text).toContain("required");
  });

  test("fleet update refuses a malformed fleet_id", async () => {
    const text = await refusalText(
      updateEffectFromArgs(MALFORMED, null) as Effect.Effect<unknown, unknown, never>,
    );
    expect(text).toContain("fleet_id");
  });
});
