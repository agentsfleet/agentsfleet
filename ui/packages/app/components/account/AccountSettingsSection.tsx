"use client";

import { Section, SectionHeader } from "@agentsfleet/design-system";
import { AccountControls } from "@/lib/auth/account-controls";
import { useAccountSettings } from "@/lib/auth/account-client";
import { AccountAccess } from "./AccountAccess";
import { ACCOUNT_SETTINGS_ID } from "./AccountSettingsAction";

/** Keep the named region mounted across verification panels and auth changes. */
export function AccountSettingsSection() {
  const access = useAccountSettings();
  return <Section id={ACCOUNT_SETTINGS_ID} aria-label="Profile and sign-in settings" tabIndex={-1} className="scroll-mt-2xl focus-visible:outline-2 focus-visible:outline-offset-4 focus-visible:outline-ring">
    <SectionHeader description="Manage your name, email addresses and sign-in methods.">Profile and sign-in settings</SectionHeader>
    {access.status === "ready" ? <AccountControls key={access.contextKey} /> : <AccountAccess status={access.status} />}
  </Section>;
}
