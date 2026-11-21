import {
  listFleetMessages,
  type ThreadPage,
} from "@/lib/api/turns";
import { listWorkspaceMembers, type WorkspaceMember } from "@/lib/api/tenant-members";
import { request } from "@/lib/api/client";
import { ModelsSchema, type WorkspaceModel } from "@/lib/api/draft-types";

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
    models: readModels(args.workspaceId, args.token).catch(() => null),
  };
}

/** Every page of the workspace's models, at the deployment's page size. */
async function readModels(workspaceId: string, token: string): Promise<WorkspaceModel[]> {
  const path = `/v1/workspaces/${encodeURIComponent(workspaceId)}/models`;
  const models: WorkspaceModel[] = [];
  const seen = new Set<string>();
  let cursor: string | null = null;
  do {
    const query: string = cursor === null ? "" : `?${new URLSearchParams({starting_after: cursor})}`;
    const page = ModelsSchema.parse(await request<unknown>(`${path}${query}`, {method: "GET"}, token));
    models.push(...page.items);
    cursor = page.next_cursor;
    if (cursor !== null && seen.has(cursor)) throw new Error("model pagination repeated a cursor");
    if (cursor !== null) seen.add(cursor);
  } while (cursor !== null);
  return models;
}

/** A missing catalogue must not pretend that a different model was selected. */
export function runtimeModelLabel(models: readonly WorkspaceModel[] | null, selected: unknown): string {
  const ref = typeof selected === "string" ? selected : PLATFORM_DEFAULT;
  return models?.find(model => model.ref === ref)?.label
    ?? (ref === PLATFORM_DEFAULT ? "Platform default model" : ref);
}
