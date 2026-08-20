import {
  listFleetEvents,
  listFleetMessages,
  type EventsPage,
  type ThreadPage,
} from "@/lib/api/events";
import { listApprovals } from "@/lib/api/approvals";
import { listAllMemories } from "@/lib/api/memory";
import { FLEET_VIEW, type FleetView } from "./FleetSubnavigation";

/** Turns the chat view opens with — one thread request, bodies included. */
export const CHAT_TURNS = 20;
const CHAT_APPROVALS_LIMIT = 50;

/** Chat opens on the transcript plus the approvals waiting on it. */
export type ChatViewData = {
  view: typeof FLEET_VIEW.chat;
  thread: Promise<ThreadPage | null>;
  approvals: Promise<Awaited<ReturnType<typeof listApprovals>> | null>;
};

/** Events opens on the page the URL cursor names. */
export type EventsViewData = {
  view: typeof FLEET_VIEW.events;
  eventsInitial: Promise<EventsPage>;
};

/** Memory opens on the whole walk — the panel filters client-side. */
export type MemoryViewData = {
  view: typeof FLEET_VIEW.memory;
  memories: Promise<Awaited<ReturnType<typeof listAllMemories>> | null>;
};

/** Skill and trigger render from the fleet itself — nothing to start early. */
export type SourceViewData = {
  view: typeof FLEET_VIEW.skill | typeof FLEET_VIEW.trigger;
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
  | MemoryViewData
  | SourceViewData;

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
        }).catch(() => ({ items: [], next_cursor: null })),
      };
    case FLEET_VIEW.memory:
      return {
        view,
        memories: listAllMemories(args.workspaceId, args.fleetId, args.token).catch(
          () => null,
        ),
      };
    case FLEET_VIEW.skill:
    case FLEET_VIEW.trigger:
      // Skill renders from the fleet itself; trigger needs the fleet's declared
      // triggers before it can fetch anything.
      return { view };
    default:
      return {
        view,
        thread: listFleetMessages(args.workspaceId, args.fleetId, args.token, {
          limit: CHAT_TURNS,
        }).catch(() => null),
        approvals: listApprovals(args.workspaceId, args.token, {
          fleetId: args.fleetId,
          limit: CHAT_APPROVALS_LIMIT,
        }).catch(() => null),
      };
  }
}
