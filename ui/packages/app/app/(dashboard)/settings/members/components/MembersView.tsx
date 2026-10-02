"use client";

import { type TransitionStartFunction, useEffect, useRef, useState, useTransition } from "react";
import { Alert, PageHeader, PageLayout, PageTitle, Section, SectionHeader } from "@agentsfleet/design-system";
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

// The two lists, re-read from the backend after every change so the page
// mirrors it rather than guessing. `afterReload` runs once a reload has
// replaced them.
function useTeamLists({ initialMembers, initialInvites }: Props, startTransition: TransitionStartFunction) {
  const [members, setMembers] = useState(initialMembers);
  const [invites, setInvites] = useState(initialInvites);
  const [error, setError] = useState<string | null>(null);

  function refresh(afterReload?: () => void) {
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
      afterReload?.();
    });
  }

  return { members, invites, error, setError, refresh };
}

type TeamLists = ReturnType<typeof useTeamLists>;

// The confirm dialog the table shares and the changes the page makes. Each
// change re-reads the lists, so a row shows what the backend recorded.
// `onRemoved` runs once a removal or revoke has landed and its row is gone.
function useTeamChanges({ setError, refresh }: TeamLists, startTransition: TransitionStartFunction, onRemoved: () => void) {
  const [target, setTarget] = useState<ConfirmTarget>(null);

  // Settles once the request has answered. `ConfirmDialog` holds both of its
  // buttons while the promise it is handed runs, so a second confirm cannot
  // send a second request.
  function confirm(active: ConfirmTargetActive): Promise<void> {
    setError(null);
    const answered = Promise.withResolvers<void>();
    startTransition(async () => {
      try {
        const revoking = active.kind === CONFIRM_KIND.revoke;
        const result = revoking ? await revokeInviteAction(active.invite.id) : await removeMemberAction(active.member.user_id);
        if (!result.ok) {
          const action = revoking ? "revoke the invite" : "remove the member";
          setError(presentErrorString({ errorCode: result.errorCode, message: result.error, action }));
          refresh();
          return;
        }
        setTarget(null);
        refresh(onRemoved);
      } finally {
        answered.resolve();
      }
    });
    return answered.promise;
  }

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

  return { target, setTarget, confirm, resend, dismiss };
}

// One transition for the lists and the changes, so a control stays held until
// the reload behind its change has landed too.
function useTeam(props: Props, onRemoved: () => void) {
  const [pending, startTransition] = useTransition();
  const lists = useTeamLists(props, startTransition);
  const changes = useTeamChanges(lists, startTransition, onRemoved);
  return { ...lists, ...changes, pending };
}

// The row whose button opened the dialog is gone after a removal, and focus
// would fall to the page with it; it lands on the team's table instead.
function useFocusAfterRemoval() {
  const regionRef = useRef<HTMLDivElement>(null);
  const [removals, setRemovals] = useState(0);
  useEffect(() => {
    if (removals > 0) regionRef.current?.focus();
  }, [removals]);
  return { regionRef, onRemoved: () => setRemovals((count) => count + 1) };
}

export function MembersView(props: Props) {
  const { regionRef, onRemoved } = useFocusAfterRemoval();
  const team = useTeam(props, onRemoved);
  return (
    <PageLayout>
      <PageHeader description={MEMBERS_DESCRIPTION}>
        <PageTitle>{MEMBERS_TITLE}</PageTitle>
      </PageHeader>
      {/* Reachable by script only, for the focus a removal hands back. A screen
          reader announces it by its label; no outline marks it for a mouse. */}
      <Section ref={regionRef} tabIndex={-1} aria-label={TEAM_CAPTION} className="focus:outline-none">
        <SectionHeader as="p" actions={<InviteDialogDynamic onSettled={team.refresh} />}>
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
      </Section>
      {team.error && team.target === null ? <Alert variant="destructive">{team.error}</Alert> : null}
      <TeamConfirm target={team.target} error={team.error} onOpenChange={team.dismiss} onConfirm={team.confirm} />
    </PageLayout>
  );
}
