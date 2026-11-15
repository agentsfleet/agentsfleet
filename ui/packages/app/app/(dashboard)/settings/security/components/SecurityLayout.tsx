import type { ReactNode } from "react";
import { PageHeader, PageLayout, PageTitle } from "@agentsfleet/design-system";

export function SecurityLayout({children}: {children: ReactNode}) {
  return <PageLayout><PageHeader description="Manage your API keys and signed-in devices."><PageTitle>API Keys</PageTitle></PageHeader>{children}</PageLayout>;
}
