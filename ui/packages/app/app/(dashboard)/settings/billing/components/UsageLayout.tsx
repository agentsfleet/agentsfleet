import type { ReactNode } from "react";
import { Button, PageHeader, PageLayout, PageTitle } from "@agentsfleet/design-system";
import { SUPPORT_EMAIL } from "@/lib/contact";

export function UsageLayout({children}: {children: ReactNode}) {
  return <PageLayout className="billing-page gap-2xl">
    <PageHeader description="See what your fleets used and what you were charged."><PageTitle>Usage</PageTitle></PageHeader>
    {children}
    <Button asChild variant="link" className="self-start"><a href={`mailto:${SUPPORT_EMAIL}`}>Add credits</a></Button>
  </PageLayout>;
}
