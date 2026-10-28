import { Suspense } from "react";
import { PageHeader, PageLayout, PageTitle } from "@agentsfleet/design-system";
import { RouteLoading } from "@/components/layout/RouteLoading";
import { ConnectorStatus } from "./components/connector-status";
import { CONNECT_APPS_COPY } from "./copy";

export const dynamic = "force-dynamic";

export default async function IntegrationsPage({ params }: {
  params: Promise<{ workspaceId: string }>;
}) {
  const { workspaceId } = await params;
  return <PageLayout>
    <PageHeader description={CONNECT_APPS_COPY.description}>
      <PageTitle>{CONNECT_APPS_COPY.title}</PageTitle>
    </PageHeader>
    <Suspense fallback={<RouteLoading />}>
      <ConnectorStatus workspaceId={workspaceId} />
    </Suspense>
  </PageLayout>;
}
