import { auth } from "@clerk/nextjs/server";
import { redirect } from "next/navigation";
import { EmptyState, PageHeader, PageTitle } from "@agentsfleet/design-system";
import { ShieldIcon } from "lucide-react";

export const dynamic = "force-dynamic";

export default async function SettingsSecurityPage() {
  const { getToken } = await auth();
  const token = await getToken();
  if (!token) redirect("/sign-in");

  return (
    <div className="space-y-8">
      <PageHeader>
        <PageTitle>Security</PageTitle>
      </PageHeader>
      <EmptyState
        icon={<ShieldIcon size={32} />}
        title="Security"
        description="Security and access policy for this workspace will live here."
      />
    </div>
  );
}
