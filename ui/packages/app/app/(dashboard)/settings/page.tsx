import { auth } from "@clerk/nextjs/server";
import { redirect } from "next/navigation";
import Link from "next/link";
import { Button, EmptyState, PageHeader, PageLayout, PageTitle } from "@agentsfleet/design-system";

const PLATFORM_NOTICES = new Set([
  "models-platform-admin-only",
  "runners-platform-admin-only",
  "fleet-libraries-platform-admin-only",
]);

// The Workspace tab folded into API Keys (workspace name/ID now lives there,
// switching/creating stays in the top-right WorkspaceSwitcher) — this route
// survives only so existing "/settings" links and bookmarks keep resolving.
// Checks auth itself (rather than deferring to /settings/api-keys) so an
// unauthenticated visit goes straight to /sign-in instead of a double redirect.
export default async function SettingsPage({ searchParams }: {
  searchParams?: Promise<{ notice?: string }>;
} = {}) {
  const { getToken } = await auth();
  const token = await getToken();
  if (!token) redirect("/sign-in");
  const query = searchParams ? await searchParams : {};
  if (PLATFORM_NOTICES.has(query.notice ?? "")) {
    return (
      <PageLayout>
        <PageHeader><PageTitle>Access restricted</PageTitle></PageHeader>
        <EmptyState
          title="Platform administrator access required"
          description="This page manages platform resources. Your workspace remains available."
          action={<Button asChild><Link href="/">Back to workspace</Link></Button>}
        />
      </PageLayout>
    );
  }
  redirect("/settings/api-keys");
}
