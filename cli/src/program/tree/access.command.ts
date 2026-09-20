// The access groups: who you are, what you may reach, and who approved it.
//
// `auth`, `api-key`, `connector`, `approvals` and `grant`. Each leaf declares
// its flags from the shared vocabulary and hands the parsed config straight to
// the command Effect — there is no frame, no options bag and no bridge between
// the two, because the parser already produced the values the Effect asks for.

import { Option } from "effect";
import { Command } from "effect/unstable/cli";
import { guardedHandler } from "./guarded-handler.ts";
import { authStatusEffect } from "../../commands/auth.ts";
import {
  apiKeyCreateEffectFromArgs,
  apiKeyDeleteEffectFromId,
  apiKeyListEffectFromArgs,
  apiKeyRevokeEffectFromId,
} from "../../commands/api_key.ts";
import {
  connectorListEffectFromArgs,
  connectorStatusEffectFromArgs,
} from "../../commands/connector.ts";
import {
  approvalsListEffectFromArgs,
  approvalsShowEffectFromArgs,
} from "../../commands/approvals.ts";
import {
  approvalsApproveEffectFromArgs,
  approvalsDenyEffectFromArgs,
} from "../../commands/approvals_decide.ts";
import {
  grantDeleteEffectFromArgs,
  grantListEffectFromArgs,
} from "../../commands/grant.ts";
import {
  apiKeyIdArgument,
  descriptionFlag,
  fleetFlag,
  gateIdArgument,
  grantIdArgument,
  keyNameFlag,
  providerArgument,
  sortFlag,
  workspaceFlag,
} from "./flags.ts";

/** An optional flag as the command Effects spell it. */
const opt = Option.getOrUndefined;

const LIST = "list" as const;
const SHOW = "show" as const;
const CREATE = "create" as const;
const DELETE = "delete" as const;
const STATUS = "status" as const;

// ── auth ────────────────────────────────────────────────────────────

const authStatusCommand = Command.make(STATUS).pipe(
  Command.withDescription("Show active credential source and server-side validity"),
  guardedHandler(() => authStatusEffect),
);

export const authCommand = Command.make("auth").pipe(
  Command.withDescription("Inspect authentication state"),
  Command.withSubcommands([authStatusCommand]),
);

// ── api-key ─────────────────────────────────────────────────────────

const apiKeyCreateCommand = Command.make(CREATE, {
  name: keyNameFlag,
  description: descriptionFlag,
}).pipe(
  Command.withDescription("Create a tenant API key"),
  guardedHandler(({ name, description }) =>
    apiKeyCreateEffectFromArgs({ name: opt(name), description: opt(description) }),
  ),
);

const apiKeyListCommand = Command.make(LIST, { sort: sortFlag }).pipe(
  Command.withDescription("List tenant API keys"),
  guardedHandler(({ sort }) => apiKeyListEffectFromArgs({ sort: opt(sort) })),
);

const apiKeyRevokeCommand = Command.make("revoke", { apiKeyId: apiKeyIdArgument }).pipe(
  Command.withDescription("Revoke a tenant API key"),
  guardedHandler(({ apiKeyId }) => apiKeyRevokeEffectFromId(apiKeyId)),
);

const apiKeyDeleteCommand = Command.make(DELETE, { apiKeyId: apiKeyIdArgument }).pipe(
  Command.withDescription("Delete a revoked tenant API key"),
  guardedHandler(({ apiKeyId }) => apiKeyDeleteEffectFromId(apiKeyId)),
);

export const apiKeyCommand = Command.make("api-key").pipe(
  Command.withDescription("Manage tenant API keys"),
  Command.withSubcommands([
    apiKeyCreateCommand,
    apiKeyListCommand,
    apiKeyRevokeCommand,
    apiKeyDeleteCommand,
  ]),
);

// ── connector ───────────────────────────────────────────────────────

const connectorListCommand = Command.make(LIST, { workspace: workspaceFlag }).pipe(
  Command.withDescription("List connector setup and connection state"),
  guardedHandler(({ workspace }) => connectorListEffectFromArgs(opt(workspace))),
);

const connectorStatusCommand = Command.make(STATUS, {
  provider: providerArgument,
  workspace: workspaceFlag,
}).pipe(
  Command.withDescription("Show connector state"),
  guardedHandler(({ provider, workspace }) =>
    connectorStatusEffectFromArgs(opt(workspace), provider),
  ),
);

export const connectorCommand = Command.make("connector").pipe(
  Command.withDescription("Inspect workspace connectors"),
  Command.withSubcommands([connectorListCommand, connectorStatusCommand]),
);

// ── approvals ───────────────────────────────────────────────────────

const approvalsListCommand = Command.make(LIST, { fleet: fleetFlag }).pipe(
  Command.withDescription("List approval gates in the active workspace"),
  guardedHandler(({ fleet }) => approvalsListEffectFromArgs(opt(fleet))),
);

const approvalsShowCommand = Command.make(SHOW, { gateId: gateIdArgument }).pipe(
  Command.withDescription("Show one gate's proposed action and blast radius"),
  guardedHandler(({ gateId }) => approvalsShowEffectFromArgs(gateId)),
);

// Approve and deny are subcommands rather than one `--decision` flag: the
// daemon gives each its own path segment and its own capability, and a
// decision reachable by a flag default is a decision nobody made.
const approvalsApproveCommand = Command.make("approve", { gateId: gateIdArgument }).pipe(
  Command.withDescription("Approve a gate and let the Fleet continue"),
  guardedHandler(({ gateId }) => approvalsApproveEffectFromArgs(gateId)),
);

const approvalsDenyCommand = Command.make("deny", { gateId: gateIdArgument }).pipe(
  Command.withDescription("Deny a gate and stop the proposed action"),
  guardedHandler(({ gateId }) => approvalsDenyEffectFromArgs(gateId)),
);

export const approvalsCommand = Command.make("approvals").pipe(
  Command.withDescription("Review and decide pending approval gates"),
  Command.withSubcommands([
    approvalsListCommand,
    approvalsShowCommand,
    approvalsApproveCommand,
    approvalsDenyCommand,
  ]),
);

// ── grant ───────────────────────────────────────────────────────────

const grantListCommand = Command.make(LIST, { fleet: fleetFlag }).pipe(
  Command.withDescription("List integration grants for a Fleet"),
  guardedHandler(({ fleet }) => grantListEffectFromArgs(undefined, opt(fleet))),
);

const grantDeleteCommand = Command.make(DELETE, {
  grantId: grantIdArgument,
  fleet: fleetFlag,
}).pipe(
  Command.withDescription("Revoke an integration grant"),
  guardedHandler(({ grantId, fleet }) => grantDeleteEffectFromArgs(opt(fleet), grantId)),
);

export const grantCommand = Command.make("grant").pipe(
  Command.withDescription("Manage integration grants"),
  Command.withSubcommands([grantListCommand, grantDeleteCommand]),
);
