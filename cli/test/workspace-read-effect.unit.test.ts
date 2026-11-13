// The read-side workspace handlers: list, use and show. Grouped because each
// is small and none writes workspace state.

import { describe, test, expect } from "bun:test";
import { Effect, Exit } from "effect";
import {
  workspaceListEffect,
  workspaceShowEffectFromArgs,
  workspaceUseEffectFromArgs,
} from "../src/commands/workspace/workspace.ts";
import { OUTPUT_FORMAT } from "../src/services/output.ts";
import type { WorkspacesValue } from "../src/services/workspaces.ts";
import { ConfigError, ValidationError } from "../src/errors/index.ts";
import {
  WS_ID,
  WS_ID_2,
  makeRecorder,
  outputLayer,
  analyticsLayer,
  workspacesLayer,
  configLayer,
  runWith,
  expectFailure,
} from "./helpers-workspace-effect.ts";

describe("workspaceListEffect", () => {
  test("names the workspace you are in under STATUS", async () => {
    const rec = makeRecorder();
    const workspacesState = {
      value: {
        current_workspace_id: WS_ID,
        items: [
          { workspace_id: WS_ID, name: "main", created_at: "1970-01-01T00:00:00.001Z" },
          { workspace_id: WS_ID_2, name: "other", created_at: "1970-01-01T00:00:00.002Z" },
        ],
      } as WorkspacesValue,
    };
    const program = workspaceListEffect.pipe(
      Effect.provide(configLayer()),
      Effect.provide(workspacesLayer(workspacesState)),
      Effect.provide(outputLayer(rec)),
      Effect.provide(analyticsLayer(rec)),
    );
    await runWith(program);
    expect(rec.stdout[0]).toContain(`"status":"active"`);
    expect(rec.stdout[0]).toContain(`"workspace_id":"${WS_ID}"`);
    // The row that is not the current workspace reports the empty cell, not a
    // second status word — there is only one workspace you are in.
    expect(rec.stdout[1]).not.toContain(`"status":"active"`);
    expect(rec.events[0]?.event).toBe("workspace_list_viewed");
    expect(rec.events[0]?.properties).toEqual({ workspace_count: 2 });
  });

  test("emits empty-state info when no workspaces", async () => {
    const rec = makeRecorder();
    const workspacesState = {
      value: { current_workspace_id: null, items: [] } as WorkspacesValue,
    };
    const program = workspaceListEffect.pipe(
      Effect.provide(configLayer()),
      Effect.provide(workspacesLayer(workspacesState)),
      Effect.provide(outputLayer(rec)),
      Effect.provide(analyticsLayer(rec)),
    );
    await runWith(program);
    expect(rec.stdout).toContain("no workspaces");
  });

  test("emits JSON envelope in jsonMode", async () => {
    const rec = makeRecorder();
    const workspacesState = {
      value: {
        current_workspace_id: WS_ID,
        items: [{ workspace_id: WS_ID, name: "main", created_at: "1970-01-01T00:00:00.001Z" }],
      } as WorkspacesValue,
    };
    const program = workspaceListEffect.pipe(
      Effect.provide(configLayer({ jsonMode: true })),
      Effect.provide(workspacesLayer(workspacesState)),
      Effect.provide(outputLayer(rec, OUTPUT_FORMAT.json)),
      Effect.provide(analyticsLayer(rec)),
    );
    await runWith(program);
    expect(
      rec.stdout.some((line) =>
        line.includes(`"current_workspace_id":"${WS_ID}"`),
      ),
    ).toBe(true);
  });
});

