"use client";

import { Button } from "@agentsfleet/design-system";

export const ACCOUNT_SETTINGS_ID = "account-sign-in-settings";

/** Clerk owns the hash; navigation to this section changes only focus/scroll. */
export function AccountSettingsAction() {
  return <Button variant="secondary" onClick={() => {
    const section = document.getElementById(ACCOUNT_SETTINGS_ID);
    section?.focus({preventScroll: true});
    section?.scrollIntoView({block: "start"});
  }}>Profile and sign-in settings</Button>;
}
