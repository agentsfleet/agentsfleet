import { auth } from "@clerk/nextjs/server";
import { redirect } from "next/navigation";
import { EmptyState, PageHeader, PageTitle } from "@agentsfleet/design-system";
import { SlidersHorizontalIcon } from "lucide-react";

export const dynamic = "force-dynamic";

export default async function SettingsDefaultsPage() {
  const { getToken } = await auth();
  const token = await getToken();
  if (!token) redirect("/sign-in");

  return (
    <div className="space-y-8">
      <PageHeader>
        <PageTitle>Defaults</PageTitle>
      </PageHeader>
      <EmptyState
        icon={<SlidersHorizontalIcon size={32} />}
        title="Defaults"
        description="Workspace-wide defaults for new Fleets will live here."
      />
    </div>
  );
}
