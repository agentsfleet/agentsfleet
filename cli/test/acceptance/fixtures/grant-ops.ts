/**
 * grant-ops.ts — the access-request surface, from the CLI's side of it.
 *
 * A fleet asks for a name-bound refresh handle with `request_access` when its
 * first message tells it to, and waits on the card for a person's answer. The
 * fleet calls no other tool, so this walk exercises permission and delivery
 * without attempting an upstream token exchange.
 */

import { runFleetctl } from "./cli.js";
import type { AuthContext } from "./native-api.ts";

/**
 * The gate kind the daemon raises for a mintable credential. Cross-runtime pair
 * of `afd_approval::KIND_INTEGRATION_GRANT` and of the dashboard acceptance
 * suite's constant of the same name.
 */
export const KIND_INTEGRATION_GRANT = "integration_grant";

/** The connector asked for, as `Connector::name()` spells it. */
export const CONNECTOR_SERVICE_LINEAR = "linear";

/**
 * On-demand handles must be stored under their connector's own name. Linear
 * needs no reverse-routing installation row, unlike GitHub and Slack.
 */
export const GRANT_CREDENTIAL_NAME = CONNECTOR_SERVICE_LINEAR;

/** The vault-handle field naming the connector. Mirrors
 * `afd_credential::secrets::connector::FIELD_INTEGRATION`. */
const FIELD_INTEGRATION = "integration";
const FIELD_REFRESH_TOKEN = "refresh_token";

/** A first message that makes the fleet ask for the connector, and nothing else. */
export const ACCESS_REQUEST_MESSAGE =
  `Call request_access with the name ${GRANT_CREDENTIAL_NAME}. When it answers, reply DONE and stop. Call no other tool.`;

/** Grant-row statuses, as `afd_api_wire::grant::status` spells them. */
export const GRANT_STATUS = {
  pending: "pending",
  approved: "approved",
  revoked: "revoked",
} as const;

/** Gate statuses, as `afd_api_wire::approval::status` spells them. */
export const GATE_STATUS = {
  pending: "pending",
  approved: "approved",
} as const;

const SECRET_CREATE_TIMEOUT_MS = 30_000;
const HTTP_TIMEOUT_MS = 30_000;
const GATE_POLL_MS = 2_000;

/** `Grant` as `GET F/integration-grants` lists it. */
export interface GrantRow {
  readonly id?: string | null;
  readonly kind?: string | null;
  readonly name?: string | null;
  readonly status?: string | null;
  readonly approved_at?: number | string | null;
}

export interface GateRow {
  readonly gate_id: string;
  readonly fleet_id: string;
  readonly gate_type: string;
  readonly status: string;
  readonly proposed_action: string;
  readonly resolved_by: string;
  readonly evidence: Record<string, unknown>;
}

/**
 * Store the connector handle through the CLI's own vault verb.
 *
 * Through the CLI on purpose: this is the one step of the walk an operator
 * genuinely performs from the terminal today, so driving it over HTTP would
 * skip the surface under test.
 */
export async function ensureConnectorHandle(
  env: Readonly<Record<string, string>>,
): Promise<void> {
  const data = JSON.stringify({ [FIELD_INTEGRATION]: CONNECTOR_SERVICE_LINEAR,
    [FIELD_REFRESH_TOKEN]: GRANT_CREDENTIAL_NAME });
  const result = await runFleetctl(
    ["secrets", "create", GRANT_CREDENTIAL_NAME, "--data", data, "--json"],
    { env, timeoutMs: SECRET_CREATE_TIMEOUT_MS },
  );
  if (result.code !== 0) {
    throw new Error(
      `secret create ${GRANT_CREDENTIAL_NAME} exited ${result.code}: ` +
        `${result.stderr.trim() || result.stdout.trim()}`,
    );
  }
}

/** Every grant the CLI reports for one fleet. */
export async function listGrants(
  env: Readonly<Record<string, string>>,
  fleetId: string,
): Promise<ReadonlyArray<GrantRow>> {
  const result = await runFleetctl(["fleet", "grants", "list", "--fleet", fleetId, "--json"], {
    env,
    timeoutMs: HTTP_TIMEOUT_MS,
  });
  if (result.code !== 0) {
    throw new Error(`grant list exited ${result.code}: ${result.stderr.trim()}`);
  }
  const parsed = JSON.parse(result.stdout.trim()) as { items?: ReadonlyArray<GrantRow> };
  return Array.isArray(parsed.items) ? parsed.items : [];
}

async function approvals(ctx: AuthContext, query: string): Promise<ReadonlyArray<GateRow>> {
  const res = await fetch(
    `${ctx.apiUrl}/v1/workspaces/${encodeURIComponent(ctx.workspaceId)}/approvals?${query}`,
    {
      headers: { Authorization: `Bearer ${ctx.token}` },
      signal: AbortSignal.timeout(HTTP_TIMEOUT_MS),
    },
  );
  if (!res.ok) {
    const detail = await res.text().catch(() => "");
    throw new Error(`approvals read ${res.status}: ${detail.slice(0, 200)}`);
  }
  const body = (await res.json()) as { items?: ReadonlyArray<GateRow> };
  return Array.isArray(body.items) ? body.items : [];
}

/** The one unanswered integration-grant card a fleet's request raised, or null
 * while the fleet's first run has not asked yet. */
export async function pendingGateFor(
  ctx: AuthContext,
  fleetId: string,
): Promise<GateRow | null> {
  const query = new URLSearchParams({
    fleet_id: fleetId,
    gate_type: KIND_INTEGRATION_GRANT,
    status: GATE_STATUS.pending,
  });
  const items = await approvals(ctx, query.toString());
  return items[0] ?? null;
}



/** The card the fleet's `request_access` raises, once its first run asks. */
export async function waitForPendingGate(
  ctx: AuthContext,
  fleetId: string,
  deadline: number,
): Promise<GateRow> {
  while (Date.now() < deadline) {
    const card = await pendingGateFor(ctx, fleetId);
    if (card) return card;
    await Bun.sleep(GATE_POLL_MS);
  }
  throw new Error(`fleet ${fleetId} raised no access request before the deadline`);
}
