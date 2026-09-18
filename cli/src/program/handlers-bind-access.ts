import type { Handlers } from "./cli-tree-types.ts";
import type { WrapEFn } from "./handlers-bind-fleet.ts";
import { readStringOpt as optString } from "../commands/types.ts";
import {
  apiKeyCreateEffectFromArgs,
  apiKeyDeleteEffectFromId,
  apiKeyListEffectFromArgs,
  apiKeyRevokeEffectFromId,
} from "../commands/api_key.ts";
import {
  connectorListEffectFromArgs,
  connectorStatusEffectFromArgs,
} from "../commands/connector.ts";
import {
  approvalsListEffectFromArgs,
  approvalsShowEffectFromArgs,
} from "../commands/approvals.ts";
import {
  approvalsApproveEffectFromArgs,
  approvalsDenyEffectFromArgs,
} from "../commands/approvals_decide.ts";
import { OPT_AGENT } from "../constants/cli-flags.ts";

const OPTION_WORKSPACE = "workspace" as const;

export const buildAccessHandlers = (
  wrapEFn: WrapEFn,
): Pick<Handlers, "apiKey" | "connector" | "approvals"> => ({
  apiKey: {
    create: wrapEFn(
      "api-key.create",
      (frame) =>
        apiKeyCreateEffectFromArgs({
          name: optString(frame.parsed.options, "name"),
          description: optString(frame.parsed.options, "description"),
        }),
    ),
    list: wrapEFn(
      "api-key.list",
      (frame) =>
        apiKeyListEffectFromArgs({
          sort: optString(frame.parsed.options, "sort"),
        }),
    ),
    revoke: wrapEFn(
      "api-key.revoke",
      (frame) => apiKeyRevokeEffectFromId(frame.parsed.positionals[0]),
    ),
    delete: wrapEFn(
      "api-key.delete",
      (frame) => apiKeyDeleteEffectFromId(frame.parsed.positionals[0]),
    ),
  },
  approvals: {
    list: wrapEFn(
      "approvals.list",
      (frame) => approvalsListEffectFromArgs(optString(frame.parsed.options, OPT_AGENT)),
    ),
    show: wrapEFn(
      "approvals.show",
      (frame) => approvalsShowEffectFromArgs(frame.parsed.positionals[0]),
    ),
    approve: wrapEFn(
      "approvals.approve",
      (frame) => approvalsApproveEffectFromArgs(frame.parsed.positionals[0]),
    ),
    deny: wrapEFn(
      "approvals.deny",
      (frame) => approvalsDenyEffectFromArgs(frame.parsed.positionals[0]),
    ),
  },
  connector: {
    list: wrapEFn(
      "connector.list",
      (frame) => connectorListEffectFromArgs(optString(frame.parsed.options, OPTION_WORKSPACE)),
    ),
    status: wrapEFn(
      "connector.status",
      (frame) =>
        connectorStatusEffectFromArgs(
          optString(frame.parsed.options, OPTION_WORKSPACE),
          frame.parsed.positionals[0],
        ),
    ),
  },
});
