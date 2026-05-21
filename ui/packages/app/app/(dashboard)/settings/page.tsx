import { auth } from "@clerk/nextjs/server";
import { redirect } from "next/navigation";
import Link from "next/link";
import { ChevronRightIcon, ZapIcon, WalletIcon, KeyRoundIcon } from "lucide-react";
import {
  Alert,
  AlertDescription,
  AlertTitle,
  DescriptionList,
  DescriptionTerm,
  DescriptionDetails,
  List,
  ListItem,
  PageHeader,
  PageTitle,
  Section,
  SectionLabel,
} from "@usezombie/design-system";
import { resolveActiveWorkspace } from "@/lib/workspace";

export const dynamic = "force-dynamic";

export default async function SettingsPage({
  searchParams,
}: {
  searchParams?: Promise<{ notice?: string }>;
} = {}) {
  const { getToken, userId } = await auth();
  const token = await getToken();
  if (!token) redirect("/sign-in");

  const workspace = await resolveActiveWorkspace(token);
  const { notice } = (await searchParams) ?? {};

  return (
    <div className="space-y-8">
      <PageHeader>
        <PageTitle>Settings</PageTitle>
      </PageHeader>

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

      <div className="grid grid-cols-1 gap-6 max-w-4xl md:grid-cols-2">
        <Section asChild className="min-w-0">
          <section aria-label="Workspace">
            <SectionLabel>Workspace</SectionLabel>
            <DescriptionList className="mt-3 break-all">
              <div>
                <DescriptionTerm>Name</DescriptionTerm>
                <DescriptionDetails>{workspace?.name ?? "—"}</DescriptionDetails>
              </div>
              <div>
                <DescriptionTerm>Workspace ID</DescriptionTerm>
                <DescriptionDetails mono>{workspace?.id ?? "—"}</DescriptionDetails>
              </div>
              <div>
                <DescriptionTerm>User ID</DescriptionTerm>
                <DescriptionDetails mono>{userId ?? "—"}</DescriptionDetails>
              </div>
            </DescriptionList>
          </section>
        </Section>
        <Section asChild className="min-w-0">
          <section aria-label="Settings index">
            <SectionLabel>More settings</SectionLabel>
            <List variant="plain" className="mt-3">
              <SettingsLink
                href="/settings/provider"
                icon={<ZapIcon size={16} />}
                label="LLM Provider"
                description="Platform-managed credits or Bring-Your-Own-Key."
              />
              <SettingsLink
                href="/settings/billing"
                icon={<WalletIcon size={16} />}
                label="Billing"
                description="Tenant balance, credit-pool charges, invoices."
              />
              <SettingsLink
                href="/settings/api-keys"
                icon={<KeyRoundIcon size={16} />}
                label="API keys"
                description="Mint, list, and revoke tenant API keys (zmb_t_…)."
              />
            </List>
          </section>
        </Section>
      </div>
    </div>
  );
}

function SettingsLink({
  href, icon, label, description,
}: { href: string; icon: React.ReactNode; label: string; description: string }) {
  return (
    <ListItem>
      <Link
        href={href}
        className="flex items-center justify-between gap-3 rounded-md border border-border p-3 transition-colors duration-200 ease-out hover:bg-accent/40 hover:border-primary/40"
      >
        <div className="flex items-start gap-3">
          <span className="mt-0.5 text-muted-foreground" aria-hidden>{icon}</span>
          <div>
            <div className="font-medium">{label}</div>
            <div className="text-xs text-muted-foreground">{description}</div>
          </div>
        </div>
        <ChevronRightIcon size={16} className="text-muted-foreground" aria-hidden />
      </Link>
    </ListItem>
  );
}
