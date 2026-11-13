// Workspace selection and the usage view.
//
// Two groups that share nothing but their shape: each names a thing the
// tenant owns and offers the verbs that read or set it.

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
import { billingLimitFlag, cursorFlag, opt, VERB, workspaceFlag, workspaceIdArgument, workspaceIdOptionalArgument, workspaceNameArgument } from "./flags.ts";


// ── workspace ───────────────────────────────────────────────────────

const workspaceCreateCommand = Command.make(VERB.create, { name: workspaceNameArgument }).pipe(
  Command.withDescription("Create a new workspace"),
  guardedHandler(({ name }) => workspaceAddEffect(name)),
);

const workspaceListCommand = Command.make(VERB.list).pipe(
  Command.withDescription("List workspaces"),
  guardedHandler(() => workspaceListEffect),
);

const workspaceUseCommand = Command.make("use", { workspaceId: workspaceIdArgument }).pipe(
  Command.withDescription("Set the active workspace"),
  guardedHandler(({ workspaceId }) => workspaceUseEffectFromArgs(workspaceId, undefined)),
);

// The id may arrive as a positional or as `--workspace`; the handler owns
// which wins, so both reach it rather than one being resolved away here.
const workspaceShowCommand = Command.make(VERB.show, {
  workspaceId: workspaceIdOptionalArgument,
  workspace: workspaceFlag,
}).pipe(
  Command.withDescription("Show workspace details"),
  guardedHandler(({ workspaceId, workspace: fromFlag }) =>
    workspaceShowEffectFromArgs(opt(workspaceId), opt(fromFlag)),
  ),
);

const workspaceDeleteCommand = Command.make(VERB.delete, { workspaceId: workspaceIdArgument }).pipe(
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

const usageShowCommand = Command.make(VERB.show, {
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
