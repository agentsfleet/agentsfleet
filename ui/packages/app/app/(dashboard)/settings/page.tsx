import { getServerAuth } from "@/lib/auth/server";
import { redirect } from "next/navigation";
import Link from "next/link";
import { ChevronRightIcon, ZapIcon, WalletIcon } from "lucide-react";
import {
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

export default async function SettingsPage() {
  const { token, userId } = await getServerAuth();
  if (!token) redirect("/sign-in");

  const workspace = await resolveActiveWorkspace(token);

  return (
    <div>
      <PageHeader>
        <PageTitle>Settings</PageTitle>
      </PageHeader>

      <div className="grid gap-6 md:grid-cols-2 max-w-4xl">
        <Section asChild>
          <section aria-label="Workspace">
            <SectionLabel>Workspace</SectionLabel>
            <DescriptionList className="mt-3">
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
        <Section asChild>
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
