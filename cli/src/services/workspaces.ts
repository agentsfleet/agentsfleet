// Workspaces service — wraps the on-disk workspaces.json store.
//
// Login hydrates this after credentials land so the operator has an
// active workspace from the first command. The shape mirrors the
// Workspaces record from lib/state.ts verbatim (tenant_id,
// current_workspace_id, and items[]) — same JSON on disk, just exposed
// as an Effect surface.

import { Effect, Layer, Context } from "effect";
import {
  STATE_STORE_SUGGESTION,
  loadWorkspaces as loadWorkspacesRaw,
  saveWorkspaces as saveWorkspacesRaw,
  type Workspaces as WorkspacesRecord,
  type WorkspaceItem as WorkspaceItemRecord,
} from "../lib/state.ts";
import { UnexpectedError } from "../errors/index.ts";

export type WorkspaceItem = WorkspaceItemRecord;
export type WorkspacesValue = WorkspacesRecord;

export interface WorkspacesShape {
  readonly load: Effect.Effect<WorkspacesValue, UnexpectedError>;
  readonly save: (
    next: WorkspacesValue,
  ) => Effect.Effect<void, UnexpectedError>;
}

export type Workspaces = WorkspacesShape;
export const Workspaces = Context.Service<Workspaces>(
  "agentsfleet/state/Workspaces",
);

const unexpected =
  (op: string) =>
  (cause: unknown): UnexpectedError =>
    new UnexpectedError({
      detail: `workspaces ${op} failed: ${cause instanceof Error ? cause.message : String(cause)}`,
      suggestion: STATE_STORE_SUGGESTION,
    });

// A factory for the same reason as credentialsLayer: the composition root
// supplies the environment; this module never reads the process environment.
export const workspacesLayer = (
  env: NodeJS.ProcessEnv,
): Layer.Layer<Workspaces> =>
  Layer.succeed(
    Workspaces,
    Workspaces.of({
      load: Effect.tryPromise({
        try: () => loadWorkspacesRaw(env),
        catch: unexpected("load"),
      }),
      save: (next) =>
        Effect.tryPromise({
          try: () => saveWorkspacesRaw(env, next),
          catch: unexpected("save"),
        }),
    }),
  );

export const workspacesFromValueLayer = (
  initial: WorkspacesValue,
): Layer.Layer<Workspaces> => {
  let current: WorkspacesValue = { ...initial, items: [...initial.items] };
  return Layer.succeed(
    Workspaces,
    Workspaces.of({
      load: Effect.sync(() => current),
      save: (next) =>
        Effect.sync(() => {
          current = { ...next, items: [...next.items] };
        }),
    }),
  );
};
