"use client";

import { Badge, Button, CopyButton, DashboardPanel, DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuTrigger, Time } from "@agentsfleet/design-system";
import { DotsThreeIcon, EnvelopeSimpleIcon } from "@phosphor-icons/react/ssr";
import type { InviteSummary } from "@/lib/api/invites";
import { EMAIL_STATUS } from "@/lib/api/invites-types";
import type { MemberSummary } from "@/lib/api/tenant-members";
import { ACCOUNT_ROLE } from "@/lib/api/workspaces-types";
import { useCurrentUser } from "@/lib/auth/client";
import { memberName } from "./TeamConfirm";

export const TEAM_CAPTION = "People";
type Props = {
  members: MemberSummary[]; invites: InviteSummary[]; pending: boolean;
  onRemove: (member: MemberSummary) => void;
  onRevoke: (invite: InviteSummary) => void;
  onResend: (invite: InviteSummary) => void;
};
const ROW = [
  "grid min-w-0 grid-cols-[minmax(0,1fr)_44px] items-center gap-md p-xl",
  "min-[681px]:grid-cols-[36px_minmax(0,1fr)_72px_36px] min-[681px]:gap-x-lg min-[681px]:gap-y-1.5 min-[681px]:px-5 min-[681px]:py-4.5",
  "min-[901px]:grid-cols-[36px_minmax(0,1fr)_76px_148px_36px] min-[901px]:gap-x-xl min-[901px]:gap-y-sm",
].join(" ");
const AVATAR = "hidden size-9 shrink-0 items-center justify-center rounded-full text-eyebrow font-bold min-[681px]:flex";
const ROLE = "col-start-1 min-[681px]:col-start-3 min-[681px]:row-start-1 min-[901px]:col-auto min-[901px]:row-auto";
const DATE = "col-start-1 min-[681px]:col-start-2 min-[681px]:row-start-2 min-[901px]:col-auto min-[901px]:row-auto";
const ACTION = "col-start-2 row-start-1 min-[681px]:col-start-4 min-[901px]:col-auto min-[901px]:row-auto";

export function TeamTable(props: Props) {
  return <div className="space-y-2xl" aria-busy={props.pending}>
    {props.members.length > 0 ? <section aria-label="Members" className="space-y-lg">
      <Heading count={props.members.length}>Members</Heading>
      <DashboardPanel padding="none"><ul className="divide-y divide-border">
        {props.members.map(member => <MemberRow key={member.user_id} member={member} {...props} />)}
      </ul></DashboardPanel>
    </section> : null}
    {props.invites.length > 0 ? <section aria-label="Pending invitations" className="space-y-lg">
      <Heading count={props.invites.length}>Pending invitations</Heading>
      <DashboardPanel padding="none"><ul className="divide-y divide-border">
        {props.invites.map(invite => <InvitationRow key={invite.id} invite={invite} {...props} />)}
      </ul></DashboardPanel>
    </section> : null}
  </div>;
}

function Heading({children, count}: {children: string; count: number}) {
  return <h2 className="flex h-5 items-center gap-md text-label font-semibold">{children}<Badge className="h-6.75">{count}</Badge></h2>;
}

function MemberRow({member, pending, onRemove}: Pick<Props, "pending" | "onRemove"> & {member: MemberSummary}) {
  const {userId} = useCurrentUser();
  const owner = member.role === ACCOUNT_ROLE.owner;
  const self = member.user_id === userId;
  const name = memberName(member);
  const initials = name.split(/\s+/).slice(0, 2).map(part => part[0]).join("").toUpperCase();
  return <li aria-label={name} className={ROW}>
    <span className={`${AVATAR} ${self ? "bg-muted" : "bg-pulse/15 text-success"}`} aria-hidden="true">{initials}</span>
    <div className="min-w-0"><p className="break-words text-label font-semibold"><span>{name}</span>{self ? <span className="ml-md text-eyebrow font-normal leading-eyebrow text-text-dim">You</span> : null}</p>
      {name !== member.email ? <p className="mt-sm break-all text-label text-muted-foreground">{member.email}</p> : null}</div>
    <span className={`${ROLE} text-label capitalize text-muted-foreground`}>{member.role}</span>
    <span className={`${DATE} text-eyebrow leading-eyebrow text-text-dim`}>Joined <Time value={member.joined_at} format="date" tooltip /></span>
    {!owner ? <div className={ACTION}><DropdownMenu>
      <DropdownMenuTrigger asChild><Button variant="ghost" size="icon" className="min-[681px]:size-9 min-[681px]:min-h-9" disabled={pending} aria-label={`Manage ${name}`}><DotsThreeIcon size={16} /></Button></DropdownMenuTrigger>
      <DropdownMenuContent align="end"><DropdownMenuItem onSelect={() => onRemove(member)}>Remove {name}</DropdownMenuItem></DropdownMenuContent>
    </DropdownMenu></div> : null}
  </li>;
}

function InvitationRow({invite, pending, onResend, onRevoke}: Pick<Props, "pending" | "onResend" | "onRevoke"> & {invite: InviteSummary}) {
  const delivered = invite.email_status === EMAIL_STATUS.sent;
  const status = delivered ? "Invitation sent" : invite.email_status === EMAIL_STATUS.failed ? "Email not sent · Share the invite link" : "Email not set up · Share the invite link";
  return <li aria-label={invite.email} className={ROW}>
    <span className={`${AVATAR} border border-border bg-transparent text-muted-foreground`} aria-hidden="true"><EnvelopeSimpleIcon size={18} /></span>
    <div className="min-w-0"><p className="break-all text-label font-semibold">{invite.email}</p>
      <p className={`mt-sm text-label ${delivered ? "text-muted-foreground" : "text-warning"}`}>{status}</p></div>
    <span className={`${ROLE} text-label text-muted-foreground`}>Member</span>
    <span className={`${DATE} text-eyebrow leading-eyebrow text-text-dim`}>Expires <Time value={invite.expires_at} format="date" tooltip /></span>
    <div className={ACTION}><DropdownMenu>
      <DropdownMenuTrigger asChild><Button variant="ghost" size="icon" className="min-[681px]:size-9 min-[681px]:min-h-9" disabled={pending} aria-label={`Manage invite for ${invite.email}`}><DotsThreeIcon size={16} /></Button></DropdownMenuTrigger>
      <DropdownMenuContent align="end">
        {invite.email_status !== EMAIL_STATUS.unconfigured ? <DropdownMenuItem onSelect={() => onResend(invite)}>Send invite again</DropdownMenuItem> : null}
        <DropdownMenuItem asChild onSelect={event => event.preventDefault()}><CopyButton value={invite.link} label={`Copy invite link for ${invite.email}`} showLabel className="w-full justify-start border-0 bg-transparent" /></DropdownMenuItem>
        <DropdownMenuItem onSelect={() => onRevoke(invite)}>Revoke invite</DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu></div>
  </li>;
}
