/**
 * workspace-mutation slice — owned helpers.
 *
 * The shared `seed.ts` / `teardown.ts` fixtures cover fleet lifecycle only;
 * this file carries the workspace-create + scope-isolation primitives the
 * `workspace-mutation.spec.ts` round-trip needs and nothing else owns.
 *
 * Surface confirmed against `src/program/cli-tree.ts` +
 * `src/program/cli-tree-fleet.ts` + `src/commands/workspace.ts`:
 *   - `workspace create [name]`  → POST /v1/workspaces; --json → { workspace_id, name }
 *   - `workspace list --json` → { current_workspace_id, workspaces: [...] }
 *   - `workspace use <id>`    → --json → { active: <id> } (active carries the id)
 *   - `workspace show --json` → { workspace_id, active: <bool>, name, created_at }
 *   - `workspace delete <id>` → LOCAL store removal only (no server route);
 *                               --json → { removed_from_local_state: <id> }
 *   - `list --workspace-id <id> --json` → { items: [...] } (top-level fleet list)
 *
 * Hard constraint surfaced here (also in the spec header): the server
 * exposes no workspace-delete route, so every `workspace create` leaves a
 * permanent workspace in the shared DEV tenant. Names are
 * `ACCEPTANCE_RUN_PREFIX`-scoped so the residue is attributable; fleets
 * inside are still torn down via `cleanWorkspaceFleets`.
 *
 * Note the two `active` keys carry DIFFERENT shapes and so get DIFFERENT
 * named consts: `workspace use` returns the active id as a string
 * (`WS_USE_ACTIVE_KEY`), `workspace show` returns a boolean
 * (`WS_SHOW_ACTIVE_KEY`). They share the wire name `"active"` but the
 * callers assert against different value types.
 */

import assert from "node:assert/strict";

import { runFleetctl } from "./cli.js";
import type { RunResult } from "./cli.js";

type Env = Readonly<Record<string, string>>;

export const WS_LIST_ITEMS_KEY = "workspaces" as const;
export const WS_LIST_CURRENT_KEY = "current_workspace_id" as const;
export const WS_ID_KEY = "workspace_id" as const;
export const WS_SHOW_ACTIVE_KEY = "active" as const;
export const WS_USE_ACTIVE_KEY = "active" as const;
export const WORKSPACE_LOCAL_REMOVAL_FIELD = "removed_from_local_state" as const;
export const AGENT_LIST_ITEMS_KEY = "items" as const;
export const AGENT_ID_KEY = "fleet_id" as const;
export const AGENT_NAME_KEY = "name" as const;
export const FLAG_JSON = "--json" as const;
// Top-level `list` scopes to a workspace via `--workspace-id` (per
// cli-tree-fleet.ts line 55). NOT `--workspace` — that's the per-resource
// flag on fleet-key/grant subcommands.
export const FLAG_WORKSPACE_ID = "--workspace-id" as const;

export interface WorkspaceRow {
  readonly workspace_id?: string;
  readonly name?: string | null;
  readonly [key: string]: unknown;
}

export interface FleetRow {
  readonly id?: string;
  readonly fleet_id?: string;
  readonly name?: string;
  readonly status?: string;
  readonly workspace_id?: string;
  readonly [key: string]: unknown;
}

export interface AddedWorkspace {
  readonly workspaceId: string;
  readonly name: string | null;
}

function parseJson(result: RunResult, label: string): Record<string, unknown> {
  assert.equal(result.code, 0, `${label} exited ${result.code}: ${result.stderr || result.stdout}`);
  const trimmed = result.stdout.trim();
  assert.ok(trimmed.length > 0, `${label}: empty stdout`);
  return JSON.parse(trimmed) as Record<string, unknown>;
}

/** `workspace create <name>` → asserts a real `workspace_id`, returns it. */
export async function addWorkspace(env: Env, name: string): Promise<AddedWorkspace> {
  const result = await runFleetctl(["workspace", "create", name, FLAG_JSON], { env });
  const parsed = parseJson(result, `workspace create ${name}`);
  const workspaceId = parsed[WS_ID_KEY];
  assert.equal(typeof workspaceId, "string", `workspace create ${name}: missing ${WS_ID_KEY}: ${result.stdout}`);
  assert.ok((workspaceId as string).length > 0, `workspace create ${name}: empty ${WS_ID_KEY}`);
  const rawName = parsed[AGENT_NAME_KEY];
  return { workspaceId: workspaceId as string, name: typeof rawName === "string" ? rawName : null };
}

/** `workspace list --json` → the `workspaces` array (local store view). */
export async function listWorkspaces(env: Env): Promise<ReadonlyArray<WorkspaceRow>> {
  const result = await runFleetctl(["workspace", "list", FLAG_JSON], { env });
  const parsed = parseJson(result, "workspace list");
  const items = parsed[WS_LIST_ITEMS_KEY];
  assert.ok(Array.isArray(items), `workspace list: ${WS_LIST_ITEMS_KEY} not an array: ${result.stdout}`);
  return items as ReadonlyArray<WorkspaceRow>;
}

/** `workspace use <id>` then assert it became active locally. */
export async function useWorkspace(env: Env, workspaceId: string): Promise<void> {
  const result = await runFleetctl(["workspace", "use", workspaceId, FLAG_JSON], { env });
  const parsed = parseJson(result, `workspace use ${workspaceId}`);
  assert.equal(parsed[WS_USE_ACTIVE_KEY], workspaceId,
    `workspace use ${workspaceId}: ${WS_USE_ACTIVE_KEY} mismatch: ${result.stdout}`);
}

/** `list --workspace-id <id> --json` → the workspace's fleet rows. */
export async function listFleetsIn(env: Env, workspaceId: string): Promise<ReadonlyArray<FleetRow>> {
  const result = await runFleetctl(["list", FLAG_WORKSPACE_ID, workspaceId, FLAG_JSON], { env });
  const parsed = parseJson(result, `list ${FLAG_WORKSPACE_ID} ${workspaceId}`);
  const items = parsed[AGENT_LIST_ITEMS_KEY];
  assert.ok(Array.isArray(items),
    `list ${FLAG_WORKSPACE_ID} ${workspaceId}: ${AGENT_LIST_ITEMS_KEY} not an array: ${result.stdout}`);
  return items as ReadonlyArray<FleetRow>;
}

/**
 * Extract the fleet id from an install envelope, tolerating both `fleet_id`
 * (the documented install JSON key) and `id` (some list/route variants).
 * Throws rather than returning undefined so callers never `.startsWith` /
 * compare against an undefined id.
 */
export function fleetIdOf(installed: { readonly id?: unknown; readonly fleet_id?: unknown }): string {
  const raw = (typeof installed.fleet_id === "string" && installed.fleet_id)
    || (typeof installed.id === "string" && installed.id);
  assert.ok(typeof raw === "string" && raw.length > 0,
    `install envelope missing ${AGENT_ID_KEY}/id: ${JSON.stringify(installed)}`);
  return raw;
}

/** True iff a row whose `fleet_id`/`id` equals `fleetId` is present. */
export function hasFleetWithId(rows: ReadonlyArray<FleetRow>, fleetId: string): boolean {
  return rows.some((row) => row.fleet_id === fleetId || row.id === fleetId);
}
