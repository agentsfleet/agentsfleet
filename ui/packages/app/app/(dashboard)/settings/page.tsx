import { auth } from "@clerk/nextjs/server";
import { redirect } from "next/navigation";
import {
  Alert,
  AlertDescription,
  AlertTitle,
  DescriptionList,
  DescriptionTerm,
  DescriptionDetails,
  Section,
  SectionLabel,
} from "@usezombie/design-system";
import { resolveActiveWorkspace } from "@/lib/workspace";
import SettingsTabs from "@/components/layout/SettingsTabs";

export const dynamic = "force-dynamic";

export default async function SettingsPage({
  searchParams,
}: {
  searchParams?: Promise<{ notice?: string }>;
} = {}) {
  const { getToken } = await auth();
  const token = await getToken();
  if (!token) redirect("/sign-in");

  const workspace = await resolveActiveWorkspace(token);
  const { notice } = (await searchParams) ?? {};

  return (
    <div className="space-y-8">
      <SettingsTabs />

      {notice === "api-keys-operator-only" ? (
        <Alert variant="warning">
          <div>
            <AlertTitle>API keys need operator access</AlertTitle>
            <AlertDescription>
              Ask a tenant operator or admin to manage API keys.
            </AlertDescription>
          </div>
        </Alert>
      ) : null}

      <Section aria-label="Workspace" className="min-w-0 max-w-2xl">
        <SectionLabel>Workspace</SectionLabel>
        <DescriptionList layout="stacked" className="mt-3 break-all">
          <div>
            <DescriptionTerm>Name</DescriptionTerm>
            <DescriptionDetails>{workspace?.name ?? "—"}</DescriptionDetails>
          </div>
          <div>
            <DescriptionTerm>Workspace ID</DescriptionTerm>
            <DescriptionDetails mono>{workspace?.id ?? "—"}</DescriptionDetails>
          </div>
        </DescriptionList>
      </Section>
    </div>
  );
}
