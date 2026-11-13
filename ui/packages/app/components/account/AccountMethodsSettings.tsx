"use client";

import { Section } from "@agentsfleet/design-system";
import { AccountControls } from "@/lib/auth/account-controls";
import { useAccountSettings } from "@/lib/auth/account-client";
import { AccountAccess } from "./AccountAccess";

export function AccountMethodsSettings() {
  const access = useAccountSettings();
  return <Section aria-label="Secure account controls">
    {access.status === "ready" ? <AccountControls key={access.contextKey} /> : <AccountAccess status={access.status} />}
  </Section>;
}
