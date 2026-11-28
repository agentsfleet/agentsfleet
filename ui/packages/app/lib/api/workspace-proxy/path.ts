// A workspace proxy is a route whitelist, never a general credentialed tunnel.

import { z } from "zod";
import { CHAT_BODY_LIMIT_BYTES } from "../body-limits";

export const PROXY_METHOD = {get: "GET", post: "POST", patch: "PATCH", delete: "DELETE"} as const;
const RESOURCE = {models: "models", fleets: "fleets"} as const;
const SUFFIX = {durableTasks: "durable_tasks", turns: "turns", schedules: "schedules", sync: "sync",
  triggers: "triggers", grants: "integration-grants", memories: "memories"} as const;
const PAGE = ["limit", "starting_after"] as const;
const uuid = z.uuidv7();
// A memory entry is named by its key, not an identifier. encodeURIComponent
// leaves '.' intact, so an empty or dot-only key would dot-normalize inside
// fetch and steer the minted token at another path.
const MEMORY_KEY = /^(?!\.*$)/;
/** `bodyless`: a POST action that takes no body, so the proxy forwards none and asks for no JSON media type. */
type MatchedRoute = {readonly query: readonly string[]; readonly bodyless?: boolean; readonly bodyLimit?: number};
const ORDINARY: MatchedRoute = {query: []};
const BODYLESS_ACTION: MatchedRoute = {query: [], bodyless: true};
const PAGED: MatchedRoute = {query: PAGE};
const MODELS: MatchedRoute = {query: [...PAGE, "purpose"]};
// The fleet list alone also takes `name`: an addressed chat message finds the
// fleet it names through it. Every other list keeps the shared page keys.
const FLEET_LIST: MatchedRoute = {query: [...PAGE, "name"]};
const FLEET_CREATE: MatchedRoute = {query: [], bodyLimit: CHAT_BODY_LIMIT_BYTES};
// The lists beside a fleet's chat, read whole and trimmed one row at a time.
const LISTS: ReadonlyMap<string | undefined, MatchedRoute> = new Map([
  [SUFFIX.schedules, ORDINARY], [SUFFIX.triggers, ORDINARY], [SUFFIX.grants, ORDINARY], [SUFFIX.memories, PAGED],
]);
const REMOVABLE: ReadonlySet<string | undefined> = new Set([SUFFIX.triggers, SUFFIX.grants]);

export function matchWorkspaceRoute(workspace: string, parts: readonly string[], method: string): MatchedRoute | undefined {
  if (!uuid.safeParse(workspace).success) return undefined;
  const [resource, id, child, member] = parts;
  if (resource === RESOURCE.models && parts.length === 1 && method === PROXY_METHOD.get) return MODELS;
  if (resource !== RESOURCE.fleets) return undefined;
  if (parts.length === 1) return method === PROXY_METHOD.get ? FLEET_LIST : method === PROXY_METHOD.post ? FLEET_CREATE : undefined;
  if (!uuid.safeParse(id).success) return undefined;
  if (parts.length === 2) return method === PROXY_METHOD.get || method === PROXY_METHOD.patch ? ORDINARY : undefined;
  if (parts.length === 3) return method === PROXY_METHOD.get ? LISTS.get(child) : undefined;
  if (parts.length === 4) return fleetMember(child, member, method);
  // A schedule's `:sync` retry (`FleetRoute::ScheduleSync`): a POST with no body.
  if (parts.length === 5 && child === SUFFIX.schedules && uuid.safeParse(member).success
    && parts[4] === SUFFIX.sync && method === PROXY_METHOD.post) return BODYLESS_ACTION;
  return undefined;
}

function fleetMember(child: string | undefined, member: string | undefined, method: string): MatchedRoute | undefined {
  if (child === SUFFIX.memories) return method === PROXY_METHOD.delete && MEMORY_KEY.test(String(member)) ? ORDINARY : undefined;
  if (!uuid.safeParse(member).success) return undefined;
  // Stop: a turn's status PATCH (`FleetRoute::Turn`); a task's state or statements.
  if ((child === SUFFIX.turns || child === SUFFIX.durableTasks) && method === PROXY_METHOD.patch) return ORDINARY;
  if (child === SUFFIX.schedules && [PROXY_METHOD.get, PROXY_METHOD.patch, PROXY_METHOD.delete].some((allowed) => allowed === method)) return ORDINARY;
  return REMOVABLE.has(child) && method === PROXY_METHOD.delete ? ORDINARY : undefined;
}