describe("workspaceUseEffectFromArgs", () => {
  test("activates known workspace and emits event", async () => {
    const rec = makeRecorder();
    const workspacesState = {
      value: {
        current_workspace_id: null,
        items: [{ workspace_id: WS_ID, name: "main", created_at: "1970-01-01T00:00:00Z" }],
      } as WorkspacesValue,
    };
    const program = workspaceUseEffectFromArgs(WS_ID, undefined).pipe(
      Effect.provide(configLayer()),
      Effect.provide(workspacesLayer(workspacesState)),
      Effect.provide(outputLayer(rec)),
      Effect.provide(analyticsLayer(rec)),
    );
    const exit = await runWith(program);
    expect(Exit.isSuccess(exit)).toBe(true);
    expect(workspacesState.value.current_workspace_id).toBe(WS_ID);
    expect(rec.events[0]?.event).toBe("workspace_used");
  });

  test("ValidationError when no id provided", async () => {
    const rec = makeRecorder();
    const workspacesState = {
      value: { current_workspace_id: null, items: [] } as WorkspacesValue,
    };
    const program = workspaceUseEffectFromArgs(undefined, undefined).pipe(
      Effect.provide(configLayer()),
      Effect.provide(workspacesLayer(workspacesState)),
      Effect.provide(outputLayer(rec)),
      Effect.provide(analyticsLayer(rec)),
    );
    const failure = expectFailure(await runWith(program));
    expect(failure).toBeInstanceOf(ValidationError);
  });

  test("ValidationError on malformed uuid", async () => {
    const rec = makeRecorder();
    const workspacesState = {
      value: { current_workspace_id: null, items: [] } as WorkspacesValue,
    };
    const program = workspaceUseEffectFromArgs("not-a-uuid", undefined).pipe(
      Effect.provide(configLayer()),
      Effect.provide(workspacesLayer(workspacesState)),
      Effect.provide(outputLayer(rec)),
      Effect.provide(analyticsLayer(rec)),
    );
    const failure = expectFailure(await runWith(program));
    expect(failure).toBeInstanceOf(ValidationError);
  });

  test("ConfigError when id is well-formed but unknown", async () => {
    const rec = makeRecorder();
    const workspacesState = {
      value: { current_workspace_id: null, items: [] } as WorkspacesValue,
    };
    const program = workspaceUseEffectFromArgs(WS_ID, undefined).pipe(
      Effect.provide(configLayer()),
      Effect.provide(workspacesLayer(workspacesState)),
      Effect.provide(outputLayer(rec)),
      Effect.provide(analyticsLayer(rec)),
    );
    const failure = expectFailure(await runWith(program));
    expect(failure).toBeInstanceOf(ConfigError);
    expect(failure.suggestion).toContain("workspace create <name>");
  });

  test("reads workspaceId from --workspace flag", async () => {
    const rec = makeRecorder();
    const workspacesState = {
      value: {
        current_workspace_id: null,
        items: [{ workspace_id: WS_ID, name: "main", created_at: "1970-01-01T00:00:00Z" }],
      } as WorkspacesValue,
    };
    const program = workspaceUseEffectFromArgs(undefined, WS_ID).pipe(
      Effect.provide(configLayer({ jsonMode: true })),
      Effect.provide(workspacesLayer(workspacesState)),
      Effect.provide(outputLayer(rec, OUTPUT_FORMAT.json)),
      Effect.provide(analyticsLayer(rec)),
    );
    await runWith(program);
    expect(workspacesState.value.current_workspace_id).toBe(WS_ID);
    expect(
      rec.stdout.some((line) => line.includes(`"active":"${WS_ID}"`)),
    ).toBe(true);
  });
});

describe("workspaceShowEffectFromArgs", () => {
  test("falls back to current_workspace_id and renders detail", async () => {
    const rec = makeRecorder();
    const workspacesState = {
      value: {
        current_workspace_id: WS_ID,
        items: [{ workspace_id: WS_ID, name: "main", created_at: "1970-01-01T00:00:12.345Z" }],
      } as WorkspacesValue,
    };
    const program = workspaceShowEffectFromArgs(undefined, undefined).pipe(
      Effect.provide(configLayer({ jsonMode: true })),
      Effect.provide(workspacesLayer(workspacesState)),
      Effect.provide(outputLayer(rec, OUTPUT_FORMAT.json)),
    );
    await runWith(program);
    expect(
      rec.stdout.some((line) => line.includes(`"workspace_id":"${WS_ID}"`)),
    ).toBe(true);
    expect(rec.stdout.some((line) => line.includes(`"active":true`))).toBe(
      true,
    );
  });

  test("ConfigError when no id and no current workspace", async () => {
    const rec = makeRecorder();
    const workspacesState = {
      value: { current_workspace_id: null, items: [] } as WorkspacesValue,
    };
    const program = workspaceShowEffectFromArgs(undefined, undefined).pipe(
      Effect.provide(configLayer()),
      Effect.provide(workspacesLayer(workspacesState)),
      Effect.provide(outputLayer(rec)),
    );
    const failure = expectFailure(await runWith(program));
    expect(failure).toBeInstanceOf(ConfigError);
  });

  test("human render emits section + key-value block", async () => {
    const rec = makeRecorder();
    const workspacesState = {
      value: {
        current_workspace_id: WS_ID,
        items: [{ workspace_id: WS_ID, name: "main", created_at: "1970-01-01T00:00:00.001Z" }],
      } as WorkspacesValue,
    };
    const program = workspaceShowEffectFromArgs(undefined, undefined).pipe(
      Effect.provide(configLayer()),
      Effect.provide(workspacesLayer(workspacesState)),
      Effect.provide(outputLayer(rec)),
    );
    await runWith(program);
    expect(rec.stdout).toContain("# Workspace");
    expect(rec.stdout.some((line) => line.includes(`workspace_id:`))).toBe(
      true,
    );
  });
});
