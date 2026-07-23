"use client";

import { Button, EmptyState, PageHeader, PageLayout, PageTitle } from "@agentsfleet/design-system";
import { AlertTriangleIcon } from "lucide-react";

// Dashboard error boundary. A transient failure loading a dashboard surface
// (e.g. the workspace list on the entry redirect) renders an honest retry state
// rather than a misleading empty/create-first screen or a blank page. `reset`
// re-renders the segment, re-running the failed server work.
export default function DashboardError({ reset }: { error: Error; reset: () => void }) {
  return (
    <PageLayout>
      <PageHeader>
        <PageTitle>Something went wrong</PageTitle>
      </PageHeader>
      <EmptyState
        icon={<AlertTriangleIcon size={32} />}
        title="Couldn't load this page"
        description="A transient error occurred. Try again in a moment."
        action={
          <Button type="button" onClick={() => reset()} data-testid="dashboard-error-retry">
            Retry
          </Button>
        }
      />
    </PageLayout>
  );
}
