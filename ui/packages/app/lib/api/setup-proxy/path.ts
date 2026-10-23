// A workspace proxy is a route whitelist, never a general credentialed tunnel.

import { z } from "zod";

export const SETUP_METHOD = {get: "GET", post: "POST", patch: "PATCH", delete: "DELETE"} as const;
const RESOURCE = {models: "models", drafts: "drafts", fleets: "fleets", libraries: "fleet-libraries"} as const;
const SUFFIX = {messages: "messages", cancellations: "cancellations", events: "events", stream: "stream",
  revisions: "revisions", schedules: "schedules", sync: "sync", owned: "owned"} as const;
const PAGE = ["limit", "starting_after"] as const;
const uuid = z.uuidv7();
const DOT_ONLY = /^\.+$/u;
const MAX_LIBRARY_ID_LENGTH = 128;
/** `bodyless`: a POST action that takes no body, so the proxy forwards none and asks for no JSON media type. */
type MatchedRoute = {readonly stream: boolean; readonly query: readonly string[]; readonly bodyless?: boolean};
const ORDINARY: MatchedRoute = {stream: false, query: []};
const BODYLESS_ACTION: MatchedRoute = {stream: false, query: [], bodyless: true};
const PAGED: MatchedRoute = {stream: false, query: PAGE};

export function matchSetupRoute(workspace: string, parts: readonly string[], method: string): MatchedRoute | undefined {
  if (!uuid.safeParse(workspace).success) return undefined;
  const [resource, id, child, member] = parts;
  if (resource === RESOURCE.models && parts.length === 1 && method === SETUP_METHOD.get) {
    return {stream: false, query: [...PAGE, "purpose"]};
  }
  if (resource === RESOURCE.drafts) return draft(parts, method);
  if (resource === RESOURCE.libraries) return library(parts, method);
  if (resource !== RESOURCE.fleets) return undefined;
  if (parts.length === 1 && method === SETUP_METHOD.get) return PAGED;
  if (parts.length === 1 && method === SETUP_METHOD.post) return ORDINARY;
  if (!uuid.safeParse(id).success) return undefined;
  if (parts.length === 2 && [SETUP_METHOD.get, SETUP_METHOD.patch].some((allowed) => allowed === method)) return ORDINARY;
  if (parts.length === 3 && child === SUFFIX.revisions && method === SETUP_METHOD.post) return ORDINARY;
  if (parts.length === 4 && child === SUFFIX.schedules && uuid.safeParse(member).success
    && [SETUP_METHOD.get, SETUP_METHOD.patch].some((allowed) => allowed === method)) return ORDINARY;
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
  if (parts.length === 3 && child === SUFFIX.cancellations && method === SETUP_METHOD.post) return ORDINARY;
  if (parts.length === 4 && child === SUFFIX.events && tail === SUFFIX.stream && method === SETUP_METHOD.get) return {stream: true, query: []};
  return undefined;
}

function library(parts: readonly string[], method: string): MatchedRoute | undefined {
  const [, id, child, revision] = parts;
  if (parts.length === 1) {
    if (method === SETUP_METHOD.get) return PAGED;
    return method === SETUP_METHOD.post ? ORDINARY : undefined;
  }
  if (id === SUFFIX.owned) return parts.length === 2 && method === SETUP_METHOD.get ? PAGED : undefined;
  if (!id || id.length > MAX_LIBRARY_ID_LENGTH || DOT_ONLY.test(id)) return undefined;
  if (parts.length === 2) {
    if (method === SETUP_METHOD.get) return {stream: false, query: ["revision_id"]};
    return method === SETUP_METHOD.delete ? ORDINARY : undefined;
  }
  if (child !== SUFFIX.revisions) return undefined;
  if (parts.length === 3) {
    if (method === SETUP_METHOD.get) return PAGED;
    return method === SETUP_METHOD.post ? ORDINARY : undefined;
  }
  return parts.length === 4 && uuid.safeParse(revision).success && method === SETUP_METHOD.get ? ORDINARY : undefined;
}
