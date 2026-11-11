"use client";

import type { AccountProfile, AccountSettingsClient } from "@/lib/auth/account-types";
import { useActionState, useId, useRef, useState } from "react";
import Link from "next/link";
import Image from "next/image";
import { Alert, Button, DashboardPanel, Input, Label, PageHeader, PageLayout, PageTitle, Section, SectionHeader } from "@agentsfleet/design-system";
import { useAccountSettings } from "@/lib/auth/account-client";
import { AccountAccess } from "./AccountAccess";

type Props = {profile: AccountProfile; client: AccountSettingsClient};
type Feedback = {kind: "idle"} | {kind: "saved"; firstName: string; lastName: string} | {kind: "failed"; error: string};

export function ProfileSettings() {
  const access = useAccountSettings();
  return access.status === "ready" ? <ProfileSettingsView key={access.contextKey} profile={access.profile} client={access.client} /> : <AccountAccess status={access.status} />;
}

export function ProfileSettingsView({profile, client}: Props) {
  return <PageLayout className="max-w-5xl">
    <PageHeader description="Your personal details and sign-in preferences."><PageTitle>Profile</PageTitle></PageHeader>
    <Section aria-label="Personal details">
      <SectionHeader>Personal details</SectionHeader>
      <DashboardPanel className="space-y-xl">
        <div className="flex min-w-0 items-center gap-lg">
          {profile.imageUrl ? <Image src={profile.imageUrl} alt="" width={44} height={44} unoptimized className="size-11 shrink-0 rounded-full object-cover" referrerPolicy="no-referrer" />
            : <span aria-hidden="true" className="flex size-11 shrink-0 items-center justify-center rounded-full bg-muted font-semibold">{(profile.firstName || profile.email || "?").slice(0, 1).toUpperCase()}</span>}
          <div className="min-w-0"><p className="break-words text-body font-medium">{[profile.firstName, profile.lastName].filter(Boolean).join(" ") || "Your profile"}</p>
            <p className="break-words text-label text-muted-foreground">{profile.email ?? "No primary email address"}</p></div>
        </div>
        <ProfileNameForm profile={profile} client={client} />
      </DashboardPanel>
    </Section>
    <Section aria-label="Sign-in methods">
      <SectionHeader>Sign-in methods</SectionHeader>
      <DashboardPanel className="flex flex-col items-start gap-lg sm:flex-row sm:items-center sm:justify-between">
        <p className="max-w-prose text-body text-muted-foreground">Manage your photo, email addresses, password, passkeys and two-step verification through your secure account controls.</p>
        <Button variant="outline" className="shrink-0" onClick={client.manageSignIn}>Manage account</Button>
      </DashboardPanel>
      <Button asChild variant="link" className="self-start"><Link href="/settings/security">View signed-in sessions</Link></Button>
    </Section>
  </PageLayout>;
}

function ProfileNameForm({profile, client}: Props) {
  const id = useId();
  const [form, setForm] = useState({source: {firstName: profile.firstName, lastName: profile.lastName}, draft: {firstName: profile.firstName, lastName: profile.lastName}});
  if (form.source.firstName !== profile.firstName || form.source.lastName !== profile.lastName) {
    setForm({source: {firstName: profile.firstName, lastName: profile.lastName}, draft: {
      firstName: form.draft.firstName.trim() === form.source.firstName.trim() ? profile.firstName : form.draft.firstName,
      lastName: form.draft.lastName.trim() === form.source.lastName.trim() ? profile.lastName : form.draft.lastName,
    }});
  }
  const {draft} = form;
  const busy = useRef(false);
  const [feedback, action, pending] = useActionState<Feedback, FormData>(async (_previous, data) => {
    try {
      const firstName = data.get("firstName"), lastName = data.get("lastName");
      if (typeof firstName !== "string" || typeof lastName !== "string") return {kind: "failed", error: "Enter a first and last name."};
      const result = await client.updateProfile({firstName, lastName});
      return result.ok ? {kind: "saved", firstName, lastName} : {kind: "failed", error: result.error};
    } catch {return {kind: "failed", error: "Your profile change could not be confirmed. Reload before trying again."};}
    finally {busy.current = false;}
  }, {kind: "idle"});
  return <div className="space-y-sm"><form action={action} aria-label="Edit personal details" className="space-y-lg"
    onSubmit={event => {if (busy.current) event.preventDefault(); else busy.current = true;}}>
    <div className="grid gap-lg sm:grid-cols-2">
      <div className="space-y-sm"><Label htmlFor={`${id}-first-name`}>First name</Label><Input id={`${id}-first-name`} name="firstName" autoComplete="given-name" value={draft.firstName} onChange={event => setForm(previous => ({...previous, draft: {...previous.draft, firstName: event.target.value}}))} maxLength={256} disabled={pending} /></div>
      <div className="space-y-sm"><Label htmlFor={`${id}-last-name`}>Last name</Label><Input id={`${id}-last-name`} name="lastName" autoComplete="family-name" value={draft.lastName} onChange={event => setForm(previous => ({...previous, draft: {...previous.draft, lastName: event.target.value}}))} maxLength={256} disabled={pending} /></div>
    </div>
    {feedback.kind === "failed" ? <Alert variant="destructive">{feedback.error}</Alert> : null}
    <div className="flex flex-wrap items-center justify-end gap-lg">
      <Button type="submit" disabled={pending}>{pending ? "Saving…" : "Save changes"}</Button>
    </div>
  </form><output className="block min-h-5 text-right text-label text-success">{feedback.kind === "saved" && feedback.firstName.trim() === draft.firstName.trim() && feedback.lastName.trim() === draft.lastName.trim() ? "Profile saved" : ""}</output></div>;
}
