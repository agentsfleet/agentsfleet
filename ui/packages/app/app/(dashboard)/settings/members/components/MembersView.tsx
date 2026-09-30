"use client";

import { useState, useTransition } from "react";
import { PageHeader, PageLayout, PageTitle, Section, SectionHeader } from "@agentsfleet/design-system";
import type { InviteSummary } from "@/lib/api/invites";
import type { MemberSummary } from "@/lib/api/tenant-members";
import { presentErrorString } from "@/lib/errors";
import { loadTeamAction, removeMemberAction, revokeInviteAction } from "../actions";
import { MEMBERS_DESCRIPTION, MEMBERS_TITLE } from "../copy";
import { InviteForm } from "./InviteForm";
import { InviteList } from "./InviteList";
import { MemberList } from "./MemberList";
import { CONFIRM_KIND, TeamConfirm, type ConfirmTarget, type ConfirmTargetActive } from "./TeamConfirm";

const SECTION = {
  invite: "Invite someone",
  invites: "Pending invites",
  people: "People",
} as const;

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
        setError(presentErrorString({ errorCode: result.errorCode, message: result.error, action: "reload this page's lists" }));
        return;
      }
      setMembers(result.data.members);
      setInvites(result.data.invites);
    });
  }

  function confirm(active: ConfirmTargetActive) {
    setError(null);
    startTransition(async () => {
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
    });
  }

  function dismiss() {
    setTarget(null);
    setError(null);
  }

  return { members, invites, target, error, pending, setTarget, refresh, confirm, dismiss };
}

export function MembersView(props: Props) {
  const team = useTeam(props);
  return (
    <PageLayout>
      <PageHeader description={MEMBERS_DESCRIPTION}>
        <PageTitle>{MEMBERS_TITLE}</PageTitle>
      </PageHeader>
      <Section asChild>
        <section aria-label={SECTION.invite}>
          <SectionHeader as="p">{SECTION.invite}</SectionHeader>
          <InviteForm onCreated={team.refresh} />
        </section>
      </Section>
      <Section asChild>
        <section aria-label={SECTION.invites}>
          <SectionHeader as="p">{SECTION.invites}</SectionHeader>
          <InviteList
            invites={team.invites}
            pending={team.pending}
            onRevoke={(invite) => team.setTarget({ kind: CONFIRM_KIND.revoke, invite })}
          />
        </section>
      </Section>
      <Section asChild>
        <section aria-label={SECTION.people}>
          <SectionHeader as="p">{SECTION.people}</SectionHeader>
          <MemberList
            members={team.members}
            pending={team.pending}
            onRemove={(member) => team.setTarget({ kind: CONFIRM_KIND.remove, member })}
          />
        </section>
      </Section>
      {team.error && team.target === null ? <p className="text-sm text-destructive">{team.error}</p> : null}
      <TeamConfirm target={team.target} error={team.error} onOpenChange={team.dismiss} onConfirm={team.confirm} />
    </PageLayout>
  );
}
