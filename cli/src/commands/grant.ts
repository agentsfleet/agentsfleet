// Integration Grant CLI commands — Effect-shaped.
//
// agentsfleet grant list   --fleet <id>              → list grants for a fleet
// agentsfleet grant delete --fleet <id> <grant_id>   → revoke a grant immediately

import { Effect } from "effect";
import { EMPTY_CELL } from "../output/index.ts";
import { HTTP_METHOD } from "../constants/http-method.ts";
import { type CommandEffect, workspaceContext } from "./context.ts";
import { render } from "./render.ts";
import { requireValidId, requireValue } from "./workspace-guards.ts";
import { wsGrantsListPath, wsGrantPath } from "../lib/api-paths.ts";

const GRANT_ID_FIELD = "id";

interface GrantRow {
  readonly service?: string | null;
  readonly status?: string | null;
  readonly created_at?: number | string | null;
  readonly approved_at?: number | string | null;
  readonly id?: string | null;
}

interface GrantListResponse {
  readonly items?: ReadonlyArray<GrantRow>;
}

const GRANT_COLUMNS = [
  { key: "service", label: "SERVICE" },
  { key: "status", label: "STATUS" },
  { key: "created_at", label: "CREATED_AT" },
  { key: "approved_at", label: "APPROVED_AT" },
  { key: GRANT_ID_FIELD, label: "ID" },
] as const;

const NO_GRANTS = "no integration grants found" as const;

/** A stamp as an operator reads it, or the empty cell.
 *  The wire carries either a millisecond number or an ISO string, and `Date`
 *  takes both — the union is the schema's, not a widening for convenience. */
const stamp = (at: string | number | null | undefined): string =>
  at ? new Date(at).toISOString() : EMPTY_CELL;

export const grantListEffectFromArgs = (
  fleetIdPositional: string | undefined,
  fleetIdFlag: string | undefined,
): CommandEffect =>
  Effect.gen(function* () {
    const ctx = yield* workspaceContext;
    const fleetId = yield* requireValue(
      fleetIdFlag ?? fleetIdPositional,
      FLEET_REQUIRED,
      GRANT_LIST_USAGE,
    );

    const res = yield* ctx.http.request<GrantListResponse>({
      path: wsGrantsListPath(ctx.workspaceId, fleetId),
      token: ctx.token,
    });

    yield* render(ctx, {
      json: res,
      human: (body) => {
        const grants = body.items ?? [];
        if (grants.length === 0) return ctx.output.info(NO_GRANTS);
        return ctx.output.printTable(
          GRANT_COLUMNS,
          grants.map((g) => ({
            service: g.service ?? "",
            status: g.status ?? "",
            created_at: stamp(g.created_at),
            approved_at: stamp(g.approved_at),
            id: g.id ?? "",
          })),
        );
      },
    });
  });

const FIELD_FLEET_ID = "fleet_id" as const;

/** What a revoked grant means for the fleet, said once. */
const deletedNotice = (grantId: string): string =>
  `Grant ${grantId} deleted. The fleet can no longer use this integration; further attempts will be denied.`;

export const grantDeleteEffectFromArgs = (
  fleetIdFlag: string | undefined,
  grantIdPositional: string | undefined,
): CommandEffect =>
  Effect.gen(function* () {
    const ctx = yield* workspaceContext;
    const fleetIdRaw = yield* requireValue(
      fleetIdFlag,
      FLEET_REQUIRED,
      GRANT_DELETE_USAGE,
    );
    const fleetId = yield* requireValidId(fleetIdRaw, FIELD_FLEET_ID, GRANT_DELETE_USAGE);
    const grantIdRaw = yield* requireValue(
      grantIdPositional,
      GRANT_ID_REQUIRED,
      GRANT_DELETE_USAGE,
    );
    const grantId = yield* requireValidId(grantIdRaw, GRANT_ID_FIELD, GRANT_DELETE_USAGE);

    yield* ctx.http.request<unknown>({
      path: wsGrantPath(ctx.workspaceId, fleetId, grantId),
      method: HTTP_METHOD.delete,
      token: ctx.token,
    });

    yield* render(ctx, {
      json: { deleted: true, id: grantId },
      human: (body) => ctx.output.success(deletedNotice(body.id)),
    });
  });
const FLEET_REQUIRED = "--fleet <id> is required" as const;
const GRANT_ID_REQUIRED = "<grant_id> is required" as const;
const GRANT_LIST_USAGE = "usage: agentsfleet grant list --fleet <id>" as const;
const GRANT_DELETE_USAGE =
  "usage: agentsfleet grant delete <grant_id> --fleet <id>" as const;
