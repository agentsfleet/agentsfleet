"use client";

import {
  Badge,
  CopyButton,
  DataTable,
  type DataTableColumn,
  IconAction,
  Time,
  TooltipButton,
  cn,
} from "@agentsfleet/design-system";
import { BanIcon, SendIcon, UserMinusIcon } from "lucide-react";
import type { InviteSummary } from "@/lib/api/invites";
import { EMAIL_STATUS, type EmailStatus } from "@/lib/api/invites-types";
import type { MemberSummary } from "@/lib/api/tenant-members";
import { ACCOUNT_ROLE } from "@/lib/api/workspaces-types";
import { memberName } from "./TeamConfirm";

export const TEAM_CAPTION = "People";
const INVITED = "invited";
const STATUS_TEXT_CLASS = "text-label leading-label";

/** What each email status reads as under an invite's dates. */
const EMAIL_STATUS_LABEL: Record<EmailStatus, string> = {
  [EMAIL_STATUS.sent]: "Invite emailed",
  [EMAIL_STATUS.failed]: "Email not sent",
  [EMAIL_STATUS.unconfigured]: "Email not set up",
};

// A sent email reads quietly. One that did not go, or has no relay to go
// through, is the owner's to act on, so it reads as a warning.
const EMAIL_STATUS_TONE: Record<EmailStatus, string> = {
  [EMAIL_STATUS.sent]: "text-muted-foreground",
  [EMAIL_STATUS.failed]: "text-warning",
  [EMAIL_STATUS.unconfigured]: "text-warning",
};

const NO_RELAY_EXPLAINED = "Email isn't set up for this deployment. Copy the link and share it instead.";

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
  return (
    <div className="min-w-0">
      {row.invite ? <div className="truncate text-sm">{row.invite.email}</div> : <MemberName member={row.member} />}
      {/* The Time column leaves the row on a phone, so its dates and an
          invite's email status read here instead, as the API Keys table does. */}
      <div className="mt-xs flex flex-col items-start gap-xs sm:hidden">
        <TimeCell row={row} />
      </div>
    </div>
  );
}

function MemberName({ member }: { member: MemberSummary }) {
  return (
    <>
      <div className="truncate text-sm">{memberName(member)}</div>
      {member.display_name ? <div className="truncate text-label text-muted-foreground">{member.email}</div> : null}
    </>
  );
}

// "Email not set up" leaves nothing to send again, so it says why and what to
// do instead; the trigger is the system's button, so a keyboard reaches that
// too and sees the shared focus ring. `link` is the variant with no chrome;
// `eyebrow` is the size that sits on a text line, whose side padding would
// indent it from the status lines it stands among, hence `px-0`.
function EmailStatusLabel({ status }: { status: EmailStatus }) {
  const className = cn(STATUS_TEXT_CLASS, EMAIL_STATUS_TONE[status]);
  if (status !== EMAIL_STATUS.unconfigured) return <span className={className}>{EMAIL_STATUS_LABEL[status]}</span>;
  return (
    <TooltipButton variant="link" size="eyebrow" className={cn(className, "px-0")} tooltip={NO_RELAY_EXPLAINED}>
      {EMAIL_STATUS_LABEL[status]}
    </TooltipButton>
  );
}

// When the person joined or the invite went out, as the API Keys table shows
// a key's creation; an invite adds when it lapses and whether its email went.
function TimeCell({ row }: { row: TeamRow }) {
  const since = row.invite ? row.invite.created_at : row.member.joined_at;
  return (
    <div className="flex flex-col items-start gap-xs text-label leading-label text-muted-foreground">
      <Time value={new Date(since)} format="relative" className="tabular-nums" />
      {row.invite ? (
        <span>expires <Time value={new Date(row.invite.expires_at)} format="relative" className="tabular-nums" /></span>
      ) : null}
      {row.invite ? <EmailStatusLabel status={row.invite.email_status} /> : null}
    </div>
  );
}

function RoleCell({ row }: { row: TeamRow }) {
  if (row.invite) return <Badge variant="amber">{INVITED}</Badge>;
  return <Badge variant={row.member.role === ACCOUNT_ROLE.owner ? "cyan" : "default"}>{row.member.role}</Badge>;
}

// Send again (a sent email may still never arrive), copy the link the invitee
// needs either way, and revoke: icons only, right-aligned with the destructive
// one last, as every other table's row actions are. With no mail relay there is
// nothing to send again (it could only answer 503); the copied link is the way
// in. The email status reads in the Time column.
function InviteActions({ invite, pending, onRevoke, onResend }: InviteHandlers & { invite: InviteSummary }) {
  return (
    <div className="flex justify-end gap-1">
      {invite.email_status === EMAIL_STATUS.unconfigured ? null : (
        <IconAction
          type="button"
          variant="ghost"
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
