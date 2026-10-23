/**
 * grants.ts — the wire side of the integration-grant walk.
 *
 * The name-bound Linear handle requires explicit integration approval.
 * These fleets have no tools, so its placeholder refresh value is never sent
 * upstream. The journey proves approval, delivery and billing, not OAuth refresh.
 */
import { clientFor, type ClientHandle } from "./api-client";
import type { ApprovalGate, ApprovalsListResponse } from "@/lib/api/approvals";
import { APPROVAL_STATUS } from "@/lib/api/approvals-types";
import { NANOS_PER_USD, type TenantBilling, type TenantBillingChargesResponse } from "@/lib/types";

/**
 * The gate kind the daemon raises for a mintable credential.
 * Cross-runtime pair of `afd_approval::KIND_INTEGRATION_GRANT`; the approve
 * statement joins on it, so a re-spelling here would silently match no row.
 */
export const KIND_INTEGRATION_GRANT = "integration_grant";

export const CONNECTOR_SERVICE_LINEAR = "linear";

/** The vault-handle field that names the connector. Mirrors
 * `afd_credential::secrets::connector::FIELD_INTEGRATION`. */
const FIELD_INTEGRATION = "integration";
const FIELD_REFRESH_TOKEN = "refresh_token";

/** The card's `evidence` key naming the third party it is about. Mirrors
 * `afd_approval::request::EVIDENCE_SERVICE`, which is also what
 * `RESOLVE_GATE` joins the grant row on. */
const EVIDENCE_SERVICE = "service";

// Vault admission requires the handle's name to match its connector.
// Each journey owns a dedicated workspace and removes this synthetic handle.
export const GRANT_CREDENTIAL_NAME = CONNECTOR_SERVICE_LINEAR;

const SECRETS_PATH = (workspaceId: string) => `/v1/workspaces/${workspaceId}/secrets`;
const APPROVALS_PATH = (workspaceId: string) => `/v1/workspaces/${workspaceId}/approvals`;
const GRANTS_PATH = (workspaceId: string, fleetId: string) =>
  `/v1/workspaces/${workspaceId}/fleets/${fleetId}/integration-grants`;
const TENANT_BILLING_PATH = "/v1/tenants/me/billing";
const TENANT_CHARGES_PATH = `${TENANT_BILLING_PATH}/charges`;

/** One page of the ledger is enough: the walk reads it seconds after its own
 * runs settle, and the fixture tenant's older rows are another run's evidence. */
const CHARGES_PAGE_LIMIT = 100;

/**
 * Store a synthetic handle only in this journey's dedicated workspace.
 * A prior interrupted run may have left it there; teardown removes it after
 * every fleet that references it has been purged.
 */
export async function ensureConnectorHandle(
  handle: ClientHandle,
  workspaceId: string,
): Promise<void> {
  const client = clientFor(handle);
  try {
    await client.post(SECRETS_PATH(workspaceId), {
      name: GRANT_CREDENTIAL_NAME,
      data: { [FIELD_INTEGRATION]: CONNECTOR_SERVICE_LINEAR, [FIELD_REFRESH_TOKEN]: CONNECTOR_SERVICE_LINEAR },
    });
    return;
  } catch (refused) {
    const stored = await client.get<{ secrets: { name: string }[] }>(SECRETS_PATH(workspaceId));
    if (!stored.secrets.some((secret) => secret.name === GRANT_CREDENTIAL_NAME)) throw refused;
  }
}

/**
 * Native setup with one workspace credential requirement for the grant journey.
 */
export function connectorConfig(name: string) {
  return { name, "x-agentsfleet": { triggers: [], tools: [],
    credentials: [GRANT_CREDENTIAL_NAME], budget: { daily_nanos: String(NANOS_PER_USD) } } };
}

/** Every gate the workspace holds for one fleet, newest first. */
export async function listGatesForFleet(
  handle: ClientHandle,
  workspaceId: string,
  fleetId: string,
): Promise<ApprovalGate[]> {
  const query = new URLSearchParams({ fleet_id: fleetId, gate_type: KIND_INTEGRATION_GRANT });
  const page = await clientFor(handle).get<ApprovalsListResponse>(
    `${APPROVALS_PATH(workspaceId)}?${query.toString()}`,
  );
  return page.items;
}

/**
 * The one card a fleet's install raised and nobody has answered, or `null`
 * while the install-time request is still in flight.
 *
 * Singular on purpose: the daemon allows one actionable card per
 * (fleet, service), so a second pending row for one fleet is a defect the
 * caller should see rather than a list to pick from.
 */
