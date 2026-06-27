"use client";

import type { ComponentType } from "react";
import { useState, useTransition } from "react";
import {
  Alert,
  Button,
  DashboardRow,
  DashboardRowGroup,
  StatusPill,
  type StatusPillVariant,
} from "@agentsfleet/design-system";
import { BriefcaseIcon, GitPullRequestIcon, HashIcon } from "lucide-react";
import {
  INTEGRATION_AUTH,
  INTEGRATION_CATALOG,
  type Integration,
} from "@/lib/integrations/catalog";
import { CONNECTOR_STATUS, type ConnectorStatus } from "@/lib/api/connectors";
import { startGithubConnectAction } from "../connector-actions";
import { EVENTS } from "@/lib/analytics/events";
import { captureProductEvent } from "@/lib/analytics/posthog";
import { presentErrorString } from "@/lib/errors";

const NOT_CONNECTED_LABEL = "Not connected";
const CONNECTED_LABEL = "Connected";
const RECONNECT_LABEL = "Reconnect needed";
const TOKEN_STORED_LABEL = "Token stored";
const PLANNED_LABEL = "Planned";
const REQUESTED_LABEL = "Requested";
const CONNECT_GITHUB_LABEL = "Connect GitHub";
const RECONNECT_GITHUB_LABEL = "Reconnect GitHub";
const CONNECTING_LABEL = "Connecting…";
const REQUEST_ACCESS_LABEL = "Request access";
// Planned-connector access requests route to the team inbox and fire the
// EVENTS.integration_requested PostHog event so demand can be filtered.
const REQUEST_EMAIL = "agentsfleet@agentmail.to";

const INTEGRATION_ICON = {
  github: GitPullRequestIcon,
  zoho: BriefcaseIcon,
  slack: HashIcon,
} as const satisfies Record<Integration["id"], ComponentType<{ size?: number }>>;

function requestMailto(integration: Integration): string {
  const subject = `Integration request: ${integration.name}`;
  const body = `I'd like the ${integration.name} integration for agentsfleet.`;
  return `mailto:${REQUEST_EMAIL}?subject=${encodeURIComponent(subject)}&body=${encodeURIComponent(body)}`;
}

// ── GitHub: App-connect (browser OAuth install, no token paste) ──────────────

function githubPill(status: ConnectorStatus): { label: string; variant: StatusPillVariant } {
  if (status === CONNECTOR_STATUS.connected) return { label: CONNECTED_LABEL, variant: "success" };
  if (status === CONNECTOR_STATUS.reconnectRequired) return { label: RECONNECT_LABEL, variant: "warning" };
  return { label: NOT_CONNECTED_LABEL, variant: "warning" };
}

function GithubConnectorRow({
  integration,
  workspaceId,
  status,
}: {
  integration: Integration;
  workspaceId: string;
  status: ConnectorStatus;
}) {
  const Icon = INTEGRATION_ICON[integration.id];
  const [connecting, startConnecting] = useTransition();
  const [error, setError] = useState<string | null>(null);
  const pill = githubPill(status);
  const isConnected = status === CONNECTOR_STATUS.connected;
  const ctaLabel =
    status === CONNECTOR_STATUS.reconnectRequired ? RECONNECT_GITHUB_LABEL : CONNECT_GITHUB_LABEL;

  // Connect is a redirect: the action returns the GitHub App install URL (with a
  // signed state binding this workspace); the browser leaves for GitHub and
  // returns via the backend callback. No token is exchanged client-side.
  function connect() {
    setError(null);
    startConnecting(async () => {
      const result = await startGithubConnectAction(workspaceId);
      if (!result.ok) {
        setError(
          presentErrorString({
            errorCode: result.errorCode,
            message: result.error,
            action: "connect GitHub",
          }),
        );
        return;
      }
      window.location.href = result.data.install_url;
    });
  }

  return (
    <DashboardRow
      data-testid={`integration-${integration.id}`}
      icon={<Icon size={16} />}
      title={integration.name}
      description={
        <>
          {integration.description}
          {error ? (
            <Alert variant="destructive" className="mt-2">
              {error}
            </Alert>
          ) : null}
        </>
      }
      action={
        <div className="flex items-center gap-2">
          <StatusPill variant={pill.variant} dot>
            {pill.label}
          </StatusPill>
          {isConnected ? null : (
            <Button
              type="button"
              variant="outline"
              size="sm"
              onClick={connect}
              disabled={connecting}
              aria-busy={connecting}
            >
              {connecting ? CONNECTING_LABEL : ctaLabel}
            </Button>
          )}
        </div>
      }
    />
  );
}

