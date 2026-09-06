"use client";

import { PageHeader, PageLayout, PageTitle } from "@agentsfleet/design-system";
import { AuthUserProfile } from "@/lib/auth/client";
import { AUTH_APPEARANCE } from "@/lib/clerkAppearance";

export default function AccountPage() {
  return (
    <PageLayout>
      <PageHeader><PageTitle>Account settings</PageTitle></PageHeader>
      <AuthUserProfile routing="path" path="/settings/account" appearance={AUTH_APPEARANCE} />
    </PageLayout>
  );
}
