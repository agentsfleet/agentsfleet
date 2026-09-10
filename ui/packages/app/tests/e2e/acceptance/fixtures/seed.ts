/**
 * Idempotent fixture seeding helpers.
 *
 * Fixture rows are conceptually tagged with `x-test-fixture: true` for
 * cleanup discrimination. agentsfleetd does not
 * currently read that header, but each fixture user has its own dedicated
 * tenant + workspace — every fleet in that workspace is a fixture row by
 * construction. Per-spec cleanup deletes everything in the fixture user's
 * workspace; no extra discriminator needed today.
 */
import type { ClientHandle } from "./api-client";
import type { FixtureKey, FleetStatus } from "./constants";
import { SOURCE_KIND_UPLOAD } from "@/lib/types";
import { clientFor } from "./api-client";

const FIXTURE_LIBRARY_NAME = "acceptance-seed";

export interface Workspace {
  id: string;
  name: string | null;
}

export interface Fleet {
  id: string;
  name: string;
  status?: FleetStatus;
}

interface ListResp<T> {
  items: T[];
  total: number;
  next_cursor?: string | null;
}

function handleLabel(handle: ClientHandle): string {
  return typeof handle === "string" ? handle : "ephemeral-jwt";
}

// Widened to ClientHandle so the ephemeral signup-flow user (whose JWT is
// minted mid-test and is NOT in the .fixture-jwts.json cache) can drive
// the lookup the same way persistent fixtures do.
export async function getDefaultWorkspaceId(handle: ClientHandle): Promise<string> {
  const c = clientFor(handle);
  const res = await c.get<ListResp<Workspace>>("/v1/tenants/me/workspaces");
  if (res.items.length === 0) {
    throw new Error(
      `Fixture user '${handleLabel(handle)}' has no workspace; bootstrap step must have failed.`,
    );
  }
  return res.items[0]!.id;
}

// The ONE trigger fixture for the whole acceptance tree. The daemon's importer
// requires name, triggers, tools, and budget in TRIGGER.md frontmatter
// (fleet_runtime/config_parser.zig) — a spec-local copy that drifts from that
// set fails every install with UZ-BUNDLE-001, which is why no spec defines its
// own (pinned by seed.test.ts).
export function triggerMd(name: string): string {
  // Use cron here so seeded fleets keep a concrete wake rule.
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
    "  budget:",
    "    daily_dollars: 1.0",
    "---",
    "",
  ].join("\n");
}

// A TRIGGER.md woken by a signed GitHub delivery rather than a cron: the
// per-fleet webhook route measures a delivery against the events listed here
// and verifies it with the workspace secret `credentialName` names. Same
// required keys as `triggerMd` (name, triggers, tools, budget), so the
// importer accepts it for the same reason.
export const WEBHOOK_SOURCE_GITHUB = "github";
export const WEBHOOK_EVENT_WORKFLOW_RUN = "workflow_run";

export function webhookTriggerMd(name: string, credentialName: string): string {
  return [
    "---",
    `name: ${name}`,
    "",
    "x-agentsfleet:",
    "  triggers:",
    "    - type: webhook",
    `      source: ${WEBHOOK_SOURCE_GITHUB}`,
    "      events:",
    `        - ${WEBHOOK_EVENT_WORKFLOW_RUN}`,
    `      credential_name: ${credentialName}`,
    "  tools:",
    "    - agentmail",
    "  budget:",
    "    daily_dollars: 1.0",
    "---",
    "",
  ].join("\n");
}

export function skillMd(name: string): string {
  // SKILL.md frontmatter requires name (kebab), description, version (semver).
  // Mirrors tests/fixtures/fleetbundle/skill/name_mismatch/SKILL.md.
  return [
    "---",
    `name: ${name}`,
    "description: Fixture skill body for e2e tests; echoes inputs, no side effects.",
    "version: 0.1.0",
    "---",
    "",
    `# ${name}`,
    "",
    "Body for fixture fleet used by e2e harness.",
    "",
  ].join("\n");
}

// Same frontmatter as skillMd, body deliberately EMPTY: the runner refuses to
// execute an instruction-less skill as a generic chat, so every delivery fails
// closed at startup — the one deterministic, model-free way to place a failed
// lease on a runner from the outside.
export function emptyBodySkillMd(name: string): string {
  return [
    "---",
    `name: ${name}`,
    "description: Fixture skill with an empty body; every delivery fails its startup check.",
    "version: 0.1.0",
    "---",
    "",
  ].join("\n");
}

// Same frontmatter again, body a REAL instruction: the one bundle in this tree
// whose delivery is meant to reach the provider and come back with an answer.
// Deterministic in shape (one line, no tools) so the journey can assert that a
// reply exists without asserting what a model chose to say.
export const EXECUTION_REPLY_PREFIX = "ACK";

export function executionSkillMd(name: string): string {
  return [
    "---",
    `name: ${name}`,
    "description: Acceptance probe; answers every message with one acknowledging line.",
    "version: 0.1.0",
    "---",
    "",
    `# ${name}`,
    "",
    "You are an acceptance probe. When you receive a message, reply with exactly",
    `one line: the word ${EXECUTION_REPLY_PREFIX}, a space, then the message text verbatim.`,
    "Do not call any tool. Do not add anything before or after that line.",
    "",
  ].join("\n");
}

export interface SeedFleetOpts {
  name: string;
}

