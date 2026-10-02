import { request } from "./client";
import { decodeOnePage, isEpochMs, isNonEmptyString, isRecord } from "./decode";
import { isAccountRole } from "./workspaces";
import type { AccountRole } from "./workspaces-types";

// The people in the caller's own account. Removing one is idempotent, and the
// backend refuses to remove the last owner with 409 `UZ-INV-004`.

const MEMBERS_PATH = "/v1/tenants/me/members";

/** One member of the caller's account. */
export type MemberSummary = {
  user_id: string;
  /** Null when the identity provider supplied no name. */
  display_name: string | null;
  email: string;
  role: AccountRole;
  /** When they joined the account, epoch milliseconds. */
  joined_at: number;
};

const decodeMember = (value: unknown): MemberSummary => {
  if (
    !isRecord(value) ||
    !isNonEmptyString(value.user_id) ||
    (value.display_name !== null && typeof value.display_name !== "string") ||
    !isNonEmptyString(value.email) ||
    !isAccountRole(value.role) ||
    !isEpochMs(value.joined_at)
  ) {
    throw new Error("member is invalid");
  }
  return {
    user_id: value.user_id,
    display_name: value.display_name,
    email: value.email,
    role: value.role,
    joined_at: value.joined_at,
  };
};

/** One member of a workspace's account, as a thread names its senders. */
export type WorkspaceMember = {
  user_id: string;
  /** Null when the identity provider supplied no name. */
  display_name: string | null;
  role: AccountRole;
  /** The actor this member's messages record: `steer:<subject>`. */
  actor: string;
};

const decodeWorkspaceMember = (value: unknown): WorkspaceMember => {
  if (
    !isRecord(value) ||
    !isNonEmptyString(value.user_id) ||
    (value.display_name !== null && typeof value.display_name !== "string") ||
    !isAccountRole(value.role) ||
    !isNonEmptyString(value.actor)
  ) {
    throw new Error("workspace member is invalid");
  }
  return {
    user_id: value.user_id,
    display_name: value.display_name,
    role: value.role,
    actor: value.actor,
  };
};

// GET /v1/workspaces/{workspace_id}/members — who can work in a workspace,
// with no addresses; one page.
export async function listWorkspaceMembers(workspaceId: string, token: string): Promise<WorkspaceMember[]> {
  const path = `/v1/workspaces/${encodeURIComponent(workspaceId)}/members`;
  const response = await request<unknown>(path, { method: "GET" }, token);
  return decodeOnePage(response, decodeWorkspaceMember);
}

// GET /v1/tenants/me/members — oldest membership first.
export async function listMembers(token: string): Promise<MemberSummary[]> {
  const response = await request<unknown>(MEMBERS_PATH, { method: "GET" }, token);
  return decodeOnePage(response, decodeMember);
}

// DELETE /v1/tenants/me/members/{user_id} — idempotent: 204 either way.
export async function removeMember(token: string, userId: string): Promise<void> {
  await request<void>(`${MEMBERS_PATH}/${encodeURIComponent(userId)}`, { method: "DELETE" }, token);
}
