import { PageHeader, PageLayout, PageTitle, Skeleton } from "@agentsfleet/design-system";

export default function ProfileLoading() {
  return <PageLayout>
    <PageHeader description="Manage your personal details, sign-in methods, and active sessions."><PageTitle>Profile</PageTitle></PageHeader>
    <Skeleton className="h-96 w-full rounded-lg" aria-label="Loading profile" />
  </PageLayout>;
}