interface CreateFleetResp {
  fleet_id: string;
  name: string;
  status: string;
}

interface OnboardTemplateResp {
  id: string;
}

async function onboardFixtureLibrary(
  client: ReturnType<typeof clientFor>,
  workspaceId: string,
): Promise<string> {
  const resp = await client.post<OnboardTemplateResp>(
    `/v1/workspaces/${workspaceId}/fleet-libraries`,
    {
      source_kind: SOURCE_KIND_UPLOAD,
      skill_markdown: skillMd(FIXTURE_LIBRARY_NAME),
      trigger_markdown: triggerMd(FIXTURE_LIBRARY_NAME),
    },
  );
  return resp.id;
}

export async function seedFleet(
  key: FixtureKey,
  workspaceId: string,
  opts: SeedFleetOpts,
): Promise<Fleet> {
  const c = clientFor(key);
  const tenantLibraryId = await onboardFixtureLibrary(c, workspaceId);
  // create_fleet returns `fleet_id`; list_fleets items have `id`. Normalize
  // to the listing shape so callers can compare against listFleets output.
  const resp = await c.post<CreateFleetResp>(`/v1/workspaces/${workspaceId}/fleets`, {
    tenant_library_id: tenantLibraryId,
    name: opts.name,
  });
  return { id: resp.fleet_id, name: resp.name };
}

/**
 * Poll until the fleet is `active`. `seedFleet` returns on the create response,
 * but the detail page hides its working surfaces behind the install gate until
 * the status flips — a spec that navigates immediately lands on the gate, not
 * the fleet. Waiting for `active` specifically (not merely "not installing")
 * means a fleet that FAILED, was killed, or paused fails the wait loudly
 * instead of passing the suite onto a fleet that can never render a console.
 */
export async function waitForFleetActive(
  handle: ClientHandle,
  workspaceId: string,
  fleetId: string,
  timeoutMs = 30_000,
): Promise<void> {
  const c = clientFor(handle);
  const deadline = Date.now() + timeoutMs;
  for (;;) {
    const fleet = await c.get<{ status: string }>(
      `/v1/workspaces/${workspaceId}/fleets/${fleetId}`,
    );
    if (fleet.status === "active") return;
    if (fleet.status !== "installing") {
      throw new Error(
        `[e2e:seed] fleet ${fleetId} reached terminal status "${fleet.status}" before active`,
      );
    }
    if (Date.now() > deadline) {
      throw new Error(
        `[e2e:seed] fleet ${fleetId} still installing after ${timeoutMs}ms`,
      );
    }
    await new Promise((resolve) => setTimeout(resolve, 500));
  }
}

/** The name the server stored for a fleet — the template's, or a suffixed one. */
export async function readFleetName(
  handle: ClientHandle,
  workspaceId: string,
  fleetId: string,
): Promise<string> {
  const fleet = await clientFor(handle).get<{ name: string }>(
    `/v1/workspaces/${workspaceId}/fleets/${fleetId}`,
  );
  return fleet.name;
}

/** The fleet's own lifetime counters, in the units the daemon stores them in.
 * The wall tile renders these two, spend rounded to cents — so a walk grading
 * the tile reads them here, in nanos, and lets the tile do its own rounding. */
export interface FleetCounters {
  budget_used_nanos: number;
  events_processed: number;
}

export async function readFleetCounters(
  handle: ClientHandle,
  workspaceId: string,
  fleetId: string,
): Promise<FleetCounters> {
  return clientFor(handle).get<FleetCounters>(
    `/v1/workspaces/${workspaceId}/fleets/${fleetId}`,
  );
}

export async function listFleets(handle: ClientHandle, workspaceId: string): Promise<Fleet[]> {
  const c = clientFor(handle);
  const res = await c.get<ListResp<Fleet>>(`/v1/workspaces/${workspaceId}/fleets`);
  return res.items;
}

export async function listWorkspaces(key: FixtureKey): Promise<Workspace[]> {
  const c = clientFor(key);
  const workspaces: Workspace[] = [];
  const seenCursors = new Set<string>();
  let startingAfter: string | null = null;
  do {
    const query = new URLSearchParams({ limit: "100" });
    if (startingAfter) query.set("starting_after", startingAfter);
    const page = await c.get<ListResp<Workspace>>(
      `/v1/tenants/me/workspaces?${query.toString()}`,
    );
    workspaces.push(...page.items);
    startingAfter = page.next_cursor ?? null;
    if (startingAfter !== null && seenCursors.has(startingAfter)) {
      throw new Error("Workspace pagination repeated a cursor");
    }
    if (startingAfter !== null) seenCursors.add(startingAfter);
  } while (startingAfter !== null);
  return workspaces;
}

interface CreateWorkspaceResp {
  workspace_id: string;
  name: string;
}

// POST /v1/workspaces — name is optional; server picks a Heroku-style name
// when omitted. Used by multi-workspace.spec.ts to ensure the fixture user
// has at least two workspaces for the WorkspaceSwitcher dropdown.
export async function ensureSecondWorkspace(
  key: FixtureKey,
  desiredName: string,
): Promise<Workspace> {
  const existing = await listWorkspaces(key);
  const match = existing.find((w) => (w.name ?? "") === desiredName);
  if (match) return match;
  const c = clientFor(key);
  const resp = await c.post<CreateWorkspaceResp>("/v1/workspaces", { name: desiredName });
  return { id: resp.workspace_id, name: resp.name };
}
