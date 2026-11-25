import type { ReactNode } from "react";
import { PageHeader, PageLayout, PageTitle } from "@agentsfleet/design-system";
import { CONNECT_APPS_COPY } from "./copy";

export default function ConnectedAppsLayout({ children }: { children: ReactNode }) {
  return (
    <PageLayout className="gap-2xl">
      <PageHeader description={CONNECT_APPS_COPY.description}>
        <PageTitle>{CONNECT_APPS_COPY.title}</PageTitle>
      </PageHeader>
      {children}
    </PageLayout>
  );
}
