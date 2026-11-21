"use client";

import type { AccountSession, AccountSettingsClient } from "@/lib/auth/account-types";
import { useState } from "react";
import { Alert, Badge, Button, ConfirmDialog, DashboardPanel, EmptyState, Section, SectionHeader, Time } from "@agentsfleet/design-system";
import { DesktopIcon, DeviceMobileIcon } from "@phosphor-icons/react/ssr";
import { AuthSignOutButton } from "@/lib/auth/client";
import { useAccountSettings } from "@/lib/auth/account-client";
import { SecurityResourceRow } from "./SecurityResourceRow";
import { AccountAccess } from "./AccountAccess";
import { useAccountSessions } from "./use-account-sessions";

export function SessionSettings() {
  const access = useAccountSettings();
  return access.status === "ready" ? <SessionSettingsView key={access.contextKey} client={access.client} /> : <AccountAccess status={access.status} />;
}

export function SessionSettingsView({client}: {client: AccountSettingsClient}) {
  const sessions = useAccountSessions(client);
  const [target, setTarget] = useState<AccountSession | "others" | null>(null);
  const {state} = sessions;
  const nextOffset = state.kind === "ready" ? state.page.nextOffset : null;
  const others = state.kind === "ready" && (nextOffset !== null || state.page.sessions.some(session => !session.current));
  return <>
    <Section aria-label="Active sessions">
      <SectionHeader description="Devices currently signed in to your account." actions={others ? <Button variant="outline" disabled={sessions.pending} onClick={() => setTarget("others")}>Sign out other sessions</Button> : null}>Active sessions</SectionHeader>
      {state.kind === "loading" ? <output className="text-label text-muted-foreground">Loading sessions…</output> : null}
      {state.kind === "ready" && state.page.sessions.length > 0 ? <DashboardPanel padding="none" className="divide-y divide-border">
        {state.page.sessions.map(session => <SessionRow key={session.id} session={session} disabled={sessions.pending} onRevoke={() => setTarget(session)} />)}
      </DashboardPanel> : null}
      {state.kind === "ready" && state.page.sessions.length === 0 ? <EmptyState title="No active browser sessions were returned" description="Refresh to check the latest session information." action={<Button variant="outline" disabled={sessions.pending} onClick={() => {void sessions.refresh();}}>Refresh sessions</Button>} /> : null}
      {state.kind !== "loading" && state.error ? <Alert variant="destructive" className="flex-col sm:flex-row sm:items-center">
        <span className="min-w-0 flex-1">{state.error}</span>
        <Button variant="outline" className="shrink-0 self-start" disabled={sessions.pending} onClick={() => {void sessions.refresh();}}>Refresh sessions</Button>
      </Alert> : null}
      {state.kind === "ready" && state.notice ? <output className="text-label text-success">{state.notice}</output> : null}
      {nextOffset !== null ? <Button variant="outline" className="self-start" disabled={sessions.pending}
        onClick={() => {void sessions.refresh(nextOffset, true);}}>Load more sessions</Button> : null}
    </Section>
    <ConfirmDialog open={target !== null} onOpenChange={() => setTarget(null)}
      title={target === "others" ? "Sign out other sessions?" : "Sign out this session?"} description={target && target !== "others" ? `${target.browser} on ${target.device} will need to sign in again.` : "Every other browser will need to sign in again. This device stays signed in."}
      intent="destructive" confirmLabel={target === "others" ? "Sign out other sessions" : "Sign out session"} errorMessage={state.kind !== "loading" ? state.error : null}
      onConfirm={target ? async () => {if (await (target === "others" ? sessions.revokeOthers() : sessions.revoke(target.id))) setTarget(null);} : undefined} />
  </>;
}

function SessionRow({session, disabled, onRevoke}: {session: AccountSession; disabled: boolean; onRevoke: () => void}) {
  const Icon = session.mobile ? DeviceMobileIcon : DesktopIcon;
  const title = `${session.browser} on ${session.device}`;
  const detail = <>{session.location && `${session.location} · `}Last active {session.lastActiveAt
    ? <Time value={session.lastActiveAt} format="relative" /> : "unavailable"}</>;
  const action = session.current
    ? <AuthSignOutButton redirectUrl="/sign-in"><Button variant="outline" disabled={disabled}>Sign out</Button></AuthSignOutButton>
    : <Button variant="outline" disabled={disabled} onClick={onRevoke} aria-label={`Sign out ${title}`}>Sign out</Button>;
  return <SecurityResourceRow kind="session" title={title} icon={<Icon size={16} />} detail={detail}
    badge={session.current && <Badge>This device</Badge>} action={action} />;
}
