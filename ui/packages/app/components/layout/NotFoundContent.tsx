import Link from "next/link";
import { Button, EmptyState, PageHeader, PageLayout, PageTitle } from "@agentsfleet/design-system";

export default function NotFoundContent() {
  return (
    <PageLayout>
      <PageHeader><PageTitle>Page not found</PageTitle></PageHeader>
      <EmptyState
        title="This page isn’t available"
        description="The link may have changed, or this item may no longer be available in your workspace."
        action={<Button asChild><Link href="/">Back to dashboard</Link></Button>}
      />
    </PageLayout>
  );
}
