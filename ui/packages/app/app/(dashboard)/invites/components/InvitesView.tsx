"use client";

import { useState, useTransition } from "react";
import { useRouter } from "next/navigation";
import {
  Alert,
  Button,
  Card,
  CardContent,
  DataTable,
  type DataTableColumn,
  EmptyState,
  IconAction,
  PageHeader,
  PageLayout,
  PageTitle,
  Time,
} from "@agentsfleet/design-system";
import { CheckIcon, MailOpenIcon } from "lucide-react";
import type { WaitingInvite } from "@/lib/api/invites";
import { presentErrorString } from "@/lib/errors";
import { DASHBOARD_ROOT_PATH, DEFAULT_WORKSPACE_SUBPATH, workspacePath } from "@/lib/workspace-routes";
import { accountLabel } from "@/components/layout/workspace-groups";
import { acceptInviteAction } from "../actions";
import { INVITES_DESCRIPTION, INVITES_TITLE } from "../copy";

const ACCEPT_LABEL = "Accept";

type Props = {
  waiting: WaitingInvite[];
  /** The invite a link named, or null on the plain Invites page. */
  linkedId: string | null;
};

// Accepting lands the person in the joined account's first workspace, which
// is where the invite was meant to take them.
function useAccept() {
  const router = useRouter();
  const [error, setError] = useState<string | null>(null);
  const [pending, startTransition] = useTransition();

  function accept(inviteId: string) {
    setError(null);
    startTransition(async () => {
      const result = await acceptInviteAction(inviteId);
      if (!result.ok) {
        setError(presentErrorString({ errorCode: result.errorCode, message: result.error, action: "accept the invite" }));
        return;
      }
      const first = result.data.workspace_ids[0];
      router.push(first ? workspacePath(first, DEFAULT_WORKSPACE_SUBPATH) : DASHBOARD_ROOT_PATH);
    });
  }

  return { accept, error, pending };
}

export function InvitesView({ waiting, linkedId }: Props) {
  const { accept, error, pending } = useAccept();
  const linkedUnlisted = linkedId !== null && !waiting.some((invite) => invite.id === linkedId);
  return (
    <PageLayout>
      <PageHeader description={INVITES_DESCRIPTION}>
        <PageTitle>{INVITES_TITLE}</PageTitle>
      </PageHeader>
      {error ? <Alert variant="destructive">{error}</Alert> : null}
      {linkedUnlisted ? (
        <Card>
          <CardContent className="flex flex-col gap-sm p-md sm:flex-row sm:items-center">
            <p className="min-w-0 flex-1 text-sm">
              You opened an invite link. Accept it to join the account that sent it.
            </p>
            <Button type="button" disabled={pending} onClick={() => accept(linkedId)}>{ACCEPT_LABEL}</Button>
          </CardContent>
        </Card>
      ) : null}
      {waiting.length > 0 || !linkedUnlisted ? (
        <DataTable
          columns={columns(pending, accept)}
          rows={waiting}
          rowKey={(invite) => invite.id}
          caption="Invites waiting for you"
          pagination={false}
          empty={
            <EmptyState
              icon={<MailOpenIcon size={28} />}
              title="No invites waiting"
              description="When someone invites your address into their account, it shows up here."
            />
          }
        />
      ) : null}
    </PageLayout>
  );
}

function columns(pending: boolean, accept: (inviteId: string) => void): DataTableColumn<WaitingInvite>[] {
  return [
    {
      key: "account",
      header: "Account",
      cell: (invite) => <span className="truncate text-sm">{accountLabel(invite.account.owner_name)}</span>,
    },
    {
      key: "time",
      header: "Time",
      hideOnMobile: true,
      cell: (invite) => (
        <span className="text-label leading-label text-muted-foreground">
          expires <Time value={new Date(invite.expires_at)} format="relative" className="tabular-nums" />
        </span>
      ),
    },
    {
      key: "actions",
      header: "Actions",
      numeric: true,
      cell: (invite) => (
        <IconAction
          type="button"
          disabled={pending}
          onClick={() => accept(invite.id)}
          label={`${ACCEPT_LABEL} invite into ${accountLabel(invite.account.owner_name)}`}
        >
          <CheckIcon size={14} />
        </IconAction>
      ),
    },
  ];
}
