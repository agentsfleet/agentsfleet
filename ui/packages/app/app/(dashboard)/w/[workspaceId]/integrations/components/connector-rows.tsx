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
  BriefcaseIcon,
  GitPullRequestIcon,
  Grid2x2Icon,
  HashIcon,
  PlugIcon,
  TicketIcon,
  UnplugIcon,
} from "lucide-react";
import { type ConnectorCatalogEntry } from "@/lib/api/connectors";
import { disconnectConnectorAction, startConnectAction } from "../connector-actions";
import { presentErrorString, unexpectedErrorMessage } from "@/lib/errors";
import { EVENTS } from "@/lib/analytics/events";
import { captureProductEvent } from "@/lib/analytics/posthog";

const NOT_CONNECTED_LABEL = "Not connected";
const CONNECTED_LABEL = "Connected";
const CONNECTING_LABEL = "Connecting…";
const DISCONNECTING_LABEL = "Disconnecting…";
const CONNECT_LABEL = "Connect";
const DISCONNECT_LABEL = "Disconnect";
const REQUEST_CONNECTOR_LABEL = "Request access";
// The description never says "connected": the status pill on the same row
// already does, and a row that said it twice read as a stutter. It says what
// the connection IS instead — which account it landed on, or that fleets may
// use it — which is the one thing the pill cannot.
const INSTALLED_ON_PREFIX = "Installed on ";
const CONNECTED_DESCRIPTION = "Authorization saved. Fleet access is managed in each fleet’s setup.";
const PROVIDER_CAPABILITIES: Record<string, string> = {
  github: "GitHub App access and supported repository events. Each fleet needs approved access.",
  slack: "Channel mentions and bot replies. Invite the app to the channels your fleets use.",
  jira: "Jira API access for approved fleet tools. Incoming events need a separately configured webhook.",
  linear: "Read Linear and create comments with approved fleet tools. Incoming events need a separately configured webhook.",
  zoho: "Zoho Desk organization and basic read access. Ticket updates are not included in this connection.",
};
const NOT_CONFIGURED_DESCRIPTION = "A platform admin needs to enable this connector. Choose Request access to register your interest.";

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

// oauth2 / app_install connectors: connect is a redirect. The action returns the
// provider authorize/install URL (carrying the signed state); the browser leaves
// and returns via the backend callback, which vaults the credential. No token is
// exchanged client-side. One row serves every connector — the display name, icon,
// and any status override are all that differ.
export function OAuthConnectorRow({
  entry,
  workspaceId,
  onRequest,
}: {
  entry: ConnectorCatalogEntry;
  workspaceId: string;
  onRequest: () => void;
}) {
  const router = useRouter();
  const Icon = providerIcon(entry.id);
  const [connecting, setConnecting] = useState(false);
  const [disconnecting, setDisconnecting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const isConnected = entry.connected;
  const identity = entry.label;
  const pill: { label: string; variant: StatusPillVariant } = isConnected
    ? { label: CONNECTED_LABEL, variant: "success" }
    : { label: NOT_CONNECTED_LABEL, variant: "neutral" };
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
        setConnecting(false);
        return;
      }
      window.location.href = result.data.install_url;
    } catch (cause) {
      setError(unexpectedErrorMessage(cause, `connect ${entry.display_name}`));
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
    <DashboardPanelHeader className="md:items-center">
      <div className="flex min-w-0 items-center gap-lg">
        <span className="grid size-10 shrink-0 place-items-center rounded-lg bg-secondary text-foreground" aria-hidden="true"><Icon size={22} /></span>
        <DashboardPanelTitle>{entry.display_name}</DashboardPanelTitle>
      </div>
      {entry.configured ? <StatusPill variant={pill.variant} dot={pill.variant !== "neutral"}>{pill.label}</StatusPill>
        : <Button variant="secondary" size="sm" className="shrink-0" aria-label={`${REQUEST_CONNECTOR_LABEL} to ${entry.display_name}`}
          onClick={() => {
            captureProductEvent(EVENTS.integration_requested, { integration_id: entry.id, integration_name: entry.display_name });
            onRequest();
          }}>{REQUEST_CONNECTOR_LABEL}</Button>}
    </DashboardPanelHeader>
    <DashboardPanelDescription className="flex-1">
      {description}
      {Object.hasOwn(PROVIDER_CAPABILITIES, entry.id) ? ` ${PROVIDER_CAPABILITIES[entry.id]}` : ""}
    </DashboardPanelDescription>
    {error ? <Alert variant="destructive">{error}</Alert> : null}
    {entry.configured ? <div className="flex items-center justify-end border-t border-border pt-lg">
      {isConnected ? <Button variant="ghost" size="sm"
        aria-label={disconnecting ? DISCONNECTING_LABEL : `${DISCONNECT_LABEL} ${entry.display_name}`}
        disabled={disconnecting} aria-busy={disconnecting} onClick={() => void disconnect()}>
        {disconnecting ? <Spinner size="sm" /> : <UnplugIcon size={16} aria-hidden="true" />}{disconnecting ? DISCONNECTING_LABEL : DISCONNECT_LABEL}
      </Button> : <Button variant="secondary" size="sm"
        aria-label={connecting ? CONNECTING_LABEL : `${CONNECT_LABEL} ${entry.display_name}`}
        disabled={connecting} aria-busy={connecting} onClick={() => void connect()}>
        {connecting ? <Spinner size="sm" /> : <PlugIcon size={16} aria-hidden="true" />}{connecting ? CONNECTING_LABEL : CONNECT_LABEL}
      </Button>}
    </div> : null}
  </DashboardPanel>;
}
