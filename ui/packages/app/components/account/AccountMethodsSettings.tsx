"use client";

import Link from "next/link";
import { Button, PageHeader, PageLayout, PageTitle, Section } from "@agentsfleet/design-system";
import { AccountControls } from "@/lib/auth/account-controls";
import { useAccountSettings } from "@/lib/auth/account-client";
import { AccountAccess } from "./AccountAccess";

export function AccountMethodsSettings() {
  const access = useAccountSettings();
  return <PageLayout className="max-w-5xl">
    <PageHeader description="Manage your profile, verified email addresses and sign-in methods."
      actions={<Button asChild variant="outline"><Link href="/settings/security">Back to Security</Link></Button>}>
      <PageTitle>Personal account</PageTitle>
    </PageHeader>
    <Section aria-label="Secure account controls">
      {access.status === "ready" ? <AccountControls key={access.contextKey} /> : <AccountAccess status={access.status} />}
    </Section>
  </PageLayout>;
}
