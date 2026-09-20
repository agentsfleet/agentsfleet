// `workspace delete` — the read-side neighbour that writes, which is why it
// reads on its own: local removal and server deletion are different outcomes
// and each has to name itself.

import { describe, test, expect } from "bun:test";
import { Effect, Exit } from "effect";
import { workspaceDeleteEffectFromArgs } from "../src/commands/workspace.ts";
import { OUTPUT_FORMAT } from "../src/services/output.ts";
import type { WorkspacesValue } from "../src/services/workspaces.ts";
import { ValidationError } from "../src/errors/index.ts";
import {
  WS_ID,
  WS_ID_2,
  LOCAL_REMOVAL_STEM,
  SERVER_DELETION_STEM,
  makeRecorder,
  outputLayer,
  analyticsLayer,
  workspacesLayer,
  configLayer,
  runWith,
  expectFailure,
} from "./helpers-workspace-effect.ts";

describe("workspaceDeleteEffectFromArgs", () => {
  test("removes target workspace and emits deleted event", async () => {
    const rec = makeRecorder();
    const workspacesState = {
      value: {
        current_workspace_id: WS_ID,
        items: [
          { workspace_id: WS_ID, name: "main", created_at: 0 },
          { workspace_id: WS_ID_2, name: "other", created_at: 0 },
        ],
      } as WorkspacesValue,
    };
    const program = workspaceDeleteEffectFromArgs(WS_ID, undefined).pipe(
      Effect.provide(configLayer()),
      Effect.provide(workspacesLayer(workspacesState)),
      Effect.provide(outputLayer(rec)),
      Effect.provide(analyticsLayer(rec)),
    );
    const exit = await runWith(program);
    expect(Exit.isSuccess(exit)).toBe(true);
    expect(workspacesState.value.items).toHaveLength(1);
    expect(workspacesState.value.current_workspace_id).toBe(WS_ID_2);
    expect(rec.events[0]?.event).toBe("workspace_deleted");
    expect(rec.stdout).toContain(`ok: ${LOCAL_REMOVAL_STEM}: ${WS_ID}`);
    expect(rec.stdout.some((line) => line.includes(SERVER_DELETION_STEM))).toBe(
      false,
    );
  });

  test("ValidationError when no id provided", async () => {
    const rec = makeRecorder();
    const workspacesState = {
      value: { current_workspace_id: null, items: [] } as WorkspacesValue,
    };
    const program = workspaceDeleteEffectFromArgs(undefined, undefined).pipe(
      Effect.provide(configLayer()),
      Effect.provide(workspacesLayer(workspacesState)),
      Effect.provide(outputLayer(rec)),
      Effect.provide(analyticsLayer(rec)),
    );
    const failure = expectFailure(await runWith(program));
    expect(failure).toBeInstanceOf(ValidationError);
    expect(workspacesState.value.items).toEqual([]);
  });

  test("ValidationError on malformed uuid does not save", async () => {
    const rec = makeRecorder();
    const workspacesState = {
      value: {
        current_workspace_id: WS_ID,
        items: [{ workspace_id: WS_ID, name: "main", created_at: 0 }],
      } as WorkspacesValue,
    };
    const original = workspacesState.value.items;
    const program = workspaceDeleteEffectFromArgs("@@@@", undefined).pipe(
      Effect.provide(configLayer()),
      Effect.provide(workspacesLayer(workspacesState)),
      Effect.provide(outputLayer(rec)),
      Effect.provide(analyticsLayer(rec)),
    );
    const failure = expectFailure(await runWith(program));
    expect(failure).toBeInstanceOf(ValidationError);
    expect(workspacesState.value.items).toBe(original);
  });

  test("JSON output says the workspace was removed from local state", async () => {
    const rec = makeRecorder();
    const workspacesState = {
      value: {
        current_workspace_id: WS_ID,
        items: [{ workspace_id: WS_ID, name: "main", created_at: 0 }],
      } as WorkspacesValue,
    };
    const program = workspaceDeleteEffectFromArgs(WS_ID, undefined).pipe(
      Effect.provide(configLayer({ jsonMode: true })),
      Effect.provide(workspacesLayer(workspacesState)),
      Effect.provide(outputLayer(rec, OUTPUT_FORMAT.json)),
      Effect.provide(analyticsLayer(rec)),
    );
    await runWith(program);
    expect(
      rec.stdout.some((line) =>
        line.includes(`"removed_from_local_state":"${WS_ID}"`),
      ),
    ).toBe(true);
  });
});
