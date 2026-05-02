import { getServerAuth } from "@/lib/auth/server";
import { notFound } from "next/navigation";
import Link from "next/link";
import { ChevronRightIcon, ZapIcon, WalletIcon } from "lucide-react";
import { PageHeader, PageTitle, Section, SectionLabel } from "@usezombie/design-system";
import { resolveActiveWorkspace } from "@/lib/workspace";

export const dynamic = "force-dynamic";

export default async function SettingsPage() {
  const { token, userId } = await getServerAuth();
  if (!token) notFound();

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
            <dl className="mt-3 space-y-3 text-sm">
              <div className="flex justify-between">
                <dt className="text-muted-foreground">Name</dt>
                <dd>{workspace?.name ?? "—"}</dd>
              </div>
              <div className="flex justify-between">
                <dt className="text-muted-foreground">Workspace ID</dt>
                <dd className="font-mono">{workspace?.id ?? "—"}</dd>
              </div>
              <div className="flex justify-between">
                <dt className="text-muted-foreground">User ID</dt>
                <dd className="font-mono">{userId ?? "—"}</dd>
              </div>
            </dl>
          </section>
        </Section>
        <Section asChild>
          <section aria-label="Settings index">
            <SectionLabel>More settings</SectionLabel>
            <ul className="mt-3 space-y-2 text-sm">
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
            </ul>
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
    <li>
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
    </li>
  );
}
