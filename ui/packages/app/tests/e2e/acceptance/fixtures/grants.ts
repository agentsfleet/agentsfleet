/**
 * grants.ts — the wire side of the integration-grant walk.
 *
 * An install raises a pending grant card for every credential the bundle
 * declares that the workspace holds as a CONNECTOR handle. "Connector handle"
 * is the whole trick: the daemon classifies a stored secret as mintable only
 * when its body is an object carrying `integration: "<connector>"`
 * (`afd_credential/src/secrets/connector.rs::mintable`). A static token stored
 * under the same name declares nothing and raises no card — which is why the
 * platform-ops fixture's `github` secret, a `{webhook_secret, api_token}` pair,
 * has never produced one.
 *
 * So this module seeds a handle the classifier will accept, under a name no
 * other fixture claims, and hands the walk the trigger frontmatter that
 * declares it. Everything else here is a read: the card the install raised, the
 * grant row behind it, and the tenant's credit balance and ledger.
 *
 * The grant is never MINTED against. The walk's fleets call no tool, so the
 * handle is only ever classified — the credential a real connect would store is
 * not needed to prove that a card is raised, answered, and unparks a delivery.
 */
import { clientFor, type ClientHandle } from "./api-client";
import type { ApprovalGate, ApprovalsListResponse } from "@/lib/api/approvals";
import { APPROVAL_STATUS } from "@/lib/api/approvals-types";
import type { TenantBilling, TenantBillingChargesResponse } from "@/lib/types";

/**
 * The gate kind the daemon raises for a mintable credential.
 * Cross-runtime pair of `afd_approval::KIND_INTEGRATION_GRANT`; the approve
 * statement joins on it, so a re-spelling here would silently match no row.
 */
export const KIND_INTEGRATION_GRANT = "integration_grant";

/**
 * The connector the walk asks for, as `Connector::name()` spells it in
 * `afd_credential`'s `DECLARED` registry. GitHub's exchange is a GitHub App
 * JWT, which makes its supply on-demand — the one property `mintable()` filters
 * on, and therefore the reason this connector and not an inline one.
 */
export const CONNECTOR_SERVICE_GITHUB = "github";

/** The vault-handle field that names the connector. Mirrors
 * `afd_credential::secrets::connector::FIELD_INTEGRATION`. */
const FIELD_INTEGRATION = "integration";

/** The card's `evidence` key naming the third party it is about. Mirrors
 * `afd_approval::request::EVIDENCE_SERVICE`, which is also what
 * `RESOLVE_GATE` joins the grant row on. */
const EVIDENCE_SERVICE = "service";

/**
 * The credential name the walk's bundles declare, and the vault name the
 * handle is stored under.
 *
 * Deliberately NOT `github`. The CLI acceptance lane seeds a static `github`
 * secret into the SAME fixture tenant on api-dev (`platform-secrets.ts`), and
 * the two lanes run as separate CI jobs against one deployment — a shared name
 * means whichever ran last decides whether any card is raised at all.
 *
 * Underscores, not hyphens: `CredentialName::parse` accepts ASCII alphanumerics
 * and `_` only, and a hyphen refuses the whole bundle as UZ-BUNDLE-001 — an
 * error whose message is about a missing SKILL.md and says nothing about the
 * name that actually broke it.
 */
export const GRANT_CREDENTIAL_NAME = "grant_walk_github";

const SECRETS_PATH = (workspaceId: string) => `/v1/workspaces/${workspaceId}/secrets`;
const APPROVALS_PATH = (workspaceId: string) => `/v1/workspaces/${workspaceId}/approvals`;
const TENANT_BILLING_PATH = "/v1/tenants/me/billing";
const TENANT_CHARGES_PATH = `${TENANT_BILLING_PATH}/charges`;

/** One page of the ledger is enough: the walk reads it seconds after its own
 * runs settle, and the fixture tenant's older rows are another run's evidence. */
const CHARGES_PAGE_LIMIT = 100;

/**
 * Store the connector handle the walk's bundles declare, and leave it stored.
 *
 * Claim-then-confirm rather than delete-then-create: the name is exclusively
 * this walk's, so every writer of it wants the identical body, and a refused
 * `POST` means somebody already wrote it. Deleting first would open a window in
 * which a parallel worker's install cannot resolve the credential it declared —
 * and an unresolvable declaration FAILS the delivery instead of parking it,
 * which is the one outcome that would make this walk lie.
 *
 * The handle outlives the run on purpose. It is a classification marker with no
 * credential in it, and re-storing it per run would race the installs that read
 * it.
 */
export async function ensureConnectorHandle(
  handle: ClientHandle,
  workspaceId: string,
): Promise<void> {
  const client = clientFor(handle);
  try {
    await client.post(SECRETS_PATH(workspaceId), {
      name: GRANT_CREDENTIAL_NAME,
      data: { [FIELD_INTEGRATION]: CONNECTOR_SERVICE_GITHUB },
    });
    return;
  } catch (refused) {
    const stored = await client.get<{ secrets: { name: string }[] }>(SECRETS_PATH(workspaceId));
    if (!stored.secrets.some((secret) => secret.name === GRANT_CREDENTIAL_NAME)) throw refused;
  }
}

/**
 * TRIGGER.md frontmatter declaring the connector credential.
 *
 * Same required key set as `seed.ts:triggerMd` — the daemon's importer refuses
 * a bundle missing name, triggers, tools or budget — plus the `credentials:`
 * block that is the whole point. The cron is the daily one every browser
 * fixture uses, so a leaked fleet wakes runners no more often than the rest.
 */
export function connectorTriggerMd(name: string): string {
  return [
    "---",
    `name: ${name}`,
    "",
    "x-agentsfleet:",
    "  triggers:",
    "    - type: cron",
    '      schedule: "0 0 * * *"',
    "  tools:",
    "    - agentmail",
    "  credentials:",
    `    - ${GRANT_CREDENTIAL_NAME}`,
    "  budget:",
    "    daily_dollars: 1.0",
    "---",
    "",
  ].join("\n");
}

/** Every gate the workspace holds for one fleet, newest first. */
export async function listGatesForFleet(
  handle: ClientHandle,
  workspaceId: string,
  fleetId: string,
): Promise<ApprovalGate[]> {
  const query = new URLSearchParams({ fleet_id: fleetId, gate_kind: KIND_INTEGRATION_GRANT });
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
): Promise<number> {
  const wanted = new Set(fleetIds);
  const page = await clientFor(handle).get<TenantBillingChargesResponse>(
    `${TENANT_CHARGES_PATH}?limit=${CHARGES_PAGE_LIMIT}`,
  );
  return page.items
    .filter((row) => row.fleet_id !== null && wanted.has(row.fleet_id))
    .reduce((total, row) => total + row.credit_deducted_nanos, 0);
}
