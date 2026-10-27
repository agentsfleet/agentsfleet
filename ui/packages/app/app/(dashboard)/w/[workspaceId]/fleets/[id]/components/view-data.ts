import {
  listFleetMessages,
  type ThreadPage,
} from "@/lib/api/events";
import { listWorkspaceMembers, type WorkspaceMember } from "@/lib/api/tenant-members";
import { request } from "@/lib/api/client";
import { ModelsSchema, LIST_PAGE_MAX, type WorkspaceModel } from "@/lib/api/draft-types";

/** Turns the chat view opens with — one thread request, bodies included. */
export const CHAT_TURNS = 20;
const PLATFORM_DEFAULT = "platform-default";

/** Chat opens on the transcript; the approvals waiting on the fleet ride the
 * fleet detail the page already reads. */
export type ChatViewData = {
  thread: Promise<ThreadPage | null>;
  /** Who the thread can name. A failed read names no one, and every sender
   * falls back to its actor label. */
  members: Promise<WorkspaceMember[] | null>;
  models: Promise<WorkspaceModel[] | null>;
};

export function startChatData(args: {workspaceId: string; fleetId: string; token: string}): ChatViewData {
  return {
    thread: listFleetMessages(args.workspaceId, args.fleetId, args.token, {limit: CHAT_TURNS}).catch(() => null),
    members: listWorkspaceMembers(args.workspaceId, args.token).catch(() => null),
    models: request<unknown>(`/v1/workspaces/${encodeURIComponent(args.workspaceId)}/models?limit=${LIST_PAGE_MAX}`, {method: "GET"}, args.token)
      .then(body => ModelsSchema.parse(body).items).catch(() => null),
  };
}

/** A missing catalogue must not pretend that a different model was selected. */
export function runtimeModelLabel(models: readonly WorkspaceModel[] | null, selected: unknown): string {
  const ref = typeof selected === "string" ? selected : PLATFORM_DEFAULT;
  return models?.find(model => model.ref === ref)?.label
    ?? (ref === PLATFORM_DEFAULT ? "Platform default model" : ref);
}
