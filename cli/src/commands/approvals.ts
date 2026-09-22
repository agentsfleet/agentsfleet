// `agentsfleet approvals` — the workspace approval inbox, read side.
//
// A Fleet whose trigger declares a write capability parks every run behind a
// gate until a person decides it. Before this command the gate was reachable
// only from the dashboard, so a steer issued from a terminal could time out
// forever with nothing in the terminal naming what held it.
//
// Read and decide are separate commands because they are separate capabilities
// server-side (`ApprovalRead` / `ApprovalResolve`); the decide half lives in
// approvals_decide.ts.

import { Effect, type Redacted } from "effect";
import { CliConfig } from "../services/config.ts";
import { Credentials } from "../services/credentials.ts";
import { HttpClient } from "../services/http-client.ts";
import { OUTPUT_FORMAT, Output } from "../services/output.ts";
import { Workspaces } from "../services/workspaces.ts";
import {
  requireValue,
  requireWorkspaceId,
  resolveAuthToken,
} from "./workspace-guards.ts";
import { wsApprovalsPath, wsApprovalPath } from "../lib/api-paths.ts";
import { ValidationError, type CliError } from "../errors/index.ts";
import { ui, AGE_KEY, EMPTY_CELL } from "../output/index.ts";
import {
  GATE_COLUMN,
  GATE_FIELD,
  GATE_MAX_PAGES,
  GATE_PAGE_LIMIT,
  GATE_QUERY,
  GATE_STATUS,
} from "../constants/approvals.ts";

/** One approval gate as the daemon reports it. Every field is optional on the
 *  wire, so the renderer supplies its own placeholder rather than printing
 *  "undefined" for a gate the daemon has not finished resolving. */
export interface ApprovalGate {
  readonly gate_id?: string | null;
  readonly fleet_id?: string | null;
  readonly fleet_name?: string | null;
  readonly gate_kind?: string | null;
  readonly tool_name?: string | null;
  readonly status?: string | null;
  readonly proposed_action?: string | null;
  readonly blast_radius?: string | null;
  readonly created_at?: number | string | null;
  readonly timeout_at?: number | string | null;
  /** Who decided it. `resolved_by_name` is a display name the daemon may not
   *  have, and answers `""` when it does not; `resolved_by` is the subject and
   *  is the one that is always there. api-dev returns exactly that pair. */
  readonly resolved_by?: string | null;
  readonly resolved_by_name?: string | null;
}

export interface ApprovalListResponse {
  readonly items?: ReadonlyArray<ApprovalGate>;
  readonly next_cursor?: string | null;
}

/** One approvals page, with the daemon doing the narrowing.
 *
 *  `status` and `fleet_id` are the route's own query parameters. Filtering
 *  here rather than over a returned page is not a style choice: a client-side
 *  filter sees only what the first page happened to contain, so a workspace
 *  whose pending gate sits behind fifty decided ones reads as "nothing
 *  waiting" — the confident wrong answer this command exists to replace.
 */
export const approvalsQuery = (filters: {
  readonly fleetId?: string | undefined;
  readonly status?: string | undefined;
  readonly cursor?: string | undefined;
}): string => {
  const params = new URLSearchParams({ [GATE_QUERY.limit]: String(GATE_PAGE_LIMIT) });
  if (filters.fleetId) params.set(GATE_QUERY.fleetId, filters.fleetId);
  if (filters.status) params.set(GATE_QUERY.status, filters.status);
  if (filters.cursor) params.set(GATE_QUERY.cursor, filters.cursor);
  return params.toString();
};

/** Every gate the filters match, following `next_cursor` to exhaustion. */
export const fetchGates = (
  wsId: string,
  token: Redacted.Redacted<string>,
  filters: { readonly fleetId?: string | undefined; readonly status?: string | undefined },
): Effect.Effect<ReadonlyArray<ApprovalGate>, CliError, HttpClient> =>
  Effect.gen(function* () {
    const http = yield* HttpClient;
    const gates: ApprovalGate[] = [];
    let cursor: string | undefined;
    for (let page = 0; page < GATE_MAX_PAGES; page += 1) {
      const res: ApprovalListResponse = yield* http.request<ApprovalListResponse>({
        path: `${wsApprovalsPath(wsId)}?${approvalsQuery({ ...filters, cursor })}`,
        token,
      });
      gates.push(...(res.items ?? []));
      if (!res.next_cursor) break;
      cursor = res.next_cursor;
    }
    return gates;
  });

