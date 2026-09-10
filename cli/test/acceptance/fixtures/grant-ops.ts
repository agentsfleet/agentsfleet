/**
 * grant-ops.ts — the integration-grant surface, from the CLI's side of it.
 *
 * The CLI can raise a grant (by installing a bundle that declares a mintable
 * credential) and READ one (`agentsfleet grant list --fleet <id>`), but it has
 * no verb that answers one: `grant` ships `list` and `delete` only. So the
 * decision is made over HTTP here, against the same workspace approvals route
 * the dashboard's inbox posts to, carrying the run's own bearer — the pattern
 * `template-ops.ts` and `secret-ops.ts` already use for surfaces the CLI does
 * not cover.
 *
 * That gap is the finding this module documents rather than hides: an operator
 * living in the terminal can see the card and cannot answer it.
 *
 * The seeded handle is a CLASSIFICATION marker, not a credential. It carries
 * `integration: <connector>` and nothing else, which is all
 * `afd_credential::secrets::connector::mintable` reads — the fleets here call
 * no tool, so nothing ever mints against it.
 */

import { runFleetctl } from "./cli.js";
import type { AuthContext } from "./template-ops.ts";

/**
 * The gate kind the daemon raises for a mintable credential. Cross-runtime pair
 * of `afd_approval::KIND_INTEGRATION_GRANT` and of the dashboard acceptance
 * suite's constant of the same name.
 */
export const KIND_INTEGRATION_GRANT = "integration_grant";

/** The connector asked for, as `Connector::name()` spells it. */
export const CONNECTOR_SERVICE_GITHUB = "github";

/**
 * The credential the connector bundle declares, and the vault name its handle
 * is stored under. Shared verbatim with the dashboard acceptance suite: both
 * lanes run against one fixture tenant, and both write the identical body, so
 * one name is correct where two would be two things to reason about.
 *
 * Deliberately not `github`: that name holds the static
 * `{webhook_secret, api_token}` pair the platform-ops fixture seeds, and a
 * static handle declares nothing to mint.
 *
 * Underscores, not hyphens: `CredentialName::parse` accepts ASCII alphanumerics
 * and `_` only, and a hyphen refuses the whole bundle as UZ-BUNDLE-001.
 */
export const GRANT_CREDENTIAL_NAME = "grant_walk_github";

/** The vault-handle field naming the connector. Mirrors
 * `afd_credential::secrets::connector::FIELD_INTEGRATION`. */
const FIELD_INTEGRATION = "integration";

/** Grant-row statuses, as `afd_wire::grant::status` spells them. */
export const GRANT_STATUS = {
  pending: "pending",
  approved: "approved",
  revoked: "revoked",
} as const;

/** Gate statuses, as `afd_wire::approval::status` spells them. */
export const GATE_STATUS = {
  pending: "pending",
  approved: "approved",
} as const;

const APPROVE_DECISION = "approve";
const SECRET_CREATE_TIMEOUT_MS = 30_000;
const HTTP_TIMEOUT_MS = 30_000;

export interface GrantRow {
  readonly id?: string | null;
  readonly service?: string | null;
  readonly status?: string | null;
  readonly approved_at?: number | string | null;
}

export interface GateRow {
  readonly gate_id: string;
  readonly fleet_id: string;
  readonly gate_kind: string;
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
  const data = JSON.stringify({ [FIELD_INTEGRATION]: CONNECTOR_SERVICE_GITHUB });
  const result = await runFleetctl(
    ["secret", "create", GRANT_CREDENTIAL_NAME, "--data", data, "--json"],
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
  const result = await runFleetctl(["grant", "list", "--fleet", fleetId, "--json"], {
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

/** The one unanswered integration-grant card a fleet's install raised, or null
 * while the install-time request is still in flight. */
export async function pendingGateFor(
  ctx: AuthContext,
  fleetId: string,
): Promise<GateRow | null> {
  const query = new URLSearchParams({
    fleet_id: fleetId,
    gate_kind: KIND_INTEGRATION_GRANT,
    status: GATE_STATUS.pending,
  });
  const items = await approvals(ctx, query.toString());
  return items[0] ?? null;
}

/** Answer a card yes, and hand back what the daemon recorded. */
export async function approveGate(ctx: AuthContext, gateId: string): Promise<GateRow> {
  const res = await fetch(
    `${ctx.apiUrl}/v1/workspaces/${encodeURIComponent(ctx.workspaceId)}/approvals/` +
      `${encodeURIComponent(gateId)}/${APPROVE_DECISION}`,
    {
      method: "POST",
      headers: {
        Authorization: `Bearer ${ctx.token}`,
        "Content-Type": "application/json",
      },
      body: "{}",
      signal: AbortSignal.timeout(HTTP_TIMEOUT_MS),
    },
  );
  if (!res.ok) {
    const detail = await res.text().catch(() => "");
    throw new Error(`approve ${gateId} → ${res.status}: ${detail.slice(0, 200)}`);
  }
  return (await res.json()) as GateRow;
}
