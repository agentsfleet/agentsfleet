"use client";

import { Badge, CopyButton, DataTable, type DataTableColumn, IconAction, Time } from "@agentsfleet/design-system";
import { BanIcon, SendIcon, UserMinusIcon } from "lucide-react";
import { EMAIL_STATUS, type EmailStatus, type InviteSummary } from "@/lib/api/invites";
import type { MemberSummary } from "@/lib/api/tenant-members";
import { ACCOUNT_ROLE } from "@/lib/api/workspaces";
import { memberName } from "./TeamConfirm";

export const TEAM_CAPTION = "People";
const INVITED = "invited";

/** What each email status reads as beside an invite's actions. */
export const EMAIL_STATUS_LABEL: Record<EmailStatus, string> = {
  [EMAIL_STATUS.sent]: "Email sent",
  [EMAIL_STATUS.failed]: "Email not sent",
  [EMAIL_STATUS.unconfigured]: "Email not set up",
};

/** One row: a person in the account, or an address invited into it. */
type TeamRow =
  | { key: string; member: MemberSummary; invite?: never }
  | { key: string; invite: InviteSummary; member?: never };

type Props = {
  members: MemberSummary[];
  invites: InviteSummary[];
  pending: boolean;
  onRemove: (member: MemberSummary) => void;
  onRevoke: (invite: InviteSummary) => void;
  onResend: (invite: InviteSummary) => void;
};

type InviteHandlers = Pick<Props, "pending" | "onRevoke" | "onResend">;

// People first, then the invites still waiting for an answer. The owner is
// always listed, so the table is never empty. Only a member can be removed:
// the account's one owner cannot, which the backend also refuses with 409
// `UZ-INV-004`.
export function TeamTable({ members, invites, pending, onRemove, onRevoke, onResend }: Props) {
  const rows: TeamRow[] = [
    ...members.map((member) => ({ key: `member:${member.user_id}`, member })),
    ...invites.map((invite) => ({ key: `invite:${invite.id}`, invite })),
  ];
  return (
    <DataTable
      columns={columns(pending, onRemove, { pending, onRevoke, onResend })}
      rows={rows}
      rowKey={(row) => row.key}
      caption={TEAM_CAPTION}
      pagination={false}
    />
  );
}

function PersonCell({ row }: { row: TeamRow }) {
  if (row.invite) return <div className="truncate text-sm">{row.invite.email}</div>;
  return (
    <div className="min-w-0">
      <div className="truncate text-sm">{memberName(row.member)}</div>
      {row.member.display_name ? (
        <div className="truncate text-label text-muted-foreground">{row.member.email}</div>
      ) : null}
    </div>
  );
}

// When the person joined or the invite went out, as the API Keys table shows
// a key's creation; an invite adds when it lapses.
function TimeCell({ row }: { row: TeamRow }) {
  const since = row.invite ? row.invite.created_at : row.member.joined_at;
  return (
    <div className="flex flex-col items-start gap-xs text-label leading-label text-muted-foreground">
      <Time value={new Date(since)} format="relative" className="tabular-nums" />
      {row.invite ? (
        <span>expires <Time value={new Date(row.invite.expires_at)} format="relative" className="tabular-nums" /></span>
      ) : null}
    </div>
  );
}

function RoleCell({ row }: { row: TeamRow }) {
  if (row.invite) return <Badge variant="amber">{INVITED}</Badge>;
  return <Badge variant={row.member.role === ACCOUNT_ROLE.owner ? "cyan" : "default"}>{row.member.role}</Badge>;
}

// An invite's email status, then: send again when the email did not go, copy
// the link the invitee needs either way, and revoke. "Email not set up" means
// this deployment has no mail relay; the copied link is the way in.
function InviteActions({ invite, pending, onRevoke, onResend }: InviteHandlers & { invite: InviteSummary }) {
  return (
    <div className="inline-flex items-center gap-xs">
      <span className="text-label leading-label text-muted-foreground">{EMAIL_STATUS_LABEL[invite.email_status]}</span>
      {invite.email_status === EMAIL_STATUS.sent ? null : (
        <IconAction
          type="button"
          disabled={pending}
          onClick={() => onResend(invite)}
          label={`Send the invite email to ${invite.email} again`}
        >
          <SendIcon size={14} />
        </IconAction>
      )}
      <CopyButton value={invite.link} label={`Copy invite link for ${invite.email}`} />
      <IconAction
        type="button"
        variant="destructive"
        disabled={pending}
        onClick={() => onRevoke(invite)}
        label={`Revoke invite for ${invite.email}`}
      >
        <BanIcon size={14} />
      </IconAction>
    </div>
  );
}

function columns(
  pending: boolean,
  onRemove: (member: MemberSummary) => void,
  invites: InviteHandlers,
): DataTableColumn<TeamRow>[] {
  return [
    { key: "person", header: "Person", cell: (row) => <PersonCell row={row} /> },
    { key: "role", header: "Role", cell: (row) => <RoleCell row={row} /> },
    { key: "time", header: "Time", hideOnMobile: true, cell: (row) => <TimeCell row={row} /> },
    {
      key: "actions",
      header: "Actions",
      numeric: true,
      cell: (row) => {
        if (row.invite) return <InviteActions invite={row.invite} {...invites} />;
        const { member } = row;
        if (member.role !== ACCOUNT_ROLE.member) return null;
        return (
          <IconAction
            type="button"
            variant="destructive"
            disabled={pending}
            onClick={() => onRemove(member)}
            label={`Remove ${memberName(member)}`}
          >
            <UserMinusIcon size={14} />
          </IconAction>
        );
      },
    },
  ];
}