const EMPTY_INBOX = "No approval gates in this workspace." as const;
const GATES_LISTED = "Approval gates" as const;
const GATE_SHOWN = "Approval gate" as const;
const SECTION_TITLE = "Approval gate" as const;
const SHOW_USAGE = "usage: agentsfleet approvals show <gate_id>" as const;
const GATE_ID_REQUIRED = "<gate_id> is required" as const;
export const APPROVALS_LIST_HINT =
  "Decide one with: agentsfleet approvals approve <gate_id>" as const;

const cell = (value: string | null | undefined): string =>
  value && value.length > 0 ? value : EMPTY_CELL;

const isoOrDash = (value: number | string | null | undefined): string =>
  value ? new Date(value).toISOString() : EMPTY_CELL;

/** The gate a command was asked to act on. The shared presence guard with this
 *  command's two strings bound to it. */
export const requireGateId = (
  value: string | undefined,
): Effect.Effect<string, ValidationError> =>
  requireValue(value, GATE_ID_REQUIRED, SHOW_USAGE);

export const approvalsListEffectFromArgs = (
  fleetFilter: string | undefined,
): Effect.Effect<
  void,
  CliError,
  CliConfig | Credentials | HttpClient | Output | Workspaces
> =>
  Effect.gen(function* () {
    const output = yield* Output;
    const workspaceId = yield* requireWorkspaceId;
    const token = yield* resolveAuthToken;

    const gates = yield* fetchGates(workspaceId, token, { fleetId: fleetFilter });

    if (output.format !== OUTPUT_FORMAT.text) {
      yield* output.success(GATES_LISTED, { items: gates });
      return;
    }
    if (gates.length === 0) {
      yield* output.info(EMPTY_INBOX);
      return;
    }
    yield* output.printEntityTable(
      {
        name: { key: GATE_FIELD.fleet, label: GATE_COLUMN.fleet },
        id: { key: GATE_FIELD.gate, label: GATE_COLUMN.gate },
        domain: [
          { key: GATE_FIELD.kind, label: GATE_COLUMN.kind },
          { key: GATE_FIELD.status, label: GATE_COLUMN.status },
          { key: GATE_FIELD.action, label: GATE_COLUMN.action },
        ],
      },
      gates.map((g) => ({
        gate: cell(g.gate_id),
        fleet: cell(g.fleet_name ?? g.fleet_id),
        kind: cell(g.gate_kind),
        status: cell(g.status),
        action: cell(g.proposed_action ?? g.tool_name),
        [AGE_KEY]: typeof g.created_at === "number" ? g.created_at : undefined,
      })),
    );
    if (gates.some((g) => g.status === GATE_STATUS.pending)) {
      yield* output.info(ui.dim(APPROVALS_LIST_HINT));
    }
  });

export const approvalsShowEffectFromArgs = (
  gateIdPositional: string | undefined,
): Effect.Effect<
  void,
  CliError,
  CliConfig | Credentials | HttpClient | Output | Workspaces
> =>
  Effect.gen(function* () {
    const output = yield* Output;
    const http = yield* HttpClient;
    const workspaceId = yield* requireWorkspaceId;
    const token = yield* resolveAuthToken;
    const gateId = yield* requireGateId(gateIdPositional);

    const gate = yield* http.request<ApprovalGate>({
      path: wsApprovalPath(workspaceId, gateId),
      token,
    });

    if (output.format !== OUTPUT_FORMAT.text) {
      yield* output.success(GATE_SHOWN, { ...gate });
      return;
    }
    // The blast radius is the sentence a person needs in full to decide; it is
    // rendered as a key-value block rather than a table cell so it is never
    // truncated to a column width.
    yield* output.printSection(SECTION_TITLE);
    yield* output.printKeyValue({
      gate_id: cell(gate.gate_id),
      fleet: cell(gate.fleet_name ?? gate.fleet_id),
      kind: cell(gate.gate_kind),
      status: cell(gate.status),
      proposed: cell(gate.proposed_action),
      blast_radius: cell(gate.blast_radius),
      created_at: isoOrDash(gate.created_at),
      timeout_at: isoOrDash(gate.timeout_at),
      resolved_by: cell(gate.resolved_by_name || gate.resolved_by),
    });
  });
