"use client";

import type { ComponentType } from "react";
import { useState } from "react";
import { useRouter } from "next/navigation";
import {
  Alert,
  Button,
  DashboardPanel,
  DashboardPanelHeader,
  DashboardPanelTitle,
  DashboardPanelDescription,
  Spinner,
  StatusPill,
  type StatusPillVariant,
} from "@agentsfleet/design-system";
import {
  BookOpenIcon,
  BriefcaseIcon,
  GitPullRequestIcon,
  Grid2x2Icon,
  HashIcon,
  PlugIcon,
  TicketIcon,
  UnplugIcon,
} from "lucide-react";
import { type ConnectorCatalogEntry } from "@/lib/api/connectors";
import { CONNECTOR_NOT_CONFIGURED_DOCS_URI, CONNECTOR_STATUS, type ConnectorStatus } from "@/lib/api/connectors-types";
import { disconnectConnectorAction, startConnectAction } from "../connector-actions";
import { presentErrorString, unexpectedErrorMessage } from "@/lib/errors";
import { EVENTS } from "@/lib/analytics/events";
import { captureProductEvent } from "@/lib/analytics/posthog";

const NOT_CONNECTED_LABEL = "Not connected";
const CONNECTED_LABEL = "Connected";
const RECONNECT_LABEL = "Reconnect needed";
const NOT_CONFIGURED_LABEL = "Admin setup required";
const CONNECTING_LABEL = "Connecting…";
const DISCONNECTING_LABEL = "Disconnecting…";
const CONNECT_LABEL = "Connect";
const DISCONNECT_LABEL = "Disconnect";
const REQUEST_CONNECTOR_LABEL = "Request connector";
// The description never says "connected": the status pill on the same row
// already does, and a row that said it twice read as a stutter. It says what
// the connection IS instead — which account it landed on, or that fleets may
// use it — which is the one thing the pill cannot.
const INSTALLED_ON_PREFIX = "Installed on ";
const CONNECTED_DESCRIPTION = "Ready for your fleets to use.";
const NOT_CONFIGURED_DESCRIPTION = "A platform admin needs to enable this connector.";
const REQUEST_CONNECTOR_DESCRIPTION = "Choose Request connector to register your interest and open the setup guide.";

// The card LIST comes from the catalog; this map only decorates a known provider
// id with an icon. An unknown id falls back to a generic plug, so a newly
// registered connector still renders — just without a bespoke glyph.
const PROVIDER_ICON: Record<string, ComponentType<{ size?: number }>> = {
  github: GitPullRequestIcon,
  slack: HashIcon,
  zoho: BriefcaseIcon,
  jira: TicketIcon,
  linear: Grid2x2Icon,
};

// Registry-sourced strings key this lookup, so restrict to OWN keys — a provider
// literally named after an `Object.prototype` member ("constructor", "toString")
// must fall back to the plug, not resolve to the inherited function (which would
// then be rendered as a component).
export function providerIcon(id: string): ComponentType<{ size?: number }> {
  const icon = Object.hasOwn(PROVIDER_ICON, id) ? PROVIDER_ICON[id] : undefined;
  return icon ?? PlugIcon;
}

// A bespoke per-provider status the page fetched (GitHub/Slack tri-state + the
// Slack team). Providers without one derive status from the catalog `connected`.
export interface ConnectorStatusOverride {
  status: ConnectorStatus;
  identity?: string | null;
}

function oauthStatusPill(status: ConnectorStatus): { label: string; variant: StatusPillVariant } {
  if (status === CONNECTOR_STATUS.connected) return { label: CONNECTED_LABEL, variant: "success" };
  if (status === CONNECTOR_STATUS.reconnectRequired) return { label: RECONNECT_LABEL, variant: "warning" };
  // Not-connected is a neutral fact, not a fault — the Connect button carries
  // the invitation; amber stays reserved for states that need attention.
  return { label: NOT_CONNECTED_LABEL, variant: "neutral" };
}

