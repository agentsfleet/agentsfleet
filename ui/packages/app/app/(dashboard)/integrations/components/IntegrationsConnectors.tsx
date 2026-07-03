"use client";

import { DashboardRowGroup, EmptyState } from "@agentsfleet/design-system";
import { PlugIcon } from "lucide-react";
import {
  CONNECTOR_STATUS,
  type ConnectorCatalogEntry,
  type ConnectorStatus,
} from "@/lib/api/connectors";
import { OAuthConnectorRow, type ConnectorStatusOverride } from "./connector-rows";

export default function IntegrationsConnectors({
  workspaceId,
  catalog,
  githubStatus,
  slackStatus = CONNECTOR_STATUS.notConnected,
  slackTeam = null,
}: {
  workspaceId: string;
  catalog: readonly ConnectorCatalogEntry[];
  githubStatus: ConnectorStatus;
  slackStatus?: ConnectorStatus;
  slackTeam?: string | null;
}) {
  // GitHub and Slack are the two connectors the page fetches a bespoke status for
  // (tri-state + the Slack team); every other card derives its status from the
  // catalog. Keyed by provider id so the render loop stays provider-agnostic; read
  // with an own-key guard so a catalog id named after a prototype member can't
  // resolve to an inherited value.
  const statusOverrides: Record<string, ConnectorStatusOverride> = {
    github: { status: githubStatus },
    slack: { status: slackStatus, identity: slackTeam },
  };
  const overrideFor = (id: string): ConnectorStatusOverride | undefined =>
    Object.hasOwn(statusOverrides, id) ? statusOverrides[id] : undefined;

  return (
    <div className="space-y-md" data-testid="integrations-connectors">
      <p className="text-body-sm leading-body-sm text-muted-foreground">
        Connect a tool in one click — no token to paste.
      </p>
      {catalog.length === 0 ? (
        <EmptyState
          data-testid="connectors-empty"
          icon={<PlugIcon size={32} />}
          title="No connectors available"
          description="Connectors couldn't be loaded right now — refresh to try again."
        />
      ) : (
        <DashboardRowGroup>
          {catalog.map((entry) => (
            <OAuthConnectorRow
              key={entry.id}
              entry={entry}
              workspaceId={workspaceId}
              override={overrideFor(entry.id)}
            />
          ))}
        </DashboardRowGroup>
      )}
    </div>
  );
}
