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
import {
  BriefcaseIcon,
  GaugeIcon,
  GitPullRequestIcon,
  HashIcon,
  SquareKanbanIcon,
  TicketIcon,
} from "lucide-react";
import {
  INTEGRATION_AUTH,
  INTEGRATION_CATALOG,
  type Integration,
} from "@/lib/integrations/catalog";
import {
  CONNECTOR_PROVIDERS,
  CONNECTOR_STATUS,
  type ConnectorStatus,
} from "@/lib/api/connectors";
import type { ActionResult } from "@/lib/actions/with-token";
import { startConnectAction } from "../connector-actions";
import { EVENTS } from "@/lib/analytics/events";
import { captureProductEvent } from "@/lib/analytics/posthog";
import { presentErrorString } from "@/lib/errors";

const NOT_CONNECTED_LABEL = "Not connected";
const CONNECTED_LABEL = "Connected";
const RECONNECT_LABEL = "Reconnect needed";
const TOKEN_STORED_LABEL = "Token stored";
const REQUESTED_LABEL = "Requested";
const CONNECT_GITHUB_LABEL = "Connect GitHub";
const RECONNECT_GITHUB_LABEL = "Reconnect GitHub";
const CONNECT_SLACK_LABEL = "Connect Slack";
const RECONNECT_SLACK_LABEL = "Reconnect Slack";
const SLACK_CONNECTED_PREFIX = "Slack connected: ";
const CONNECTING_LABEL = "Connecting…";
const REQUEST_ACCESS_LABEL = "Request access";

const INTEGRATION_ICON = {
  github: GitPullRequestIcon,
  zoho: BriefcaseIcon,
  slack: HashIcon,
  jira: TicketIcon,
  linear: SquareKanbanIcon,
  grafana: GaugeIcon,
} as const satisfies Record<Integration["id"], ComponentType<{ size?: number }>>;

// ── OAuth connectors (browser connect, no token paste): GitHub + Slack ───────

// Status → pill is auth-agnostic (both GitHub and Slack surface the same three
// states), so one mapper serves both rows.
function oauthStatusPill(status: ConnectorStatus): { label: string; variant: StatusPillVariant } {
  if (status === CONNECTOR_STATUS.connected) return { label: CONNECTED_LABEL, variant: "success" };
  if (status === CONNECTOR_STATUS.reconnectRequired) return { label: RECONNECT_LABEL, variant: "warning" };
  return { label: NOT_CONNECTED_LABEL, variant: "warning" };
}

