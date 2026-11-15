import type { ReactNode } from "react";
import { PageHeader, PageLayout, PageTitle } from "@agentsfleet/design-system";

export function UsageLayout({children}: {children: ReactNode}) {
  return <PageLayout className="billing-page gap-2xl">
    <PageHeader description="See what your fleets used and what you were charged."><PageTitle>Usage</PageTitle></PageHeader>
    {children}
  </PageLayout>;
}
