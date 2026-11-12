// Workspace selection and the usage view.
//
// Two groups that share nothing but their shape: each names a thing the
// tenant owns and offers the verbs that read or set it.

import { Option } from "effect";
import { Command } from "effect/cli";
import { guardedHandler } from "./guarded-handler.ts";
import {
  workspaceAddEffect,
  workspaceDeleteEffectFromArgs,
  workspaceListEffect,
  workspaceShowEffectFromArgs,
  workspaceUseEffectFromArgs,
} from "../../commands/workspace/workspace.ts";
import { billingShowEffectFromArgs } from "../../commands/billing/billing.ts";
import {
  billingLimitFlag,
  cursorFlag,
  workspaceIdArgument,
  workspaceFlag,
  workspaceIdOptionalArgument,
  workspaceNameArgument,
} from "./flags.ts";

const opt = Option.getOrUndefined;

const LIST = "list" as const;
const SHOW = "show" as const;
const CREATE = "create" as const;
const DELETE = "delete" as const;

// ── workspace ───────────────────────────────────────────────────────

const workspaceCreateCommand = Command.make(CREATE, { name: workspaceNameArgument }).pipe(
  Command.withDescription("Create a new workspace"),
  guardedHandler(({ name }) => workspaceAddEffect(name)),
);

const workspaceListCommand = Command.make(LIST).pipe(
  Command.withDescription("List workspaces"),
  guardedHandler(() => workspaceListEffect),
);

const workspaceUseCommand = Command.make("use", { workspaceId: workspaceIdArgument }).pipe(
  Command.withDescription("Set the active workspace"),
  guardedHandler(({ workspaceId }) => workspaceUseEffectFromArgs(workspaceId, undefined)),
);

// The id may arrive as a positional or as `--workspace`; the handler owns
// which wins, so both reach it rather than one being resolved away here.
const workspaceShowCommand = Command.make(SHOW, {
  workspaceId: workspaceIdOptionalArgument,
  workspace: workspaceFlag,
}).pipe(
  Command.withDescription("Show workspace details"),
  guardedHandler(({ workspaceId, workspace: fromFlag }) =>
    workspaceShowEffectFromArgs(opt(workspaceId), opt(fromFlag)),
  ),
);

const workspaceDeleteCommand = Command.make(DELETE, { workspaceId: workspaceIdArgument }).pipe(
  Command.withDescription("Remove a workspace from local client state"),
  guardedHandler(({ workspaceId }) => workspaceDeleteEffectFromArgs(workspaceId, undefined)),
);

export const workspaceCommand = () => Command.make("workspace").pipe(
  Command.withDescription("Manage workspaces"),
  Command.withSubcommands([
    workspaceCreateCommand,
    workspaceListCommand,
    workspaceUseCommand,
    workspaceShowCommand,
    workspaceDeleteCommand,
  ]),
);

// ── usage ───────────────────────────────────────────────────────────

const usageShowCommand = Command.make(SHOW, {
  limit: billingLimitFlag,
  cursor: cursorFlag,
}).pipe(
  Command.withDescription("Balance and recent fleet and setup costs"),
  guardedHandler(({ limit, cursor }) =>
    billingShowEffectFromArgs({ limit: opt(limit), cursor: opt(cursor) }),
  ),
);

export const usageCommand = () => Command.make("usage").pipe(
  Command.withDescription("Balance and recent spend for this tenant"),
  Command.withSubcommands([usageShowCommand]),
);
