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
  Command.withDescription("Create a workspace"),
  guardedHandler(({ name }) => workspaceAddEffect(name)),
);

const workspaceListCommand = Command.make(VERB.list).pipe(
  Command.withDescription("List your workspaces"),
  guardedHandler(() => workspaceListEffect),
);

const workspaceUseCommand = Command.make("use", { workspaceId: workspaceIdArgument }).pipe(
  Command.withDescription("Switch to a workspace"),
  guardedHandler(({ workspaceId }) => workspaceUseEffectFromArgs(workspaceId, undefined)),
);

// The id may arrive as a positional or as `--workspace`; the handler owns
// which wins, so both reach it rather than one being resolved away here.
const workspaceShowCommand = Command.make(VERB.show, {
  workspaceId: workspaceIdOptionalArgument,
  workspace: workspaceFlag,
}).pipe(
  Command.withDescription("Show a workspace's details"),
  guardedHandler(({ workspaceId, workspace: fromFlag }) =>
    workspaceShowEffectFromArgs(opt(workspaceId), opt(fromFlag)),
  ),
);

const workspaceDeleteCommand = Command.make(VERB.delete, { workspaceId: workspaceIdArgument }).pipe(
  Command.withDescription("Remove a workspace from this computer's list (it stays on the server)"),
  guardedHandler(({ workspaceId }) => workspaceDeleteEffectFromArgs(workspaceId, undefined)),
);

export const workspaceCommand = () => Command.make("workspace").pipe(
  Command.withDescription("Create and switch between workspaces"),
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
  Command.withDescription("Show your balance and what recent work cost"),
  guardedHandler(({ limit, cursor }) =>
    billingShowEffectFromArgs({ limit: opt(limit), cursor: opt(cursor) }),
  ),
);

export const usageCommand = () => Command.make("usage").pipe(
  Command.withDescription("See your balance and what your fleets cost"),
  Command.withSubcommands([usageShowCommand]),
);
