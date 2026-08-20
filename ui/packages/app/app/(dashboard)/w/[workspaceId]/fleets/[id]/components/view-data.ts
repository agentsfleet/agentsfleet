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

/**
 * The in-flight reads a view needs that depend only on route params — started
 * beside the fleet read, never after it. Fields are promises; the matching
 * loader awaits them once the fleet itself has resolved. Starting these here
 * is the whole point: the detail read used to serialize every view fetch
 * behind itself.
 */
export type ViewData = {
  thread?: Promise<ThreadPage | null>;
  approvals?: Promise<Awaited<ReturnType<typeof listApprovals>> | null>;
  eventsInitial?: Promise<EventsPage>;
  memories?: Promise<Awaited<ReturnType<typeof listAllMemories>> | null>;
};

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
        eventsInitial: listFleetEvents(args.workspaceId, args.fleetId, args.token, {
          limit: args.eventsPageSize,
          ...(args.eventsCursor ? { cursor: args.eventsCursor } : {}),
        }).catch(() => ({ items: [], next_cursor: null })),
      };
    case FLEET_VIEW.memory:
      return {
        memories: listAllMemories(args.workspaceId, args.fleetId, args.token).catch(
          () => null,
        ),
      };
    case FLEET_VIEW.skill:
    case FLEET_VIEW.trigger:
      // Skill renders from the fleet itself; trigger needs the fleet's declared
      // triggers before it can fetch anything.
      return {};
    default:
      return {
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
