import { EmptyState, PageHeader, PageTitle } from "@usezombie/design-system";
import { ShieldIcon } from "lucide-react";

export default function FirewallPage() {
  return (
    <div>
      <PageHeader>
        <PageTitle>Firewall</PageTitle>
      </PageHeader>
      <EmptyState
        icon={<ShieldIcon size={32} />}
        title="Firewall rules"
        description="Per-zombie outbound firewall rules will appear here once the firewall extension ships."
      />
    </div>
  );
}
