/**
 * Release-readiness probes — the preflight group's shared surface.
 *
 * Every probe is a read: deployed service liveness (/healthz + /readyz),
 * runtime-model availability, connector-registry configuration, runner
 * liveness, and the built CLI artifact. Deciders are pure functions over
 * already-fetched values so the absent-prerequisite behavior is unit-testable
 * without a deployment; probes are thin GET wrappers around them.
 *
 * A failed probe throws ReleasePreflightError carrying an operator-facing
 * diagnosis (error code + recovery playbook + request id) — never a raw
 * response-body echo, so fixture identities and credential material cannot
 * land in CI logs or uploaded artifacts.
 */
import * as fs from "node:fs";
import type { ConnectorCatalogEntry } from "@/lib/api/connectors";
import type { RunnerListResponse, RunnerLiveness } from "@/lib/api/runners";
import type { TenantModelEntryList } from "@/lib/types";
import { clientFor } from "./api-client";
import { AGENTSFLEET_CLI_ENTRY, CLI_ARTIFACT_MISSING_DIAGNOSIS } from "./cli-runner";
import { FIXTURE_KEY } from "./constants";
import { getDefaultWorkspaceId } from "./seed";

const PROBE_TIMEOUT_MS = 10_000;
const HEALTHZ_PATH = "/healthz";
const READYZ_PATH = "/readyz";
const TENANT_MODELS_PATH = "/v1/tenants/me/models";
const FLEET_RUNNERS_PATH = "/v1/fleets/runners";

// A runner doing work is as alive as an idle one — either satisfies the gate.
const LIVE_RUNNER_STATES: readonly RunnerLiveness[] = ["online", "busy"];

// agentsfleetd error codes the preflight can translate into a recovery step.
// Values are operator guidance, deliberately free of any response-body echo.
const DIAGNOSIS_BY_ERROR_CODE: Readonly<Record<string, string>> = {
  "UZ-SCHED-007":
    "QStash schedule credentials are not seeded in this environment's platform-admin " +
    "workspace vault. Run playbooks/operations/qstash_registration/001_playbook.md, " +
    "then roll agentsfleetd.",
  "UZ-CONN-001":
    "A connector provider bag is missing platform-side. Re-run the provider " +
    "registration playbook under playbooks/operations/ for the failing provider.",
  "UZ-AUTH-002":
    "The fixture bearer token was rejected. Re-run global setup so fixture " +
    "sessions are re-minted, and check the Clerk instance the deployment trusts.",
};

interface ApiErrorBody {
  error_code?: string;
  request_id?: string;
}

export class ReleasePreflightError extends Error {
  constructor(message: string) {
    super(`[release-preflight] ${message}`);
    this.name = "ReleasePreflightError";
  }
}

function apiBase(): string {
  const url = process.env.NEXT_PUBLIC_API_URL;
  if (!url) throw new ReleasePreflightError("NEXT_PUBLIC_API_URL must be set");
  return url;
}

// ── Pure deciders (unit-tested without a deployment) ─────────────────────────

export function assertServiceHealthy(healthzStatus: number, readyzReady: boolean): void {
  if (healthzStatus !== 200) {
    throw new ReleasePreflightError(
      `deployed service is not live: ${HEALTHZ_PATH} returned ${healthzStatus}`,
    );
  }
  if (!readyzReady) {
    throw new ReleasePreflightError(
      `deployed service dependencies are not ready: ${READYZ_PATH} reports ready=false ` +
        "(database or queue is down)",
    );
  }
}

export function assertRuntimeModelAvailable(list: Pick<TenantModelEntryList, "platform_default_available">): void {
  if (!list.platform_default_available) {
    throw new ReleasePreflightError(
      "no platform default model is available to tenants. Register the platform " +
        "key via the admin bootstrap playbook before running user journeys.",
    );
  }
}

export function assertConnectorConfigured(entries: readonly ConnectorCatalogEntry[]): void {
  if (entries.length === 0) {
    throw new ReleasePreflightError("the connector registry returned no providers");
  }
  if (!entries.some((entry) => entry.configured)) {
    throw new ReleasePreflightError(
      "no connector provider is configured platform-side. Provider app bags are " +
        "missing from this environment's deployment inputs.",
    );
  }
}

