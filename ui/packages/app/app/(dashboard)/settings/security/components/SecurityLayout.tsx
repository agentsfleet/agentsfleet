import type { ReactNode } from "react";
import { PageHeader, PageLayout, PageTitle } from "@agentsfleet/design-system";

export function SecurityLayout({children}: {children: ReactNode}) {
  return <PageLayout><PageHeader description="Manage API keys and the devices signed in to your account."><PageTitle>Security</PageTitle></PageHeader>{children}</PageLayout>;
}
