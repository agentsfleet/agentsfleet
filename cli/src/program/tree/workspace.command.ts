// Workspace selection, tenant provider posture, and the billing view.
//
// Three groups that share nothing but their shape: each names a thing the
// tenant owns and offers the verbs that read or set it.

import { Option } from "effect";
import { Command } from "effect/unstable/cli";
import { guardedHandler } from "./guarded-handler.ts";
import {
  workspaceAddEffect,
  workspaceDeleteEffectFromArgs,
  workspaceListEffect,
  workspaceSecretsEffect,
  workspaceShowEffectFromArgs,
  workspaceUseEffectFromArgs,
} from "../../commands/workspace.ts";
import {
  tenantProviderAddEffectFromArgs,
  tenantProviderDeleteEffect,
  tenantProviderShowEffect,
} from "../../commands/tenant.ts";
import { billingShowEffectFromArgs } from "../../commands/billing.ts";
import {
  billingLimitFlag,
  cursorFlag,
  modelOverrideFlag,
  secretNameFlag,
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

const workspaceSecretsCommand = Command.make("secrets").pipe(
  Command.withDescription("Open the workspace secret vault"),
  guardedHandler(() => workspaceSecretsEffect),
);

const workspaceDeleteCommand = Command.make(DELETE, { workspaceId: workspaceIdArgument }).pipe(
  Command.withDescription("Remove a workspace from local client state"),
  guardedHandler(({ workspaceId }) => workspaceDeleteEffectFromArgs(workspaceId, undefined)),
);

export const workspaceCommand = Command.make("workspace").pipe(
  Command.withDescription("Manage workspaces"),
  Command.withSubcommands([
    workspaceCreateCommand,
    workspaceListCommand,
    workspaceUseCommand,
    workspaceShowCommand,
    workspaceSecretsCommand,
    workspaceDeleteCommand,
  ]),
);

// ── tenant provider ─────────────────────────────────────────────────

const tenantProviderShowCommand = Command.make(SHOW).pipe(
  Command.withDescription("Show the active provider config"),
  guardedHandler(() => tenantProviderShowEffect),
);

const tenantProviderCreateCommand = Command.make(CREATE, {
  secret: secretNameFlag,
  model: modelOverrideFlag,
}).pipe(
  Command.withDescription("Use a self-managed secret"),
  guardedHandler(({ secret, model }) =>
    tenantProviderAddEffectFromArgs(opt(secret), opt(model)),
  ),
);

const tenantProviderDeleteCommand = Command.make(DELETE).pipe(
  Command.withDescription("Reset to the platform default"),
  guardedHandler(() => tenantProviderDeleteEffect),
);

const tenantProviderCommand = Command.make("provider").pipe(
  Command.withDescription("Manage tenant LLM provider posture"),
  Command.withSubcommands([
    tenantProviderShowCommand,
    tenantProviderCreateCommand,
    tenantProviderDeleteCommand,
  ]),
);

export const tenantCommand = Command.make("tenant").pipe(
  Command.withDescription("Tenant-scoped commands"),
  Command.withSubcommands([tenantProviderCommand]),
);

// ── billing ─────────────────────────────────────────────────────────

const billingShowCommand = Command.make(SHOW, {
  limit: billingLimitFlag,
  cursor: cursorFlag,
}).pipe(
  Command.withDescription("Plan, balance, and recent events"),
  guardedHandler(({ limit, cursor }) =>
    billingShowEffectFromArgs({ limit: opt(limit), cursor: opt(cursor) }),
  ),
);

export const billingCommand = Command.make("billing").pipe(
  Command.withDescription("Tenant billing dashboard"),
  Command.withSubcommands([billingShowCommand]),
);
