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

const OPTION_WORKSPACE = "workspace" as const;

export const buildAccessHandlers = (
  wrapEFn: WrapEFn,
): Pick<Handlers, "apiKey" | "connector"> => ({
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
          page: optString(frame.parsed.options, "page"),
          pageSize:
            optString(frame.parsed.options, "pageSize") ??
            optString(frame.parsed.options, "page-size"),
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
