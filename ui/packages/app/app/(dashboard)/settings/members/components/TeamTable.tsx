"use client";

import type { ReactNode } from "react";
import { Badge, Button, CopyButton, DashboardPanel, DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuTrigger, Time } from "@agentsfleet/design-system";
import { DotsThreeIcon, EnvelopeSimpleIcon, PaperPlaneTiltIcon, TrashIcon } from "@phosphor-icons/react/ssr";
import type { InviteSummary } from "@/lib/api/invites";
import { EMAIL_STATUS } from "@/lib/api/invites-types";
import type { MemberSummary } from "@/lib/api/tenant-members";
import { ACCOUNT_ROLE } from "@/lib/api/workspaces-types";
import { memberName } from "./TeamConfirm";

export const TEAM_CAPTION = "People";
type Props = {
  viewerUserId: string;
  memberCount: number; inviteCount: number; searching: boolean;
  members: MemberSummary[]; invites: InviteSummary[]; pending: boolean;
  onRemove: (member: MemberSummary) => void;
  onRevoke: (invite: InviteSummary) => void;
  onResend: (invite: InviteSummary) => void;
};
export function TeamTable({members, invites, memberCount, inviteCount, searching, pending, viewerUserId, onRemove, onRevoke, onResend}: Props) {
  return <div className="space-y-2xl" aria-busy={pending}>
    <RosterSection title="Members" count={memberCount} empty={searching ? "No members match your search." : "No members."}>
      {members.map(member => <MemberRow key={member.user_id} member={member} pending={pending} viewerUserId={viewerUserId} onRemove={onRemove} />)}
    </RosterSection>
    <RosterSection title="Pending invitations" count={inviteCount} empty={searching ? "No invitations match your search." : "No pending invitations."}>
      {invites.map(invite => <InvitationRow key={invite.id} invite={invite} pending={pending} onResend={onResend} onRevoke={onRevoke} />)}
    </RosterSection>
  </div>;
}

function RosterSection({title, count, empty, children}: {title: string; count: number; empty: string; children: ReactNode[]}) {
  return <section aria-label={title} className="space-y-lg">
    <h2 className="flex h-5 items-center gap-lg text-label font-semibold">
      {title}<Badge className="h-6.75 min-w-6.25 justify-center tabular-nums">{count}</Badge>
    </h2>
    <DashboardPanel padding="none">
      {children.length ? <ul className="divide-y divide-border">{children}</ul> :
        <p className="p-2xl text-label text-muted-foreground">{empty}</p>}
    </DashboardPanel>
  </section>;
}

function MemberRow({member, pending, onRemove, viewerUserId}: Pick<Props, "pending" | "onRemove" | "viewerUserId"> & {member: MemberSummary}) {
  const owner = member.role === ACCOUNT_ROLE.owner;
  const self = member.user_id === viewerUserId;
  const name = memberName(member);
  const initials = name.split(/\s+/).slice(0, 2).map(part => part[0]).join("").toUpperCase();
  return <li aria-label={name} className="team-row">
    <span className={`team-avatar ${self ? "bg-muted" : "bg-pulse/15 text-success"}`} aria-hidden="true">{initials}</span>
    <div className="min-w-0"><p className="break-words text-label font-semibold"><span>{name}</span>{self ? <span className="ml-md text-eyebrow font-normal leading-eyebrow text-text-dim">You</span> : null}</p>
      {name !== member.email ? <p className="mt-sm break-all text-label text-muted-foreground">{member.email}</p> : null}</div>
    <span className="team-role capitalize">{member.role}</span>
    <span className="team-date">Joined <Time value={member.joined_at} format="date" tooltip /></span>
    {!owner ? <div className="team-row-action"><DropdownMenu>
      <DropdownMenuTrigger asChild><Button variant="ghost" size="icon-row" className="text-foreground max-summary:size-11" disabled={pending} aria-label={`Manage ${name}`}><DotsThreeIcon aria-hidden="true" /></Button></DropdownMenuTrigger>
      <DropdownMenuContent align="end"><DropdownMenuItem onSelect={() => onRemove(member)}>Remove {name}</DropdownMenuItem></DropdownMenuContent>
    </DropdownMenu></div> : null}
  </li>;
}

function InvitationRow({invite, pending, onResend, onRevoke}: Pick<Props, "pending" | "onResend" | "onRevoke"> & {invite: InviteSummary}) {
  const delivered = invite.email_status === EMAIL_STATUS.sent;
  const status = delivered ? "Invitation sent" : invite.email_status === EMAIL_STATUS.failed ? "Email not sent · Share the invite link" : "Email not set up · Share the invite link";
  return <li aria-label={invite.email} className="team-row">
    <span className="team-avatar border border-border bg-transparent text-muted-foreground" aria-hidden="true"><EnvelopeSimpleIcon size={18} /></span>
    <div className="min-w-0"><p className="break-all text-label font-semibold">{invite.email}</p>
      <p className={`mt-sm text-label ${delivered ? "text-muted-foreground" : "text-warning"}`}>{status}</p></div>
    <span className="team-role">Member</span>
    <span className="team-date">Expires <Time value={invite.expires_at} format="date" tooltip /></span>
    <div className="team-row-action"><DropdownMenu>
      <DropdownMenuTrigger asChild><Button variant="ghost" size="icon-row" className="text-foreground max-summary:size-11" disabled={pending} aria-label={`Manage invite for ${invite.email}`}><DotsThreeIcon aria-hidden="true" /></Button></DropdownMenuTrigger>
      <DropdownMenuContent align="end">
        {invite.email_status !== EMAIL_STATUS.unconfigured ? <DropdownMenuItem onSelect={() => onResend(invite)}><PaperPlaneTiltIcon aria-hidden="true" />Send invite again</DropdownMenuItem> : null}
        <DropdownMenuItem asChild onSelect={event => event.preventDefault()}><CopyButton value={invite.link} label={`Copy invite link for ${invite.email}`} showLabel className="w-full justify-start border-0 bg-transparent" /></DropdownMenuItem>
        <DropdownMenuItem className="text-destructive focus:text-destructive" onSelect={() => onRevoke(invite)}><TrashIcon aria-hidden="true" />Revoke invite</DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu></div>
  </li>;
}
