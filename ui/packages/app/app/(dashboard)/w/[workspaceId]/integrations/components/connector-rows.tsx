"use client";

import type { ComponentType } from "react";
import type { ConnectorCatalogEntry } from "@/lib/api/connectors";
import { useEffect, useState, useTransition } from "react";
import { useRouter } from "next/navigation";
import { Alert, Button, Spinner } from "@agentsfleet/design-system";
import { BriefcaseIcon, GitBranchIcon, SquaresFourIcon, HashIcon, PlugsConnectedIcon, TicketIcon } from "@phosphor-icons/react/ssr";
import { disconnectConnectorAction, startConnectAction } from "../connector-actions";
import { presentErrorString, unexpectedErrorMessage } from "@/lib/errors";
import { EVENTS } from "@/lib/analytics/events";
import { captureProductEvent } from "@/lib/analytics/posthog";

type ConnectorRowProps = {
  entry: ConnectorCatalogEntry;
  workspaceId: string;
  onRequest: () => void;
};

const NOT_CONNECTED_LABEL = "Not connected";
const CONNECTED_LABEL = "Connected";
const CONNECTING_LABEL = "Connecting…";
const DISCONNECTING_LABEL = "Disconnecting…";
const PAGE_SHOW_EVENT = "pageshow";
const CONNECT_LABEL = "Connect";
const DISCONNECT_LABEL = "Disconnect";
const REQUEST_CONNECTOR_LABEL = "Request access";
const CONNECTION_HINTS: Record<string, string> = {
  slack: "Invite the app to the Slack channels your fleets use.",
  jira: "Add a webhook to send Jira events to a fleet.",
  linear: "Add a webhook to send Linear events to a fleet.",
  zoho: "Fleets can read tickets after you approve access.",
};

// The card LIST comes from the catalog; this map only decorates a known provider
// id with an icon. An unknown id falls back to a generic plug, so a newly
// registered connector still renders — just without a bespoke glyph.
const PROVIDER_ICON: Record<string, ComponentType<{ size?: number }>> = {
  github: GitBranchIcon,
  slack: HashIcon,
  zoho: BriefcaseIcon,
  jira: TicketIcon,
  linear: SquaresFourIcon,
};

// Registry-sourced strings key this lookup, so restrict to OWN keys — a provider
// literally named after an `Object.prototype` member ("constructor", "toString")
// must fall back to the plug, not resolve to the inherited function (which would
// then be rendered as a component).
export function providerIcon(id: string): ComponentType<{ size?: number }> {
  const icon = Object.hasOwn(PROVIDER_ICON, id) ? PROVIDER_ICON[id] : undefined;
  return icon ?? PlugsConnectedIcon;
}

export function OAuthConnectorRow({entry, workspaceId, onRequest}: ConnectorRowProps) {
  const Icon = providerIcon(entry.id);
  const [error, setError] = useState<string | null>(null);
  const {connecting, connect} = useConnect(entry, workspaceId, setError);
  const {disconnecting, disconnect} = useDisconnect(entry, workspaceId, setError);
  const isConnected = entry.connected;
  const identity = entry.label;
  const pending = connecting || disconnecting;
  const actionLabel = pending
    ? disconnecting ? DISCONNECTING_LABEL : CONNECTING_LABEL
    : isConnected ? DISCONNECT_LABEL : CONNECT_LABEL;
  return <div data-testid={`integration-${entry.id}`} className="p-xl sm:p-5">
    <div className="flex min-w-0 flex-wrap items-center gap-xl">
      <span className="hidden size-10 shrink-0 items-center justify-center rounded-control border border-border sm:flex" aria-hidden="true"><Icon size={16} /></span>
      <div className="min-w-0 basis-full sm:flex-1 sm:basis-auto">
        <h2 className="text-label font-medium">{entry.display_name}</h2>
        <p className="mt-sm text-label text-muted-foreground" aria-live="polite">{isConnected ? `${CONNECTED_LABEL}${identity ? ` · ${identity}` : ""}` : entry.configured ? NOT_CONNECTED_LABEL : "Requires platform setup"}</p>
        {isConnected && Object.hasOwn(CONNECTION_HINTS, entry.id) ? (
          <p className="mt-sm text-conversation leading-conversation text-muted-foreground">{CONNECTION_HINTS[entry.id]}</p>
        ) : null}
      </div>
      {/* Disconnect keeps focus on this button while the catalog refreshes. */}
      {isConnected || entry.configured ? <Button
        variant="outline"
        disabled={connecting}
        aria-disabled={disconnecting || undefined}
        aria-busy={pending || undefined}
        aria-label={pending ? actionLabel : `${actionLabel} ${entry.display_name}`}
        className="aria-disabled:cursor-wait aria-disabled:opacity-50"
        onClick={disconnecting ? undefined : isConnected ? disconnect : () => void connect()}
      >
        {disconnecting ? <Spinner size="sm" aria-hidden="true" /> : null}{actionLabel}
      </Button> : <Button variant="outline" aria-label={`Request access to ${entry.display_name}`} onClick={() => {
        captureProductEvent(EVENTS.integration_requested, {integration_id: entry.id, integration_name: entry.display_name}); onRequest();
      }}>{REQUEST_CONNECTOR_LABEL}</Button>}
    </div>
    {error ? <Alert className="mt-lg" variant="destructive">{error}</Alert> : null}
  </div>;
}

function useConnect(entry: ConnectorCatalogEntry, workspaceId: string, setError: (error: string | null) => void) {
  const [connecting, setConnecting] = useState(false);
  useEffect(() => {
    const restore = (event: PageTransitionEvent) => {
      if (event.persisted) setConnecting(false);
    };
    window.addEventListener(PAGE_SHOW_EVENT, restore);
    return () => window.removeEventListener(PAGE_SHOW_EVENT, restore);
  }, []);

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
      // The provider returns to the backend callback, which saves the connection.
      window.location.href = result.data.install_url;
    } catch (cause) {
      setError(unexpectedErrorMessage(cause, `connect ${entry.display_name}`));
      setConnecting(false);
    }
  }
  return {connecting, connect};
}

function useDisconnect(entry: ConnectorCatalogEntry, workspaceId: string, setError: (error: string | null) => void) {
  const router = useRouter();
  const [disconnecting, startDisconnect] = useTransition();

  function disconnect() {
    setError(null);
    startDisconnect(async () => {
      try {
        const result = await disconnectConnectorAction(entry.id, workspaceId);
        if (!result.ok) {
          setError(presentErrorString({
            errorCode: result.errorCode,
            message: result.error,
            action: `disconnect ${entry.display_name}`,
          }));
          return;
        }
        startDisconnect(() => router.refresh());
      } catch (cause) {
        setError(unexpectedErrorMessage(cause, `disconnect ${entry.display_name}`));
      }
    });
  }
  return {disconnecting, disconnect};
}
