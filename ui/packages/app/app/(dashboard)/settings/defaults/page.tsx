import { auth } from "@clerk/nextjs/server";
import { redirect } from "next/navigation";
import { EmptyState } from "@usezombie/design-system";
import { SlidersHorizontalIcon } from "lucide-react";
import SettingsTabs from "@/components/layout/SettingsTabs";

export const dynamic = "force-dynamic";

export default async function SettingsDefaultsPage() {
  const { getToken } = await auth();
  const token = await getToken();
  if (!token) redirect("/sign-in");

  return (
    <div className="space-y-8">
      <SettingsTabs />
      <EmptyState
        icon={<SlidersHorizontalIcon size={32} />}
        title="Defaults"
        description="Workspace-wide defaults for new agents will live here."
      />
    </div>
  );
}