// One row for both browser-OAuth connectors (GitHub, Slack): identical connect
// redirect + status pill; only the labels, the action, and the connected-state
// description (Slack shows the team) differ, passed in by the caller.
function OAuthConnectorRow({
  integration,
  workspaceId,
  status,
  connectLabel,
  reconnectLabel,
  actionVerb,
  onConnect,
  connectedDescription = null,
}: {
  integration: Integration;
  workspaceId: string;
  status: ConnectorStatus;
  connectLabel: string;
  reconnectLabel: string;
  actionVerb: string;
  onConnect: (workspaceId: string) => Promise<ActionResult<{ install_url: string }>>;
  connectedDescription?: string | null;
}) {
  const Icon = INTEGRATION_ICON[integration.id];
  const [connecting, startConnecting] = useTransition();
  const [error, setError] = useState<string | null>(null);
  const pill = oauthStatusPill(status);
  const isConnected = status === CONNECTOR_STATUS.connected;
  const ctaLabel = status === CONNECTOR_STATUS.reconnectRequired ? reconnectLabel : connectLabel;

  // Connect is a redirect: the action returns the provider authorize/install URL
  // (with a signed state binding this workspace); the browser leaves for the
  // provider and returns via the backend callback, which vaults the credential.
  // No token is exchanged client-side.
  function connect() {
    setError(null);
    startConnecting(async () => {
      const result = await onConnect(workspaceId);
      if (!result.ok) {
        setError(
          presentErrorString({
            errorCode: result.errorCode,
            message: result.error,
            action: actionVerb,
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
          {isConnected && connectedDescription ? connectedDescription : integration.description}
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

// ── Coming-soon connectors: Zoho (custom-secret bridge) + Jira/Linear/Grafana ─

function comingSoonPill({
  isReady,
  requested,
}: {
  isReady: boolean;
  requested: boolean;
}): { label: string; variant: StatusPillVariant } {
  if (isReady) return { label: TOKEN_STORED_LABEL, variant: "success" };
  return requested
    ? { label: REQUESTED_LABEL, variant: "warning" }
    : { label: NOT_CONNECTED_LABEL, variant: "neutral" };
}

// One row for every connector that isn't wired for one-click OAuth yet. Zoho
// carries a vault-secret bridge (`requiredSecret`), so it can reach "Token
// stored"; the rest (Jira/Linear/Grafana) are pure coming-soon rows whose only
// affordance is Request access, which fires a PostHog demand signal.
function ComingSoonConnectorRow({
  integration,
  storedCredentialNames,
  requested,
  onRequest,
}: {
  integration: Integration;
  storedCredentialNames: ReadonlySet<string>;
  requested: boolean;
  onRequest: (integration: Integration) => void;
}) {
  const Icon = INTEGRATION_ICON[integration.id];
  // Only the vault-secret connector (Zoho) carries a bridge secret; the pure
  // coming-soon rows (Jira/Linear/Grafana) have none. Narrow on the `auth`
  // discriminant so TypeScript derives `requiredSecret` from the union member
  // itself, rather than probing for the property name with a magic string.
  const requiredSecret =
    integration.auth === INTEGRATION_AUTH.vaultSecret ? integration.requiredSecret : undefined;
  const isReady = requiredSecret != null && storedCredentialNames.has(requiredSecret);
  const pill = comingSoonPill({ isReady, requested });
  return (
    <DashboardRow
      data-testid={`integration-${integration.id}`}
      icon={<Icon size={16} />}
      title={integration.name}
      description={
        requiredSecret ? (
          <>
            {integration.description} Use <code className="font-mono">{requiredSecret}</code> for now.
          </>
        ) : (
          <>{integration.description} Coming soon.</>
        )
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
            <Button
              type="button"
              variant="outline"
              size="sm"
              onClick={() => onRequest(integration)}
            >
              {REQUEST_ACCESS_LABEL}
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
  slackStatus = CONNECTOR_STATUS.notConnected,
  slackTeam = null,
  credentialNames = [],
}: {
  workspaceId: string;
  githubStatus: ConnectorStatus;
  slackStatus?: ConnectorStatus;
  slackTeam?: string | null;
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
        Connect GitHub or Slack in one click — no token to paste. More connectors are on the way —
        request access to move them up the list.
      </p>
      <DashboardRowGroup>
        {INTEGRATION_CATALOG.map((integration) =>
          integration.auth === INTEGRATION_AUTH.appConnect ? (
            <OAuthConnectorRow
              key={integration.id}
              integration={integration}
              workspaceId={workspaceId}
              status={githubStatus}
              connectLabel={CONNECT_GITHUB_LABEL}
              reconnectLabel={RECONNECT_GITHUB_LABEL}
              actionVerb="connect GitHub"
              onConnect={startConnectAction.bind(null, CONNECTOR_PROVIDERS.github)}
            />
          ) : integration.auth === INTEGRATION_AUTH.oauthConnect ? (
            <OAuthConnectorRow
              key={integration.id}
              integration={integration}
              workspaceId={workspaceId}
              status={slackStatus}
              connectLabel={CONNECT_SLACK_LABEL}
              reconnectLabel={RECONNECT_SLACK_LABEL}
              actionVerb="connect Slack"
              onConnect={startConnectAction.bind(null, CONNECTOR_PROVIDERS.slack)}
              connectedDescription={slackTeam ? `${SLACK_CONNECTED_PREFIX}${slackTeam}` : null}
            />
          ) : (
            <ComingSoonConnectorRow
              key={integration.id}
              integration={integration}
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