// oauth2 / app_install connectors: connect is a redirect. The action returns the
// provider authorize/install URL (carrying the signed state); the browser leaves
// and returns via the backend callback, which vaults the credential. No token is
// exchanged client-side. One row serves every connector — the display name, icon,
// and any status override are all that differ.
export function OAuthConnectorRow({
  entry,
  workspaceId,
  override,
}: {
  entry: ConnectorCatalogEntry;
  workspaceId: string;
  override?: ConnectorStatusOverride;
}) {
  const router = useRouter();
  const Icon = providerIcon(entry.id);
  const [connecting, setConnecting] = useState(false);
  const [disconnecting, setDisconnecting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const status =
    override?.status ?? (entry.connected ? CONNECTOR_STATUS.connected : CONNECTOR_STATUS.notConnected);
  const identity = override?.identity ?? null;
  const isConnected = status === CONNECTOR_STATUS.connected;
  const pill = entry.configured
    ? oauthStatusPill(status)
    : { label: NOT_CONFIGURED_LABEL, variant: "neutral" as const };
  async function connect() {
    setError(null);
    setConnecting(true);
    try {
      const result = await startConnectAction(entry.id, workspaceId);
      if (!result.ok) {
        setError(
          presentErrorString({
            errorCode: result.errorCode,
            message: result.error,
            action: `connect ${entry.display_name}`,
          }),
        );
        return;
      }
      window.location.href = result.data.install_url;
    } catch (cause) {
      setError(unexpectedErrorMessage(cause, `connect ${entry.display_name}`));
    } finally {
      setConnecting(false);
    }
  }

  async function disconnect() {
    setError(null);
    setDisconnecting(true);
    try {
      const result = await disconnectConnectorAction(entry.id, workspaceId);
      if (!result.ok) {
        setError(
          presentErrorString({
            errorCode: result.errorCode,
            message: result.error,
            action: `disconnect ${entry.display_name}`,
          }),
        );
        return;
      }
      router.refresh();
    } catch (cause) {
      setError(unexpectedErrorMessage(cause, `disconnect ${entry.display_name}`));
    } finally {
      setDisconnecting(false);
    }
  }

  const description = !entry.configured
    ? NOT_CONFIGURED_DESCRIPTION
    : isConnected
      ? identity
        ? `${INSTALLED_ON_PREFIX}${identity}`
        : CONNECTED_DESCRIPTION
      : "Choose Connect to authorize access with your provider.";

  return <DashboardPanel data-testid={`integration-${entry.id}`} className="flex flex-col gap-xl p-2xl">
    <DashboardPanelHeader>
      <div className="flex items-center gap-lg">
        <span className="grid size-10 shrink-0 place-items-center rounded-lg bg-secondary text-foreground" aria-hidden="true"><Icon size={22} /></span>
        <DashboardPanelTitle>{entry.display_name}</DashboardPanelTitle>
      </div>
      <StatusPill variant={pill.variant} dot={pill.variant !== "neutral"}>{pill.label}</StatusPill>
    </DashboardPanelHeader>
    {!entry.configured ? <Alert variant="info"><span>{description} {REQUEST_CONNECTOR_DESCRIPTION}</span></Alert>
      : <DashboardPanelDescription className="flex-1">{description}</DashboardPanelDescription>}
    {error ? <Alert variant="destructive">{error}</Alert> : null}
    <div className="flex items-center justify-end border-t border-border pt-lg">
      {!entry.configured ? <Button variant="ghost" size="sm" asChild>
        <a href={CONNECTOR_NOT_CONFIGURED_DOCS_URI} target="_blank" rel="noopener noreferrer"
          onClick={() => captureProductEvent(EVENTS.integration_requested, { integration_id: entry.id, integration_name: entry.display_name })}>
          <BookOpenIcon size={16} aria-hidden="true" />{REQUEST_CONNECTOR_LABEL}</a>
      </Button> : isConnected ? <Button variant="ghost" size="sm"
        aria-label={disconnecting ? DISCONNECTING_LABEL : `${DISCONNECT_LABEL} ${entry.display_name}`}
        disabled={disconnecting} aria-busy={disconnecting} onClick={() => void disconnect()}>
        {disconnecting ? <Spinner size="sm" /> : <UnplugIcon size={16} aria-hidden="true" />}{disconnecting ? DISCONNECTING_LABEL : DISCONNECT_LABEL}
      </Button> : <Button variant="secondary" size="sm"
        aria-label={connecting ? CONNECTING_LABEL : `${CONNECT_LABEL} ${entry.display_name}`}
        disabled={connecting} aria-busy={connecting} onClick={() => void connect()}>
        {connecting ? <Spinner size="sm" /> : <PlugIcon size={16} aria-hidden="true" />}{connecting ? CONNECTING_LABEL : CONNECT_LABEL}
      </Button>}
    </div>
  </DashboardPanel>;
}