export async function pendingGateFor(
  handle: ClientHandle,
  workspaceId: string,
  fleetId: string,
): Promise<ApprovalGate | null> {
  const pending = (await listGatesForFleet(handle, workspaceId, fleetId)).filter(
    (gate) => gate.status === APPROVAL_STATUS.PENDING,
  );
  if (pending.length === 0) return null;
  if (pending.length > 1) {
    throw new Error(
      `fleet ${fleetId} holds ${pending.length} pending ${KIND_INTEGRATION_GRANT} cards; ` +
        "the daemon permits one actionable card per (fleet, service)",
    );
  }
  return pending[0] ?? null;
}

/** Grant-row statuses, as `afd_api_wire::grant::status` spells them. */
export const GRANT_STATUS = {
  pending: "pending",
  approved: "approved",
  revoked: "revoked",
} as const;

/**
 * The provenance an install-time grant carries. Cross-runtime pair of
 * `afd_approval::request::REASON_DECLARED_AT_INSTALL`.
 */
export const REASON_DECLARED_AT_INSTALL = "Declared by the fleet's setup at install";

/** One standing permission the daemon checks before it mints. */
export interface IntegrationGrant {
  readonly id: string;
  readonly service: string;
  readonly status: string;
  readonly approved_at: string | null;
  readonly revoked_at: string | null;
  readonly reason: string | null;
}

/** Every integration grant the workspace holds for one fleet. */
export async function listGrantsForFleet(
  handle: ClientHandle,
  workspaceId: string,
  fleetId: string,
): Promise<IntegrationGrant[]> {
  const page = await clientFor(handle).get<{ items: IntegrationGrant[] }>(
    GRANTS_PATH(workspaceId, fleetId),
  );
  return page.items ?? [];
}

/**
 * The one standing grant an install wrote for the connector, or `null` while
 * the install-time request is still in flight.
 *
 * Singular for the reason `pendingGateFor` is: `uq_integration_grants_fleet_id_service`
 * permits one row per (fleet, service), so a second is a defect to surface.
 */
export async function connectorGrantFor(
  handle: ClientHandle,
  workspaceId: string,
  fleetId: string,
): Promise<IntegrationGrant | null> {
  const mine = (await listGrantsForFleet(handle, workspaceId, fleetId)).filter(
    (grant) => grant.service === CONNECTOR_SERVICE_LINEAR,
  );
  if (mine.length > 1) {
    throw new Error(
      `fleet ${fleetId} holds ${mine.length} ${CONNECTOR_SERVICE_LINEAR} grants; ` +
        "uq_integration_grants_fleet_id_service permits one",
    );
  }
  return mine[0] ?? null;
}

/** One gate by id — how a walk reads back what the browser just decided. */
export async function readGate(
  handle: ClientHandle,
  workspaceId: string,
  gateId: string,
): Promise<ApprovalGate> {
  return clientFor(handle).get<ApprovalGate>(`${APPROVALS_PATH(workspaceId)}/${gateId}`);
}

/** The service an integration-grant card names, as the approve statement reads
 * it: `evidence->>'service'`. A card without it resolves and moves no grant. */
export function serviceOn(gate: ApprovalGate): string | null {
  const service = gate.evidence[EVIDENCE_SERVICE];
  return typeof service === "string" ? service : null;
}

/** The tenant's credit balance, in nanos. The figure the dashboard's billing
 * card renders — read here in nanos, because the renderer rounds to cents. */
export async function readTenantBilling(handle: ClientHandle): Promise<TenantBilling> {
  return clientFor(handle).get<TenantBilling>(TENANT_BILLING_PATH);
}

/**
 * What the tenant was charged for these fleets, summed from the ledger.
 *
 * The balance alone cannot attribute a fall: the fixture tenant is shared
 * across parallel workers, so a sibling spec's run moves it too. The ledger
 * carries `fleet_id` per row, which is the only read that says THIS fleet
 * spent money.
 */
export async function chargedToFleets(
  handle: ClientHandle,
  fleetIds: readonly string[],
): Promise<bigint> {
  const wanted = new Set(fleetIds);
  const page = await clientFor(handle).get<TenantBillingChargesResponse>(
    `${TENANT_CHARGES_PATH}?limit=${CHARGES_PAGE_LIMIT}`,
  );
  return page.items
    .filter((row) => row.fleet_id !== null && wanted.has(row.fleet_id))
    .reduce((total, row) => total + BigInt(row.credit_deducted_nanos), 0n);
}
