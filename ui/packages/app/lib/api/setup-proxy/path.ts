// A workspace proxy is a route whitelist, never a general credentialed tunnel.

import { z } from "zod";

export const SETUP_METHOD = {get: "GET", post: "POST", patch: "PATCH", delete: "DELETE"} as const;
const RESOURCE = {models: "models", drafts: "drafts", fleets: "fleets"} as const;
const SUFFIX = {tasks: "tasks", messages: "messages", turns: "turns", stream: "stream",
  revisions: "revisions", schedules: "schedules", sync: "sync", setupContext: "setup-context"} as const;
const PAGE = ["limit", "starting_after"] as const;
const uuid = z.uuidv7();
/** `bodyless`: a POST action that takes no body, so the proxy forwards none and asks for no JSON media type. */
type MatchedRoute = {readonly stream: boolean; readonly query: readonly string[]; readonly bodyless?: boolean};
const ORDINARY: MatchedRoute = {stream: false, query: []};
const BODYLESS_ACTION: MatchedRoute = {stream: false, query: [], bodyless: true};
const PAGED: MatchedRoute = {stream: false, query: PAGE};
// The fleet list alone also takes `name`: an addressed chat message finds the
// fleet it names through it. Every other list keeps the shared page keys.
const FLEET_LIST: MatchedRoute = {stream: false, query: [...PAGE, "name"]};

export function matchSetupRoute(workspace: string, parts: readonly string[], method: string): MatchedRoute | undefined {
  if (!uuid.safeParse(workspace).success) return undefined;
  const [resource, id, child, member] = parts;
  if (resource === RESOURCE.models && parts.length === 1 && method === SETUP_METHOD.get) {
    return {stream: false, query: [...PAGE, "purpose"]};
  }
  if (resource === RESOURCE.drafts) return draft(parts, method);
  if (resource !== RESOURCE.fleets) return undefined;
  if (parts.length === 1 && method === SETUP_METHOD.get) return FLEET_LIST;
  if (parts.length === 1 && method === SETUP_METHOD.post) return ORDINARY;
  if (!uuid.safeParse(id).success) return undefined;
  if (parts.length === 2 && [SETUP_METHOD.get, SETUP_METHOD.patch].some((allowed) => allowed === method)) return ORDINARY;
  if (parts.length === 4 && child === SUFFIX.tasks && uuid.safeParse(member).success && method === SETUP_METHOD.patch) return ORDINARY;
  // Stop: a turn's status PATCH (`FleetRoute::Turn`).
  if (parts.length === 4 && child === SUFFIX.turns && uuid.safeParse(member).success && method === SETUP_METHOD.patch) return ORDINARY;
  if (parts.length === 3 && child === SUFFIX.setupContext && method === SETUP_METHOD.get) return {stream: false, query: ["starting_after"]};
  if (parts.length === 3 && child === SUFFIX.revisions && method === SETUP_METHOD.post) return ORDINARY;
  if (parts.length === 4 && child === SUFFIX.schedules && uuid.safeParse(member).success
    && [SETUP_METHOD.get, SETUP_METHOD.patch, SETUP_METHOD.delete].some((allowed) => allowed === method)) return ORDINARY;
  // A schedule's `:sync` retry (`FleetRoute::ScheduleSync`): a POST with no body.
  if (parts.length === 5 && child === SUFFIX.schedules && uuid.safeParse(member).success
    && parts[4] === SUFFIX.sync && method === SETUP_METHOD.post) return BODYLESS_ACTION;
  return undefined;
}

function draft(parts: readonly string[], method: string): MatchedRoute | undefined {
  const [, id, child, tail] = parts;
  if (parts.length === 1) {
    if (method === SETUP_METHOD.get) return {stream: false, query: [...PAGE, "state", "linked_fleet_id", "q"]};
    return method === SETUP_METHOD.post ? ORDINARY : undefined;
  }
  if (!uuid.safeParse(id).success) return undefined;
  if (parts.length === 2 && [SETUP_METHOD.get, SETUP_METHOD.patch, SETUP_METHOD.delete].some((allowed) => allowed === method)) return ORDINARY;
  if (parts.length === 3 && child === SUFFIX.messages) {
    if (method === SETUP_METHOD.get) return PAGED;
    return method === SETUP_METHOD.post ? ORDINARY : undefined;
  }
  if (parts.length === 4 && child === SUFFIX.turns && tail === SUFFIX.stream && method === SETUP_METHOD.get) return {stream: true, query: []};
  // Stop: a draft turn's status PATCH (`WorkspaceRoute::DraftTurn`).
  if (parts.length === 4 && child === SUFFIX.turns && uuid.safeParse(tail).success && method === SETUP_METHOD.patch) return ORDINARY;
  return undefined;
}
