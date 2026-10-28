import Link from "next/link";
import { Button, EmptyState, PageLayout } from "@agentsfleet/design-system";
import { connectorFailureCopy, connectorReturnPath } from "@/lib/connectors/completion";

export const dynamic = "force-dynamic";

// Outside the dashboard group so a failed callback can explain itself without
// waiting for billing, invites or connector status. No provider query is echoed.
export default async function ConnectorCompletionPage({ searchParams }: {
  searchParams: Promise<{ reason?: string; return_to?: string }>;
}) {
  const query = await searchParams;
  const destination = connectorReturnPath(query.return_to);
  return (
    <PageLayout>
      <EmptyState
        title="Connection could not finish"
        description={connectorFailureCopy(query.reason)}
        action={<Button asChild><Link href={destination ?? "/"}>{destination ? "Back to Connect apps" : "Back to dashboard"}</Link></Button>}
      />
    </PageLayout>
  );
}
