// The access groups: what you may reach, and who approved it.
//
// `api-keys` and `connections` sit at the root; `approvals` and `grants`
// belong to a fleet, so `fleet.command.ts` nests them. Each leaf declares
// its flags from the shared vocabulary and hands the parsed config straight to
// the command Effect — there is no frame, no options bag and no bridge between
// the two, because the parser already produced the values the Effect asks for.

import { Command } from "effect/cli";
import { guardedHandler } from "./guarded-handler.ts";
import {
  apiKeyCreateEffectFromArgs,
  apiKeyDeleteEffectFromId,
  apiKeyListEffectFromArgs,
  apiKeyRevokeEffectFromId,
} from "../../commands/api-key/api-key.ts";
import {
  connectorListEffectFromArgs,
  connectorStatusEffectFromArgs,
} from "../../commands/connector/connector.ts";
import {
  approvalsListEffectFromArgs,
  approvalsShowEffectFromArgs,
} from "../../commands/approvals/approvals.ts";
import {
  approvalsApproveEffectFromArgs,
  approvalsDenyEffectFromArgs,
} from "../../commands/approvals/decide.ts";
import {
  grantDeleteEffectFromArgs,
  grantListEffectFromArgs,
} from "../../commands/grant/grant.ts";
import { apiKeyIdArgument, fleetFlag, gateIdArgument, grantIdArgument, keyNameFlag, opt, providerArgument, sortFlag, VERB, workspaceFlag } from "./flags.ts";

/** An optional flag as the command Effects spell it. */

// ── api-keys ────────────────────────────────────────────────────────

const apiKeyCreateCommand = Command.make(VERB.create, { name: keyNameFlag }).pipe(
  Command.withDescription("Create an API key for your account"),
  guardedHandler(({ name }) => apiKeyCreateEffectFromArgs({ name: opt(name) })),
);

const apiKeyListCommand = Command.make(VERB.list, { sort: sortFlag }).pipe(
  Command.withDescription("List your account's API keys"),
  guardedHandler(({ sort }) => apiKeyListEffectFromArgs({ sort: opt(sort) })),
);

const apiKeyRevokeCommand = Command.make("revoke", { apiKeyId: apiKeyIdArgument }).pipe(
  Command.withDescription("Turn off an API key so it stops working"),
  guardedHandler(({ apiKeyId }) => apiKeyRevokeEffectFromId(apiKeyId)),
);

const apiKeyDeleteCommand = Command.make(VERB.delete, { apiKeyId: apiKeyIdArgument }).pipe(
  Command.withDescription("Delete an API key you already revoked"),
  guardedHandler(({ apiKeyId }) => apiKeyDeleteEffectFromId(apiKeyId)),
);

export const apiKeysCommand = () => Command.make("api-keys").pipe(
  Command.withDescription("Create and manage API keys for scripts"),
  Command.withSubcommands([
    apiKeyCreateCommand,
    apiKeyListCommand,
    apiKeyRevokeCommand,
    apiKeyDeleteCommand,
  ]),
);

// ── connections ─────────────────────────────────────────────────────

const connectorListCommand = Command.make(VERB.list, { workspace: workspaceFlag }).pipe(
  Command.withDescription("List the apps you can connect, and which are connected"),
  guardedHandler(({ workspace }) => connectorListEffectFromArgs(opt(workspace))),
);

const connectorShowCommand = Command.make(VERB.show, {
  provider: providerArgument,
  workspace: workspaceFlag,
}).pipe(
  Command.withDescription("Show whether one app is connected"),
  guardedHandler(({ provider, workspace }) =>
    connectorStatusEffectFromArgs(opt(workspace), provider),
  ),
);

export const connectionsCommand = () => Command.make("connections").pipe(
  Command.withDescription("See which apps, like GitHub and Slack, are connected"),
  Command.withSubcommands([connectorListCommand, connectorShowCommand]),
);

// ── approvals ───────────────────────────────────────────────────────

const approvalsListCommand = Command.make(VERB.list, { fleet: fleetFlag }).pipe(
  Command.withDescription("List what your fleets are waiting for you to approve"),
  guardedHandler(({ fleet }) => approvalsListEffectFromArgs(opt(fleet))),
);

const approvalsShowCommand = Command.make(VERB.show, { gateId: gateIdArgument }).pipe(
  Command.withDescription("Show one request: what the fleet wants to do and what it would touch"),
  guardedHandler(({ gateId }) => approvalsShowEffectFromArgs(gateId)),
);

// Approve and deny are subcommands rather than one `--decision` flag: the
// daemon gives each its own path segment and its own capability, and a
// decision reachable by a flag default is a decision nobody made.
const approvalsApproveCommand = Command.make("approve", { gateId: gateIdArgument }).pipe(
  Command.withDescription("Approve a request so the fleet carries on"),
  guardedHandler(({ gateId }) => approvalsApproveEffectFromArgs(gateId)),
);

const approvalsDenyCommand = Command.make("deny", { gateId: gateIdArgument }).pipe(
  Command.withDescription("Deny a request so the fleet goes on without it"),
  guardedHandler(({ gateId }) => approvalsDenyEffectFromArgs(gateId)),
);

export const approvalsCommand = () => Command.make("approvals").pipe(
  Command.withDescription("Approve or deny what a fleet is asking for"),
  Command.withSubcommands([
    approvalsListCommand,
    approvalsShowCommand,
    approvalsApproveCommand,
    approvalsDenyCommand,
  ]),
);

// ── grants ──────────────────────────────────────────────────────────

const grantListCommand = Command.make(VERB.list, { fleet: fleetFlag }).pipe(
  Command.withDescription("List what a fleet is allowed to use"),
  guardedHandler(({ fleet }) => grantListEffectFromArgs(undefined, opt(fleet))),
);

const grantRevokeCommand = Command.make("revoke", {
  grantId: grantIdArgument,
  fleet: fleetFlag,
}).pipe(
  Command.withDescription("Take back something a fleet was allowed to use"),
  guardedHandler(({ grantId, fleet }) => grantDeleteEffectFromArgs(opt(fleet), grantId)),
);

export const grantsCommand = () => Command.make("grants").pipe(
  Command.withDescription("See and revoke what a fleet is allowed to use"),
  Command.withSubcommands([grantListCommand, grantRevokeCommand]),
);