export function assertRunnerOnline(runners: Pick<RunnerListResponse, "items">): void {
  const live = runners.items.some((item) => LIVE_RUNNER_STATES.includes(item.liveness));
  if (!live) {
    throw new ReleasePreflightError(
      "no runner is online or busy. Fleets cannot execute; check the runner " +
        "service and its heartbeat before running user journeys.",
    );
  }
}

export function assertCliArtifactPresent(exists: boolean = fs.existsSync(AGENTSFLEET_CLI_ENTRY)): void {
  if (!exists) throw new ReleasePreflightError(CLI_ARTIFACT_MISSING_DIAGNOSIS);
}

/**
 * Translate an API-client failure into the typed preflight diagnosis. The
 * api-client message embeds the response body after the status marker; only
 * the parsed error_code/request_id survive into the diagnosis — the body
 * itself is dropped (redaction boundary).
 */
export function diagnoseApiError(error: unknown, probeName: string): ReleasePreflightError {
  const message = error instanceof Error ? error.message : String(error);
  const body = parseErrorBody(message);
  const hint = body.error_code ? DIAGNOSIS_BY_ERROR_CODE[body.error_code] : undefined;
  // Everything from the first "{" on is response body — dropped even when the
  // message STARTS with the body, or the redaction boundary leaks.
  const jsonStart = message.indexOf("{");
  const statusLine = (jsonStart >= 0 ? message.slice(0, jsonStart) : message).trim();
  const parts = [
    `${probeName} failed: ${statusLine}`,
    body.error_code ? `error_code=${body.error_code}` : null,
    body.request_id ? `request_id=${body.request_id}` : null,
    hint ?? null,
  ].filter((part): part is string => part !== null);
  return new ReleasePreflightError(parts.join(" | "));
}

function parseErrorBody(message: string): ApiErrorBody {
  const jsonStart = message.indexOf("{");
  if (jsonStart < 0) return {};
  try {
    const parsed: unknown = JSON.parse(message.slice(jsonStart));
    if (typeof parsed !== "object" || parsed === null) return {};
    const record = parsed as Record<string, unknown>;
    return {
      error_code: typeof record.error_code === "string" ? record.error_code : undefined,
      request_id: typeof record.request_id === "string" ? record.request_id : undefined,
    };
  } catch {
    return {};
  }
}

// ── Probes (thin GET wrappers; the enclosing test owns overall cancellation) ─

async function fetchWithTimeout(url: string): Promise<Response> {
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), PROBE_TIMEOUT_MS);
  try {
    return await fetch(url, { signal: controller.signal });
  } finally {
    clearTimeout(timer);
  }
}

export async function probeDeployedService(): Promise<void> {
  const base = apiBase();
  const healthz = await fetchWithTimeout(`${base}${HEALTHZ_PATH}`);
  const readyz = await fetchWithTimeout(`${base}${READYZ_PATH}`);
  const readyBody = (await readyz.json().catch(() => ({ ready: false }))) as { ready?: boolean };
  assertServiceHealthy(healthz.status, readyz.ok && readyBody.ready === true);
}

export async function probeRuntimeModel(): Promise<void> {
  try {
    const list = await clientFor(FIXTURE_KEY.regular).get<TenantModelEntryList>(TENANT_MODELS_PATH);
    assertRuntimeModelAvailable(list);
  } catch (error) {
    if (error instanceof ReleasePreflightError) throw error;
    throw diagnoseApiError(error, "runtime-model probe");
  }
}

export async function probeConnectorRegistry(): Promise<void> {
  try {
    const workspaceId = await getDefaultWorkspaceId(FIXTURE_KEY.regular);
    const entries = await clientFor(FIXTURE_KEY.regular).get<ConnectorCatalogEntry[]>(
      `/v1/workspaces/${workspaceId}/connectors`,
    );
    assertConnectorConfigured(entries);
  } catch (error) {
    if (error instanceof ReleasePreflightError) throw error;
    throw diagnoseApiError(error, "connector-registry probe");
  }
}

export async function probeRunnerOnline(): Promise<void> {
  try {
    const runners = await clientFor(FIXTURE_KEY.operator).get<RunnerListResponse>(FLEET_RUNNERS_PATH);
    assertRunnerOnline(runners);
  } catch (error) {
    if (error instanceof ReleasePreflightError) throw error;
    throw diagnoseApiError(error, "runner-liveness probe");
  }
}
