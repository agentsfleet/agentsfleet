import { request } from "./client";
import { decodeOnePage, isEpochMs, isNonEmptyString, isRecord } from "./decode";
import { decodeWorkspaceAccount, type WorkspaceAccount } from "./workspaces";

// Invites into the caller's own account (the owner's side) and invites waiting
// for the caller's address (the invitee's side). An account's invites are few,
// so every list is one page: the backend sends `next_cursor: null` and the
// client never walks.

const OWNER_INVITES_PATH = "/v1/tenants/me/invites";
const WAITING_INVITES_PATH = "/v1/me/invites";

/** One invite, as the account's owner sees it. */
export type InviteSummary = {
  id: string;
  /** Lowercased by the backend. */
  email: string;
  role: string;
  /** Epoch milliseconds. */
  expires_at: number;
  /** Epoch milliseconds. */
  created_at: number;
  /** The dashboard page the invitee opens to accept it. */
  link: string;
};

/** One invite waiting for the caller's address. */
export type WaitingInvite = {
  id: string;
  account: WorkspaceAccount;
  /** Epoch milliseconds. */
  expires_at: number;
};

/** The account an accepted invite joined, and the workspaces now open. */
export type AcceptedInvite = {
  tenant_id: string;
  workspace_ids: string[];
};

const decodeInvite = (value: unknown): InviteSummary => {
  if (
    !isRecord(value) ||
    !isNonEmptyString(value.id) ||
    !isNonEmptyString(value.email) ||
    !isNonEmptyString(value.role) ||
    !isEpochMs(value.expires_at) ||
    !isEpochMs(value.created_at) ||
    !isNonEmptyString(value.link)
  ) {
    throw new Error("invite is invalid");
  }
  return {
    id: value.id,
    email: value.email,
    role: value.role,
    expires_at: value.expires_at,
    created_at: value.created_at,
    link: value.link,
  };
};

const decodeWaiting = (value: unknown): WaitingInvite => {
  if (!isRecord(value) || !isNonEmptyString(value.id) || !isEpochMs(value.expires_at)) {
    throw new Error("waiting invite is invalid");
  }
  return {
    id: value.id,
    account: decodeWorkspaceAccount(value.account),
    expires_at: value.expires_at,
  };
};

const decodeAccepted = (value: unknown): AcceptedInvite => {
  if (
    !isRecord(value) ||
    !isNonEmptyString(value.tenant_id) ||
    !Array.isArray(value.workspace_ids) ||
    !value.workspace_ids.every(isNonEmptyString)
  ) {
    throw new Error("accepted invite is invalid");
  }
  return { tenant_id: value.tenant_id, workspace_ids: value.workspace_ids };
};

const invitePath = (inviteId: string): string =>
  `${OWNER_INVITES_PATH}/${encodeURIComponent(inviteId)}`;

// POST /v1/tenants/me/invites — invite one address into the caller's account.
export async function createInvite(token: string, email: string): Promise<InviteSummary> {
  const response = await request<unknown>(
    OWNER_INVITES_PATH,
    { method: "POST", body: JSON.stringify({ email }) },
    token,
  );
  return decodeInvite(response);
}

// GET /v1/tenants/me/invites — the account's pending invites.
export async function listInvites(token: string): Promise<InviteSummary[]> {
  const response = await request<unknown>(OWNER_INVITES_PATH, { method: "GET" }, token);
  return decodeOnePage(response, decodeInvite);
}

// DELETE /v1/tenants/me/invites/{invite_id} — idempotent: 204 either way.
export async function revokeInvite(token: string, inviteId: string): Promise<void> {
  await request<void>(invitePath(inviteId), { method: "DELETE" }, token);
}

// GET /v1/me/invites — invites waiting for the signed-in person's address.
export async function listWaitingInvites(token: string): Promise<WaitingInvite[]> {
  const response = await request<unknown>(WAITING_INVITES_PATH, { method: "GET" }, token);
  return decodeOnePage(response, decodeWaiting);
}

// POST /v1/me/invites/{invite_id}/accept — accepting again answers the same.
export async function acceptInvite(token: string, inviteId: string): Promise<AcceptedInvite> {
  const response = await request<unknown>(
    `${WAITING_INVITES_PATH}/${encodeURIComponent(inviteId)}/accept`,
    { method: "POST" },
    token,
  );
  return decodeAccepted(response);
}
