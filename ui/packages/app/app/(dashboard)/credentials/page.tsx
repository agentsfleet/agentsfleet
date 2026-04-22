import { EmptyState, PageHeader, PageTitle } from "@usezombie/design-system";
import { KeyRoundIcon } from "lucide-react";

export default function CredentialsPage() {
  return (
    <div>
      <PageHeader>
        <PageTitle>Credentials</PageTitle>
      </PageHeader>
      <EmptyState
        icon={<KeyRoundIcon size={32} />}
        title="Credential vault"
        description="Stored secrets for your zombies will appear here once the credential vault ships."
      />
    </div>
  );
}
