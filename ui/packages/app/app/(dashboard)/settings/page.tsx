import { getServerAuth } from "@/lib/auth/server";
import { notFound } from "next/navigation";
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

      <Section asChild>
        <section aria-label="Workspace" className="max-w-lg">
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
    </div>
  );
}
