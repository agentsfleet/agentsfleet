import { request } from "./client";
import { decodeOnePage, isEpochMs, isNonEmptyString, isRecord } from "./decode";
import { decodeWorkspaceAccount, type WorkspaceAccount } from "./workspaces";

// Invites into the caller's own account (the owner's side) and invites waiting
// for the caller's address (the invitee's side). An account's invites are few,
// so every list is one page: the backend sends `next_cursor: null` and the
// client never walks.

const OWNER_INVITES_PATH = "/v1/tenants/me/invites";
const WAITING_INVITES_PATH = "/v1/users/me/invites";
const SEND_SEGMENT = "send";

/** What became of an invite's most recent email. Mirrors `EMAIL_STATUS_SENT`,
 * `EMAIL_STATUS_FAILED` and `EMAIL_STATUS_UNCONFIGURED` in
 * `rustd/crates/afd_tenant/src/team/invitation/mail_status.rs`. */
export const EMAIL_STATUS = {
  sent: "sent",
  failed: "failed",
  unconfigured: "unconfigured",
} as const;

export type EmailStatus = (typeof EMAIL_STATUS)[keyof typeof EMAIL_STATUS];

const EMAIL_STATUSES: ReadonlySet<string> = new Set(Object.values(EMAIL_STATUS));

const isEmailStatus = (value: unknown): value is EmailStatus =>
  typeof value === "string" && EMAIL_STATUSES.has(value);

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
  /** What became of its most recent email. */
  email_status: EmailStatus;
  /** When the relay last accepted its email, epoch milliseconds; null if never. */
  email_sent_at: number | null;
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
    !isNonEmptyString(value.link) ||
    !isEmailStatus(value.email_status) ||
    !(value.email_sent_at === null || isEpochMs(value.email_sent_at))
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
    email_status: value.email_status,
    email_sent_at: value.email_sent_at,
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

/** Create and send-again wait for the invite email, which the daemon bounds at
 * 10 s (`MAIL_SEND_DEADLINE`, `rustd/crates/afd_mail/src/mailer.rs`) after its
 * own reads. The client's default per-attempt timeout is also 10 s and would
 * give up first, reporting a timeout for an invite that was saved. */
export const INVITE_EMAIL_REQUEST_TIMEOUT_MS = 15_000;

// POST /v1/tenants/me/invites — invite one address into the caller's account.
export async function createInvite(token: string, email: string): Promise<InviteSummary> {
  const response = await request<unknown>(
    OWNER_INVITES_PATH,
    {
      method: "POST",
      body: JSON.stringify({ email }),
      signal: AbortSignal.timeout(INVITE_EMAIL_REQUEST_TIMEOUT_MS),
    },
    token,
  );
  return decodeInvite(response);
}

// GET /v1/tenants/me/invites — the account's pending invites.
export async function listInvites(token: string): Promise<InviteSummary[]> {
  const response = await request<unknown>(OWNER_INVITES_PATH, { method: "GET" }, token);
  return decodeOnePage(response, decodeInvite);
}

// DELETE /v1/tenants/me/invites/{invite_id} — 204 for a pending or already
// revoked invite. One its invitee already joined through answers 409
// `UZ-INV-003` (`current_state` "member"), raised as an ApiError.
export async function revokeInvite(token: string, inviteId: string): Promise<void> {
  await request<void>(invitePath(inviteId), { method: "DELETE" }, token);
}

// POST /v1/tenants/me/invites/{invite_id}/send — a new email attempt. 200 means
// the relay took it; a relay that is not set up or refused answers 503
// `UZ-INV-005`, which the client raises as an ApiError like any refusal.
export async function sendInviteEmail(token: string, inviteId: string): Promise<void> {
  const response = await request<unknown>(
    `${invitePath(inviteId)}/${SEND_SEGMENT}`,
    { method: "POST", signal: AbortSignal.timeout(INVITE_EMAIL_REQUEST_TIMEOUT_MS) },
    token,
  );
  if (!isRecord(response) || response.email_status !== EMAIL_STATUS.sent) {
    throw new Error("invite email answer is invalid");
  }
}

// GET /v1/users/me/invites — invites waiting for the signed-in person's address.
export async function listWaitingInvites(token: string): Promise<WaitingInvite[]> {
  const response = await request<unknown>(WAITING_INVITES_PATH, { method: "GET" }, token);
  return decodeOnePage(response, decodeWaiting);
}

// POST /v1/users/me/invites/{invite_id}/accept — accepting again answers the same.
export async function acceptInvite(token: string, inviteId: string): Promise<AcceptedInvite> {
  const response = await request<unknown>(
    `${WAITING_INVITES_PATH}/${encodeURIComponent(inviteId)}/accept`,
    { method: "POST" },
    token,
  );
  return decodeAccepted(response);
}
