"use client";

import Link from "next/link";
import { Button, PageHeader, PageLayout, PageTitle, Section } from "@agentsfleet/design-system";
import { AccountControls } from "@/lib/auth/account-controls";
import { useAccountSettings } from "@/lib/auth/account-client";
import { AccountAccess } from "./AccountAccess";

export function AccountMethodsSettings() {
  const access = useAccountSettings();
  return <PageLayout className="max-w-5xl">
    <PageHeader description="Manage your verified email addresses, password, passkeys and two-step verification."
      actions={<Button asChild variant="outline"><Link href="/settings/security">Back to Security</Link></Button>}>
      <PageTitle>Sign-in methods</PageTitle>
    </PageHeader>
    <Section aria-label="Secure account controls">
      {access.status === "ready" ? <AccountControls key={access.contextKey} /> : <AccountAccess status={access.status} />}
    </Section>
  </PageLayout>;
}
