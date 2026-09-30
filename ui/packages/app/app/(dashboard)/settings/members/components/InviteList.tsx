"use client";

import { CopyButton, DataTable, type DataTableColumn, EmptyState, IconAction, Time } from "@agentsfleet/design-system";
import { BanIcon, MailIcon } from "lucide-react";
import type { InviteSummary } from "@/lib/api/invites";

type Props = {
  invites: InviteSummary[];
  pending: boolean;
  onRevoke: (invite: InviteSummary) => void;
};

export function InviteList({ invites, pending, onRevoke }: Props) {
  return (
    <DataTable
      columns={columns(pending, onRevoke)}
      rows={invites}
      rowKey={(invite) => invite.id}
      caption="Pending invites"
      pagination={false}
      empty={
        <EmptyState
          icon={<MailIcon size={28} />}
          title="No pending invites"
          description="Invite someone by email and they can open every workspace in your account."
        />
      }
    />
  );
}

function columns(pending: boolean, onRevoke: (invite: InviteSummary) => void): DataTableColumn<InviteSummary>[] {
  return [
    {
      key: "email",
      header: "Email",
      cell: (invite) => <span className="truncate text-sm">{invite.email}</span>,
    },
    {
      key: "expires",
      header: "Expires",
      hideOnMobile: true,
      cell: (invite) => (
        <Time value={new Date(invite.expires_at)} format="relative" className="text-label tabular-nums text-muted-foreground" />
      ),
    },
    {
      key: "actions",
      header: "Actions",
      numeric: true,
      cell: (invite) => (
        <div className="inline-flex items-center gap-xs">
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
      ),
    },
  ];
}
