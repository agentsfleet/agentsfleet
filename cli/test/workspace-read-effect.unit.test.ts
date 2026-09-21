// The read-side workspace handlers: list, use, show, and the secrets
// redirect. Grouped because each is small and none writes workspace state.

import { describe, test, expect } from "bun:test";
import { Effect, Exit } from "effect";
import {
  workspaceSecretsEffect,
  workspaceListEffect,
  workspaceShowEffectFromArgs,
  workspaceUseEffectFromArgs,
} from "../src/commands/workspace.ts";
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
  test("renders table with active marker", async () => {
    const rec = makeRecorder();
    const workspacesState = {
      value: {
        current_workspace_id: WS_ID,
        items: [
          { workspace_id: WS_ID, name: "main", created_at: 1 },
          { workspace_id: WS_ID_2, name: "other", created_at: 2 },
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
    expect(rec.stdout[0]).toContain(`"active":"*"`);
    expect(rec.stdout[0]).toContain(`"workspace_id":"${WS_ID}"`);
    expect(rec.stdout[1]).toContain(`"active":""`);
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
        items: [{ workspace_id: WS_ID, name: "main", created_at: 1 }],
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
        items: [{ workspace_id: WS_ID, name: "main", created_at: 0 }],
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
        items: [{ workspace_id: WS_ID, name: "main", created_at: 0 }],
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
        items: [{ workspace_id: WS_ID, name: "main", created_at: 12345 }],
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
        items: [{ workspace_id: WS_ID, name: "main", created_at: 1 }],
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

describe("workspaceSecretsEffect", () => {
  test("emits redirect JSON envelope in jsonMode", async () => {
    const rec = makeRecorder();
    const program = workspaceSecretsEffect.pipe(
      Effect.provide(configLayer({ jsonMode: true })),
      Effect.provide(outputLayer(rec, OUTPUT_FORMAT.json)),
    );
    await runWith(program);
    expect(
      rec.stdout.some((line) => line.includes(`"status":"redirect"`)),
    ).toBe(true);
  });

  test("emits info line in human mode", async () => {
    const rec = makeRecorder();
    const program = workspaceSecretsEffect.pipe(
      Effect.provide(configLayer()),
      Effect.provide(outputLayer(rec)),
    );
    await runWith(program);
    expect(rec.stdout).toContain("# Workspace secrets");
    expect(rec.stdout.some((line) => line.includes("/secrets"))).toBe(true);
  });

  // The redirect must name the real top-level `secret` group
  // (cli-tree-fleet.ts), not the phantom `agentsfleet agent secret` that has
  // no registration anywhere in the CLI tree.
  const REAL_COMMAND = "agentsfleet secret";
  const PHANTOM_COMMAND = "agentsfleet agent secret";

  test("JSON-mode redirect names the real secret command", async () => {
    const rec = makeRecorder();
    const program = workspaceSecretsEffect.pipe(
      Effect.provide(configLayer({ jsonMode: true })),
      Effect.provide(outputLayer(rec, OUTPUT_FORMAT.json)),
    );
    await runWith(program);
    expect(rec.stdout.some((line) => line.includes(REAL_COMMAND))).toBe(true);
    expect(rec.stdout.some((line) => line.includes(PHANTOM_COMMAND))).toBe(
      false,
    );
  });

  test("human-mode redirect names the real secret command", async () => {
    const rec = makeRecorder();
    const program = workspaceSecretsEffect.pipe(
      Effect.provide(configLayer()),
      Effect.provide(outputLayer(rec)),
    );
    await runWith(program);
    expect(rec.stdout.some((line) => line.includes(REAL_COMMAND))).toBe(true);
    expect(rec.stdout.some((line) => line.includes(PHANTOM_COMMAND))).toBe(
      false,
    );
  });
});
