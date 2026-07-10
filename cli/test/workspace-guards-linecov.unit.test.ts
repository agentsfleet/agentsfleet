// Line-coverage backfill for workspace-guards.ts. `requireWorkspaceId` is
// normally reached through workspace-scoped command handlers, so its
// no-workspace-selected failure arm (the ConfigError block) never fires as a
// callable unit. These tests invoke the exported Effect directly with an
// in-memory Workspaces layer, exercising both the failure arm and the
// success return for branch contrast.

import { describe, expect, test } from "bun:test";
import { Cause, Effect, Exit, Option } from "effect";
import { requireWorkspaceId } from "../src/commands/workspace-guards.ts";
import {
  Workspaces,
  type WorkspacesValue,
} from "../src/services/workspaces.ts";
import { ConfigError } from "../src/errors/index.ts";

const WS_ID = "0195b4ba-8d3a-7f13-8abc-000000000010";
const NO_WORKSPACE_DETAIL = "no workspace selected";

const provideWorkspaces = (value: WorkspacesValue) =>
  Effect.provideService(
    requireWorkspaceId,
    Workspaces,
    Workspaces.of({
      load: Effect.succeed(value),
      save: () => Effect.void,
    }),
  );

const runFailure = async (
  exit: Exit.Exit<string, unknown>,
): Promise<ConfigError> => {
  if (Exit.isSuccess(exit)) throw new Error("expected failure");
  const failure = Option.getOrNull(Cause.findErrorOption(exit.cause));
  if (!(failure instanceof ConfigError)) {
    throw new Error("expected ConfigError in cause");
  }
  return failure;
};

describe("requireWorkspaceId", () => {
  test("fails with ConfigError when no workspace is selected", async () => {
    const program = provideWorkspaces({
      current_workspace_id: null,
      items: [],
    });
    const exit = await Effect.runPromiseExit(program);
    const failure = await runFailure(exit);
    expect(failure.detail).toBe(NO_WORKSPACE_DETAIL);
    expect(failure.suggestion).toContain("workspace create");
    expect(failure.suggestion).toContain("workspace use");
  });

  test("ConfigError message surfaces the detail and suggestion", async () => {
    const program = provideWorkspaces({
      current_workspace_id: null,
      items: [{ workspace_id: WS_ID, name: "main", created_at: 1 }],
    });
    const exit = await Effect.runPromiseExit(program);
    const failure = await runFailure(exit);
    expect(failure.message).toContain(NO_WORKSPACE_DETAIL);
    expect(failure.message).toContain("Suggestion:");
  });

  test("empty-string current_workspace_id is treated as unset", async () => {
    // The guard checks falsiness, not null specifically, so "" must also
    // route into the ConfigError arm rather than returning the empty id.
    const program = provideWorkspaces({
      current_workspace_id: "",
      items: [],
    });
    const exit = await Effect.runPromiseExit(program);
    const failure = await runFailure(exit);
    expect(failure).toBeInstanceOf(ConfigError);
    expect(failure.detail).toBe(NO_WORKSPACE_DETAIL);
  });

  test("returns the current workspace id when one is selected", async () => {
    const program = provideWorkspaces({
      current_workspace_id: WS_ID,
      items: [{ workspace_id: WS_ID, name: "main", created_at: 1 }],
    });
    const result = await Effect.runPromise(program);
    expect(result).toBe(WS_ID);
  });
});
