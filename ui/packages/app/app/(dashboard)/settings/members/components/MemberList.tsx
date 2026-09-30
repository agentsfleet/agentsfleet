"use client";

import { Badge, DataTable, type DataTableColumn, IconAction } from "@agentsfleet/design-system";
import { UserMinusIcon } from "lucide-react";
import { ACCOUNT_ROLE } from "@/lib/api/workspaces";
import type { MemberSummary } from "@/lib/api/tenant-members";
import { memberName } from "./TeamConfirm";

type Props = {
  members: MemberSummary[];
  pending: boolean;
  onRemove: (member: MemberSummary) => void;
};

// The owner is always listed (the account is theirs), so the table is never
// empty. Only members carry a remove action: the account's one owner cannot be
// removed, which the backend also refuses with 409 `UZ-INV-004`.
export function MemberList({ members, pending, onRemove }: Props) {
  return (
    <DataTable
      columns={columns(pending, onRemove)}
      rows={members}
      rowKey={(member) => member.user_id}
      caption="People"
      pagination={false}
    />
  );
}

function PersonCell({ member }: { member: MemberSummary }) {
  return (
    <div className="min-w-0">
      <div className="truncate text-sm">{memberName(member)}</div>
      {member.display_name ? (
        <div className="truncate text-label text-muted-foreground">{member.email}</div>
      ) : null}
    </div>
  );
}

function columns(pending: boolean, onRemove: (member: MemberSummary) => void): DataTableColumn<MemberSummary>[] {
  return [
    {
      key: "person",
      header: "Person",
      cell: (member) => <PersonCell member={member} />,
    },
    {
      key: "role",
      header: "Role",
      cell: (member) => (
        <Badge variant={member.role === ACCOUNT_ROLE.owner ? "cyan" : "default"}>{member.role}</Badge>
      ),
    },
    {
      key: "actions",
      header: "Actions",
      numeric: true,
      cell: (member) =>
        member.role === ACCOUNT_ROLE.member ? (
          <IconAction
            type="button"
            variant="destructive"
            disabled={pending}
            onClick={() => onRemove(member)}
            label={`Remove ${memberName(member)}`}
          >
            <UserMinusIcon size={14} />
          </IconAction>
        ) : null,
    },
  ];
}
