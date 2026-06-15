// Workspace group handler-binding — extracted from handlers-bind.ts to
// keep that file under the 350-line FLL cap. Each workspace.* leaf is
// an Effect.Effect<void, CliError, R>; the dispatcher (runEffect)
// provides the MainLayer.

import type { WorkspaceHandlers } from "./cli-tree-types.ts";
import type { WrapE, WrapEFn } from "./handlers-bind-agent.ts";
import { readStringOpt as optString } from "../commands/types.ts";
import {
  workspaceAddEffect,
  workspaceCredentialsEffect,
  workspaceDeleteEffectFromArgs,
  workspaceListEffect,
  workspaceShowEffectFromArgs,
  workspaceUseEffectFromArgs,
} from "../commands/workspace.ts";

export const buildWorkspaceHandlers = (
  wrapE: WrapE,
  wrapEFn: WrapEFn,
): WorkspaceHandlers => ({
  add: wrapEFn(
    "workspace.add",
    (frame) => workspaceAddEffect(frame.parsed.positionals[0]),
  ),
  list: wrapE("workspace.list", workspaceListEffect),
  use: wrapEFn(
    "workspace.use",
    (frame) =>
      workspaceUseEffectFromArgs(
        frame.parsed.positionals[0],
        optString(frame.parsed.options, FIELD_WORKSPACE_ID_CAMEL) ??
          optString(frame.parsed.options, FIELD_WORKSPACE_ID_DASHED),
      ),
  ),
  show: wrapEFn(
    "workspace.show",
    (frame) =>
      workspaceShowEffectFromArgs(
        frame.parsed.positionals[0],
        optString(frame.parsed.options, FIELD_WORKSPACE_ID_CAMEL) ??
          optString(frame.parsed.options, FIELD_WORKSPACE_ID_DASHED),
      ),
  ),
  credentials: wrapE("workspace.credentials", workspaceCredentialsEffect),
  delete: wrapEFn(
    "workspace.delete",
    (frame) =>
      workspaceDeleteEffectFromArgs(
        frame.parsed.positionals[0],
        optString(frame.parsed.options, FIELD_WORKSPACE_ID_CAMEL) ??
          optString(frame.parsed.options, FIELD_WORKSPACE_ID_DASHED),
      ),
  ),
});
const FIELD_WORKSPACE_ID_DASHED = "workspace-id" as const;
const FIELD_WORKSPACE_ID_CAMEL = "workspaceId" as const;
