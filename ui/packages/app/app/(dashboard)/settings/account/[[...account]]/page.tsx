"use client";

import { RouteLoading } from "@/components/layout/RouteLoading";

import { PageHeader, PageLayout, PageTitle } from "@agentsfleet/design-system";
import { AuthUserProfile } from "@/lib/auth/client";
import { PROFILE_APPEARANCE } from "@/lib/profile-appearance";

export default function AccountPage() {
  return (
    <PageLayout>
      <PageHeader description="Manage your personal details, sign-in methods, and active sessions."><PageTitle>Profile</PageTitle></PageHeader>
      <AuthUserProfile routing="path" path="/settings/account" appearance={PROFILE_APPEARANCE}
        fallback={<RouteLoading />} />
    </PageLayout>
  );
}
