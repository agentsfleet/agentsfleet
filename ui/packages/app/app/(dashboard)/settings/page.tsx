import { requireCredential } from "@/lib/auth/credential";
import { notFound } from "next/navigation";
import Link from "next/link";
import { Button, EmptyState, PageHeader, PageLayout, PageTitle } from "@agentsfleet/design-system";

const PLATFORM_NOTICES = new Set([
  "models-platform-admin-only",
  "runners-platform-admin-only",
  "fleet-libraries-platform-admin-only",
]);

export default async function SettingsPage({ searchParams }: {
  searchParams?: Promise<{ notice?: string }>;
} = {}) {
  await requireCredential();
  const query = searchParams ? await searchParams : {};
  if (PLATFORM_NOTICES.has(query.notice ?? "")) {
    return (
      <PageLayout>
        <PageHeader className="flex-col sm:flex-col"><PageTitle>Access restricted</PageTitle></PageHeader>
        <EmptyState
          title="Platform administrator access required"
          description="This page manages platform resources. Your workspace remains available."
          action={<Button asChild><Link href="/">Back to workspace</Link></Button>}
        />
      </PageLayout>
    );
  }
  notFound();
}
