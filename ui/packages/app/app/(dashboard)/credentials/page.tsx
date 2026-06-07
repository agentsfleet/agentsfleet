import { redirect } from "next/navigation";
import {
  EmptyState,
  PageHeader,
  PageTitle,
  Section,
} from "@usezombie/design-system";
import { KeyRoundIcon } from "lucide-react";
import { auth } from "@clerk/nextjs/server";
import { resolveActiveWorkspace } from "@/lib/workspace";
import { listCredentials } from "@/lib/api/credentials";
import AddCredentialForm from "./components/AddCredentialForm";
import CredentialsList from "./components/CredentialsList";

export const dynamic = "force-dynamic";

export default async function CredentialsPage() {
  const { getToken } = await auth();
  const token = await getToken();
  if (!token) redirect("/sign-in");

  const workspace = await resolveActiveWorkspace(token);
  if (!workspace) {
    return (
      <div>
        <PageHeader>
          <PageTitle>Credentials</PageTitle>
        </PageHeader>
        <EmptyState
          icon={<KeyRoundIcon size={32} />}
          title="No workspace yet"
          description="Create a workspace before adding credentials."
        />
      </div>
    );
  }

  const { credentials } = await listCredentials(workspace.id, token).catch(() => ({
    credentials: [],
  }));

  return (
    <div>
      <PageHeader>
        <PageTitle>Credentials</PageTitle>
      </PageHeader>
      <p className="mb-6 max-w-2xl text-sm text-muted-foreground">
        Encrypted secrets your agents use to reach other services. Reference one by name, e.g.{" "}
        <code>{"${secrets.fly.api_token}"}</code>.
      </p>
      <div className="grid items-start gap-8 md:grid-cols-2">
        <Section asChild>
          <section aria-label="Stored credentials">
            <h2 className="mb-3 text-sm font-semibold uppercase tracking-wide text-muted-foreground">
              Stored credentials
            </h2>
            <CredentialsList workspaceId={workspace.id} credentials={credentials} />
          </section>
        </Section>
        <Section asChild>
          {/* UI GATE: SKIPPED per user override (reason: this is the asChild render target of the Section primitive; its landmark name must match the "Add credential" heading) */}
          <section aria-label="Add credential">
            <h2 className="mb-3 text-sm font-semibold uppercase tracking-wide text-muted-foreground">
              Add credential
            </h2>
            <AddCredentialForm workspaceId={workspace.id} />
          </section>
        </Section>
      </div>
    </div>
  );
}
