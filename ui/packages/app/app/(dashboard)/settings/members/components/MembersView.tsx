"use client";

import { useState, useTransition } from "react";
import { PageHeader, PageLayout, PageTitle, Section, SectionHeader } from "@agentsfleet/design-system";
import type { InviteSummary } from "@/lib/api/invites";
import type { MemberSummary } from "@/lib/api/tenant-members";
import { presentErrorString } from "@/lib/errors";
import InviteDialogDynamic from "@/components/domain/island-dynamic/InviteDialogDynamic";
import { loadTeamAction, removeMemberAction, revokeInviteAction, sendInviteEmailAction } from "../actions";
import { MEMBERS_DESCRIPTION, MEMBERS_TITLE } from "../copy";
import { CONFIRM_KIND, TeamConfirm, type ConfirmTarget, type ConfirmTargetActive } from "./TeamConfirm";
import { TEAM_CAPTION, TeamTable } from "./TeamTable";

type Props = {
  initialMembers: MemberSummary[];
  initialInvites: InviteSummary[];
};

// The lists and the one confirm dialog they share. Every mutation re-reads
// both lists afterwards, so the page mirrors the backend rather than guessing.
function useTeam({ initialMembers, initialInvites }: Props) {
  const [members, setMembers] = useState(initialMembers);
  const [invites, setInvites] = useState(initialInvites);
  const [target, setTarget] = useState<ConfirmTarget>(null);
  const [error, setError] = useState<string | null>(null);
  const [pending, startTransition] = useTransition();

  function refresh() {
    startTransition(async () => {
      const result = await loadTeamAction();
      if (!result.ok) {
        // A refusal set just before this reload says what to do; a failed
        // reload only says the lists may be stale, so it never replaces one.
        const reloadFailed = presentErrorString({ errorCode: result.errorCode, message: result.error, action: "reload this page's lists" });
        setError((current) => current ?? reloadFailed);
        return;
      }
      setMembers(result.data.members);
      setInvites(result.data.invites);
    });
  }

  // Settles once the request has answered. `ConfirmDialog` holds both of its
  // buttons while the promise it is handed runs, so a second confirm cannot
  // send a second request.
  function confirm(active: ConfirmTargetActive): Promise<void> {
    setError(null);
    const answered = Promise.withResolvers<void>();
    startTransition(async () => {
      try {
        const result =
          active.kind === CONFIRM_KIND.revoke
            ? await revokeInviteAction(active.invite.id)
            : await removeMemberAction(active.member.user_id);
        if (!result.ok) {
          const action = active.kind === CONFIRM_KIND.revoke ? "revoke the invite" : "remove the member";
          setError(presentErrorString({ errorCode: result.errorCode, message: result.error, action }));
          refresh();
          return;
        }
        setTarget(null);
        refresh();
      } finally {
        answered.resolve();
      }
    });
    return answered.promise;
  }

  // A new email attempt. Either way the lists are re-read, so the row shows the
  // status the backend recorded rather than the one this page hoped for.
  function resend(invite: InviteSummary) {
    setError(null);
    startTransition(async () => {
      const result = await sendInviteEmailAction(invite.id);
      if (!result.ok) {
        setError(presentErrorString({ errorCode: result.errorCode, message: result.error, action: "send the invite email" }));
      }
      refresh();
    });
  }

  function dismiss() {
    setTarget(null);
    setError(null);
  }

  return { members, invites, target, error, pending, setTarget, refresh, confirm, resend, dismiss };
}

export function MembersView(props: Props) {
  const team = useTeam(props);
  return (
    <PageLayout>
      <PageHeader description={MEMBERS_DESCRIPTION}>
        <PageTitle>{MEMBERS_TITLE}</PageTitle>
      </PageHeader>
      <Section asChild>
        <section aria-label={TEAM_CAPTION}>
          <SectionHeader as="p" actions={<InviteDialogDynamic onCreated={team.refresh} />}>
            {TEAM_CAPTION}
          </SectionHeader>
          <TeamTable
            members={team.members}
            invites={team.invites}
            pending={team.pending}
            onRemove={(member) => team.setTarget({ kind: CONFIRM_KIND.remove, member })}
            onRevoke={(invite) => team.setTarget({ kind: CONFIRM_KIND.revoke, invite })}
            onResend={team.resend}
          />
        </section>
      </Section>
      {team.error && team.target === null ? <p role="alert" className="text-sm text-destructive">{team.error}</p> : null}
      <TeamConfirm target={team.target} error={team.error} onOpenChange={team.dismiss} onConfirm={team.confirm} />
    </PageLayout>
  );
}
