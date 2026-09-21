// Line-coverage backfill for workspace-guards.ts. `requireWorkspaceId` is
// normally reached through workspace-scoped command handlers, so its
// no-workspace-selected failure arm (the ConfigError block) never fires as a
// callable unit. These tests invoke the exported Effect directly with an
// in-memory Workspaces layer, exercising both the failure arm and the
// success return for branch contrast.

import { describe, expect, test } from "bun:test";
import { Cause, Effect, Exit, Option } from "effect";
import { requireWorkspaceId, resolveWorkspaceId } from "../src/commands/workspace-guards.ts";
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
    expect(failure.suggestion).toContain("workspace create <name>");
    expect(failure.suggestion).toContain("agentsfleet workspace use <id>");
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

  // The suggestion is shared by every workspace-scoped command, so it may only
  // name routes every one of them has. It used to end "or pass --workspace
  // <id>", which `list`, `install` and `approvals list` all answer with
  // `Unrecognized flag: --workspace` — sending a person from one refusal
  // straight into another. A command that does take the flag says so in its
  // own `--help`, where the answer is true rather than usually true.
  test("the bare guard names no flag, because its callers declare none", async () => {
    const program = provideWorkspaces({ current_workspace_id: null, items: [] });
    const exit = await Effect.runPromiseExit(program);
    const failure = await runFailure(exit);
    const suggestion = failure.suggestion ?? "";
    expect(suggestion).toContain("workspace use");
    expect(suggestion).toContain("workspace create");
    expect(suggestion).not.toMatch(/--\w/);
  });

  // The override spelling is NOT uniform: `list` declares `--workspace-id`
  // while `connector list`, `memory list` and `schedule list` declare
  // `--workspace`. One shared sentence was wrong for almost every caller, so
  // each passes the flag it actually has and the refusal names that one.
  test("the resolver names the caller's own override flag", async () => {
    for (const flag of ["--workspace", "--workspace-id"]) {
      const program = resolveWorkspaceId(undefined, flag).pipe(
        Effect.provideService(Workspaces, {
          load: Effect.succeed({ current_workspace_id: null, items: [] }),
          save: () => Effect.void,
        }),
      );
      const exit = await Effect.runPromiseExit(program);
      const failure = await runFailure(exit as never);
      expect(failure.suggestion, `${flag} must be named`).toContain(`pass ${flag} <id>`);
    }
  });
});
