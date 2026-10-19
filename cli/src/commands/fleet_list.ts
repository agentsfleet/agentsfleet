// `agentsfleet list` — paginated table of fleets in a workspace.
// Workspace defaults to `current_workspace_id`; `--workspace` overrides.

import { Effect } from "effect";
import { HttpClient } from "../services/http-client.ts";
import { OUTPUT_FORMAT, Output } from "../services/output.ts";
import {
  resolveAuthToken,
  resolveWorkspaceId,
  WORKSPACE_FLAG,
} from "./workspace-guards.ts";
import { isString } from "../lib/guards.ts";
import { QUERY_STARTING_AFTER, wsFleetsPath } from "../lib/api-paths.ts";
import { ui, AGE_KEY } from "../output/index.ts";
import { decodeFleetPage } from "./fleet-read.ts";

const FIELD_NAME = "name" as const;
const FIELD_STATUS = "status" as const;
const FIELD_FLEET_ID = "fleet_id" as const;
const FLEETS_LISTED = "Fleets" as const;
const NO_FLEETS = "No fleets in this workspace." as const;


const buildPath = (
  wsId: string,
  startingAfter: string | undefined,
  limit: string | undefined,
): string => {
  const qs = new URLSearchParams();
  if (isString(startingAfter) && startingAfter.length > 0) qs.set(QUERY_STARTING_AFTER, startingAfter);
  if (isString(limit) && limit.length > 0) qs.set("limit", limit);
  const query = qs.toString();
  return query ? `${wsFleetsPath(wsId)}?${query}` : wsFleetsPath(wsId);
};

export interface ListEffectFlags {
  readonly workspaceId?: string | undefined;
  readonly startingAfter?: string | undefined;
  readonly limit?: string | undefined;
}

export const listEffectFromFlags = Effect.fn("fleet.list")(function* (
  flags: ListEffectFlags,
) {
  const output = yield* Output;
  const http = yield* HttpClient;

  const wsId = yield* resolveWorkspaceId(flags.workspaceId, WORKSPACE_FLAG);
  const token = yield* resolveAuthToken;
  const body = yield* http.request<Record<string, unknown>>({
    path: buildPath(wsId, flags.startingAfter, flags.limit),
    token,
  });
  const res = yield* decodeFleetPage(body);

  if (output.format !== OUTPUT_FORMAT.text) {
    // Validate consumed fields, then preserve the complete served envelope.
    yield* output.success(FLEETS_LISTED, { ...body });
    return;
  }

  const items = res.items;
  if (items.length === 0) {
    yield* output.info(NO_FLEETS);
    return;
  }

  yield* output.printEntityTable(
    {
      name: { key: FIELD_NAME, label: "NAME" },
      id: { key: FIELD_FLEET_ID, label: "FLEET" },
      domain: [{ key: FIELD_STATUS, label: "STATUS" }],
    },
    items.map((z) => ({
      name: z.name,
      fleet_id: z.id,
      status: z.status,
      [AGE_KEY]: z.created_at,
    })),
  );
  if (res.next_cursor) {
    yield* output.info(
      ui.dim(`More available. Next: agentsfleet list --starting-after ${res.next_cursor}`),
    );
  }
});
