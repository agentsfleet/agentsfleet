"use client";

import type { AccountSession, AccountSettingsClient } from "@/lib/auth/account-types";
import { useState } from "react";
import Link from "next/link";
import { Alert, Badge, Button, ConfirmDialog, DashboardPanel, EmptyState, PageHeader, PageLayout, PageTitle, Section, SectionHeader, Time } from "@agentsfleet/design-system";
import { DesktopIcon, DeviceMobileIcon } from "@phosphor-icons/react/ssr";
import { AuthSignOutButton } from "@/lib/auth/client";
import { useAccountSettings } from "@/lib/auth/account-client";
import { AccountAccess } from "./AccountAccess";
import { useAccountSessions } from "./use-account-sessions";

export function SessionSettings() {
  const access = useAccountSettings();
  return access.status === "ready" ? <SessionSettingsView key={access.contextKey} client={access.client} /> : <AccountAccess status={access.status} />;
}

export function SessionSettingsView({client}: {client: AccountSettingsClient}) {
  const sessions = useAccountSessions(client);
  const [target, setTarget] = useState<AccountSession | null>(null);
  const {state} = sessions;
  const nextOffset = state.kind === "ready" ? state.page.nextOffset : null;
  return <PageLayout className="max-w-5xl">
    <PageHeader description="Manage the browsers signed in to your personal account."
      actions={<Button variant="outline" onClick={client.manageSignIn}>Sign-in methods</Button>}><PageTitle>Security</PageTitle></PageHeader>
    <Section aria-label="Active sessions">
      <SectionHeader actions={<Button variant="outline" disabled={sessions.pending} onClick={() => {void sessions.refresh();}}>Refresh sessions</Button>}>Active sessions</SectionHeader>
      <p className="text-body text-muted-foreground">These are browser sign-ins. Last active records session activity, rather than the time of the original sign-in.</p>
      {state.kind === "loading" ? <output className="text-label text-muted-foreground">Loading sessions…</output> : null}
      {state.kind === "ready" && state.page.sessions.length > 0 ? <DashboardPanel padding="none" className="divide-y divide-border">
        {state.page.sessions.map(session => <SessionRow key={session.id} session={session} disabled={sessions.pending} onRevoke={() => setTarget(session)} />)}
      </DashboardPanel> : null}
      {state.kind === "ready" && state.page.sessions.length === 0 ? <EmptyState title="No active browser sessions were returned" description="Refresh to check the latest session information." /> : null}
      {state.kind !== "loading" && state.error ? <Alert variant="destructive">{state.error}</Alert> : null}
      {state.kind === "ready" && state.notice ? <output className="text-label text-success">{state.notice}</output> : null}
      {nextOffset !== null ? <Button variant="outline" className="self-start" disabled={sessions.pending}
        onClick={() => {void sessions.refresh(nextOffset, true);}}>Load more sessions</Button> : null}
    </Section>
    <p className="text-label text-muted-foreground">Automation credentials are managed separately in <Link href="/settings/api-keys" className="underline underline-offset-2">API keys</Link>.</p>
    <ConfirmDialog open={target !== null} onOpenChange={() => setTarget(null)}
      title="Sign out this session?" description={target ? `${target.browser} on ${target.device} will need to sign in again. Your current session stays signed in.` : "Your current session stays signed in."}
      intent="destructive" confirmLabel="Sign out session" errorMessage={state.kind !== "loading" ? state.error : null}
      onConfirm={target ? async () => {if (await sessions.revoke(target.id)) setTarget(null);} : undefined} />
  </PageLayout>;
}

function SessionRow({session, disabled, onRevoke}: {session: AccountSession; disabled: boolean; onRevoke: () => void}) {
  const Icon = session.mobile ? DeviceMobileIcon : DesktopIcon;
  return <div className="flex min-w-0 flex-col items-start gap-lg px-xl py-lg sm:flex-row sm:items-center">
    <div className="flex min-w-0 flex-1 items-start gap-lg">
      <Icon className="mt-0.5 size-4 shrink-0 text-muted-foreground" aria-hidden="true" />
      <div className="min-w-0 space-y-sm"><div className="flex flex-wrap items-center gap-sm">
        <span className="break-words text-body font-medium">{session.browser} on {session.device}</span>{session.current ? <Badge>This device</Badge> : null}
      </div><p className="break-words text-label text-muted-foreground">{session.location ? `${session.location} · ` : ""}Last active {session.lastActiveAt ? <Time value={session.lastActiveAt} format="relative" /> : "unavailable"}</p></div>
    </div>
    {session.current ? <AuthSignOutButton redirectUrl="/sign-in"><Button variant="outline" disabled={disabled}>Sign out</Button></AuthSignOutButton>
      : <Button variant="outline" disabled={disabled} onClick={onRevoke} aria-label={`Sign out ${session.browser} on ${session.device}`}>Sign out</Button>}
  </div>;
}
