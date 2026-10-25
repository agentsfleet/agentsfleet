import {
  listFleetEvents,
  listFleetMessages,
  type EventsPage,
  type ThreadPage,
} from "@/lib/api/events";
import { listAllMemories } from "@/lib/api/memory";
import { listWorkspaceMembers, type WorkspaceMember } from "@/lib/api/tenant-members";
import { request } from "@/lib/api/client";
import { ModelsSchema, LIST_PAGE_MAX, type WorkspaceModel } from "@/lib/api/draft-types";
import { FLEET_VIEW, type FleetView } from "./FleetSubnavigation";

/** Turns the chat view opens with — one thread request, bodies included. */
export const CHAT_TURNS = 20;
const PLATFORM_DEFAULT = "platform-default";

/** Chat opens on the transcript; the approvals waiting on the fleet ride the
 * fleet detail the page already reads. */
export type ChatViewData = {
  view: typeof FLEET_VIEW.chat;
  thread: Promise<ThreadPage | null>;
  /** Who the thread can name. A failed read names no one, and every sender
   * falls back to its actor label. */
  members: Promise<WorkspaceMember[] | null>;
  models: Promise<WorkspaceModel[] | null>;
};

/** Events opens on the page the URL cursor names. */
export type EventsViewData = {
  view: typeof FLEET_VIEW.events;
  eventsInitial: Promise<{ ok: true; page: EventsPage } | { ok: false; error: unknown }>;
};

/** Memory opens on the whole walk — the panel filters client-side. */
export type MemoryViewData = {
  view: typeof FLEET_VIEW.memory;
  memories: Promise<Awaited<ReturnType<typeof listAllMemories>> | null>;
};

/**
 * The in-flight reads a view needs that depend only on route params — started
 * beside the fleet read, never after it. Fields are promises; the matching
 * loader awaits them once the fleet itself has resolved. Starting these here
 * is the whole point: the detail read used to serialize every view fetch
 * behind itself. The `view` tag narrows the shape at the loader boundary, so
 * a loader never carries a fallback for a field its own view always sets.
 */
export type ViewData =
  | ChatViewData
  | EventsViewData
  | MemoryViewData;

export type ViewDataArgs = {
  workspaceId: string;
  fleetId: string;
  token: string;
  eventsCursor: string | null;
  eventsPageSize: number;
};

export function startViewData(view: FleetView, args: ViewDataArgs): ViewData {
  switch (view) {
    case FLEET_VIEW.events:
      // Fetched for the cursor the URL names, so a reload or a shared link
      // opens the page the operator was actually looking at.
      return {
        view,
        eventsInitial: listFleetEvents(args.workspaceId, args.fleetId, args.token, {
          limit: args.eventsPageSize,
          ...(args.eventsCursor ? { cursor: args.eventsCursor } : {}),
        }).then(
          (page) => ({ ok: true as const, page }),
          (error: unknown) => ({ ok: false as const, error }),
        ),
      };
    case FLEET_VIEW.memory:
      return {
        view,
        memories: listAllMemories(args.workspaceId, args.fleetId, args.token).catch(
          () => null,
        ),
      };
    default:
      return {
        view,
        thread: listFleetMessages(args.workspaceId, args.fleetId, args.token, {
          limit: CHAT_TURNS,
        }).catch(() => null),
        members: listWorkspaceMembers(args.workspaceId, args.token).catch(() => null),
        models: request<unknown>(`/v1/workspaces/${encodeURIComponent(args.workspaceId)}/models?limit=${LIST_PAGE_MAX}`, {method: "GET"}, args.token)
          .then(body => ModelsSchema.parse(body).items).catch(() => null),
      };
  }
}

/** A missing catalogue must not pretend that a different model was selected. */
export function runtimeModelLabel(models: readonly WorkspaceModel[] | null, selected: unknown): string {
  const ref = typeof selected === "string" ? selected : PLATFORM_DEFAULT;
  return models?.find(model => model.ref === ref)?.label
    ?? (ref === PLATFORM_DEFAULT ? "Platform default model" : ref);
}
