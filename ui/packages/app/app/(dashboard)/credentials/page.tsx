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
      <p className="mb-6 text-sm text-muted-foreground">
        Each credential is a JSON object stored in the vault, envelope-encrypted at rest.
        Reference fields from a zombie&apos;s tool calls as
        {" "}<code>{"${secrets.<name>.<field>}"}</code>.
      </p>
      <div className="grid gap-8 md:grid-cols-2">
        <Section asChild>
          <section aria-label="Stored credentials">
            <h2 className="mb-3 text-sm font-semibold uppercase tracking-wide text-muted-foreground">
              Stored credentials
            </h2>
            <CredentialsList workspaceId={workspace.id} credentials={credentials} />
          </section>
        </Section>
        <Section asChild>
          <section aria-label="Add a credential">
            <h2 className="mb-3 text-sm font-semibold uppercase tracking-wide text-muted-foreground">
              Add a credential
            </h2>
            <AddCredentialForm workspaceId={workspace.id} />
          </section>
        </Section>
      </div>
    </div>
  );
}
