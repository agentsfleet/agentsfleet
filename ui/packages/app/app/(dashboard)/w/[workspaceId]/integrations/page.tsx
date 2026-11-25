import { ConnectorStatus } from "./components/connector-status";

export const dynamic = "force-dynamic";

export default async function IntegrationsPage({ params }: {
  params: Promise<{ workspaceId: string }>;
}) {
  const { workspaceId } = await params;
  return <ConnectorStatus workspaceId={workspaceId} />;
}