// ── Zoho / Slack: custom-secret bridge (Planned) ─────────────────────────────

function plannedPill({
  isReady,
  requested,
}: {
  isReady: boolean;
  requested: boolean;
}): { label: string; variant: StatusPillVariant } {
  if (isReady) return { label: TOKEN_STORED_LABEL, variant: "success" };
  return requested
    ? { label: REQUESTED_LABEL, variant: "warning" }
    : { label: PLANNED_LABEL, variant: "neutral" };
}

function PlannedConnectorRow({
  integration,
  requiredSecret,
  storedCredentialNames,
  requested,
  onRequest,
}: {
  integration: Integration;
  requiredSecret: string;
  storedCredentialNames: ReadonlySet<string>;
  requested: boolean;
  onRequest: (integration: Integration) => void;
}) {
  const Icon = INTEGRATION_ICON[integration.id];
  const isReady = storedCredentialNames.has(requiredSecret);
  const pill = plannedPill({ isReady, requested });
  return (
    <DashboardRow
      data-testid={`integration-${integration.id}`}
      icon={<Icon size={16} />}
      title={integration.name}
      description={
        <>
          Planned. Use <code className="font-mono">{requiredSecret}</code> for now.
        </>
      }
      action={
        <div className="flex items-center gap-2">
          <StatusPill variant={pill.variant} dot={isReady || requested}>
            {pill.label}
          </StatusPill>
          {requested ? (
            <Button type="button" variant="outline" size="sm" disabled>
              {REQUESTED_LABEL}
            </Button>
          ) : (
            <Button asChild variant="outline" size="sm">
              <a href={requestMailto(integration)} onClick={() => onRequest(integration)}>
                {REQUEST_ACCESS_LABEL}
              </a>
            </Button>
          )}
        </div>
      }
    />
  );
}

export default function IntegrationsConnectors({
  workspaceId,
  githubStatus,
  credentialNames = [],
}: {
  workspaceId: string;
  githubStatus: ConnectorStatus;
  credentialNames?: readonly string[];
}) {
  const storedCredentialNames = new Set(credentialNames);
  const [requestedIntegrations, setRequestedIntegrations] = useState<ReadonlySet<string>>(
    () => new Set(),
  );

  function requestAccess(integration: Integration) {
    captureProductEvent(
      EVENTS.integration_requested,
      { integration_id: integration.id, integration_name: integration.name },
      { setPersonProperties: { last_integration_requested: integration.id } },
    );
    setRequestedIntegrations((prev) => {
      const next = new Set(prev);
      next.add(integration.id);
      return next;
    });
  }

  return (
    <div className="space-y-md" data-testid="integrations-connectors">
      <p className="text-body-sm leading-body-sm text-muted-foreground">
        Connect GitHub in one click — no token to paste. Request Zoho or Slack if needed.
      </p>
      <DashboardRowGroup>
        {INTEGRATION_CATALOG.map((integration) =>
          integration.auth === INTEGRATION_AUTH.appConnect ? (
            <GithubConnectorRow
              key={integration.id}
              integration={integration}
              workspaceId={workspaceId}
              status={githubStatus}
            />
          ) : (
            <PlannedConnectorRow
              key={integration.id}
              integration={integration}
              requiredSecret={integration.requiredSecret}
              storedCredentialNames={storedCredentialNames}
              requested={requestedIntegrations.has(integration.id)}
              onRequest={requestAccess}
            />
          ),
        )}
      </DashboardRowGroup>
    </div>
  );
}
