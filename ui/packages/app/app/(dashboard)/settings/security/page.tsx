import { auth } from "@clerk/nextjs/server";
import { redirect } from "next/navigation";
import { EmptyState } from "@usezombie/design-system";
import { ShieldIcon } from "lucide-react";
import SettingsTabs from "@/components/layout/SettingsTabs";

export const dynamic = "force-dynamic";

export default async function SettingsSecurityPage() {
  const { getToken } = await auth();
  const token = await getToken();
  if (!token) redirect("/sign-in");

  return (
    <div className="space-y-8">
      <SettingsTabs />
      <EmptyState
        icon={<ShieldIcon size={32} />}
        title="Security"
        description="Security and access policy for this workspace will live here."
      />
    </div>
  );
}
