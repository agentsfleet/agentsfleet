"use client";

import type { ComponentType } from "react";
import { useEffect, useRef, useState } from "react";
import { useRouter } from "next/navigation";
import { Alert, Button, Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle, Spinner } from "@agentsfleet/design-system";
import { BriefcaseIcon, GitBranchIcon, SquaresFourIcon, HashIcon, PlugsConnectedIcon, TicketIcon } from "@phosphor-icons/react/ssr";
import { type ConnectorCatalogEntry } from "@/lib/api/connectors";
import { disconnectConnectorAction, startConnectAction } from "../connector-actions";
import { presentErrorString, unexpectedErrorMessage } from "@/lib/errors";
import { EVENTS } from "@/lib/analytics/events";
import { captureProductEvent } from "@/lib/analytics/posthog";

const NOT_CONNECTED_LABEL = "Not connected";
const CONNECTED_LABEL = "Connected";
const CONNECTING_LABEL = "Connecting…";
const DISCONNECTING_LABEL = "Disconnecting…";
const PAGE_SHOW_EVENT = "pageshow";
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
  const [managing, setManaging] = useState(false);
  const done = useRef<HTMLButtonElement>(null);
  const [connecting, setConnecting] = useState(false);
  const [disconnecting, setDisconnecting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    const restore = (event: PageTransitionEvent) => {
      if (event.persisted) setConnecting(false);
    };
    window.addEventListener(PAGE_SHOW_EVENT, restore);
    return () => window.removeEventListener(PAGE_SHOW_EVENT, restore);
  }, []);

  const isConnected = entry.connected;
  const identity = entry.label;
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
      setManaging(false);
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
        ? `${INSTALLED_ON_PREFIX}${identity}.`
        : CONNECTED_DESCRIPTION
      : "Choose Connect to authorize access with your provider.";
  const pending = connecting || disconnecting;
  return <div data-testid={`integration-${entry.id}`} className="p-xl sm:p-5">
    <div className="flex min-w-0 flex-wrap items-center gap-xl">
      <span className="hidden size-10 shrink-0 items-center justify-center rounded-control border border-border sm:flex" aria-hidden="true"><Icon size={16} /></span>
      <div className="min-w-0 basis-full sm:flex-1 sm:basis-auto"><h2 className="text-label font-medium">{entry.display_name}</h2>
        <p className="mt-sm text-label text-muted-foreground">{isConnected ? `${CONNECTED_LABEL}${identity ? ` · ${identity}` : ""}` : entry.configured ? NOT_CONNECTED_LABEL : "Requires platform setup"}</p></div>
      {entry.configured ? <Button variant="outline" disabled={pending} aria-busy={pending || undefined} aria-label={connecting ? CONNECTING_LABEL : `${isConnected ? "Manage" : CONNECT_LABEL} ${entry.display_name}`} onClick={() => isConnected ? setManaging(true) : void connect()}>
        {connecting ? CONNECTING_LABEL : isConnected ? "Manage" : CONNECT_LABEL}
      </Button> : <Button variant="outline" aria-label={`Request access to ${entry.display_name}`} onClick={() => {
        captureProductEvent(EVENTS.integration_requested, {integration_id: entry.id, integration_name: entry.display_name}); onRequest();
      }}>{REQUEST_CONNECTOR_LABEL}</Button>}
    </div>
    {error && !managing ? <Alert className="mt-lg" variant="destructive">{error}</Alert> : null}
    <Dialog open={managing} onOpenChange={open => {if (!pending) setManaging(open);}}>
      <DialogContent closeDisabled={pending} onOpenAutoFocus={event => {event.preventDefault(); done.current?.focus({preventScroll: true});}}>
        <DialogHeader className="pb-0"><DialogTitle>{entry.display_name}</DialogTitle><DialogDescription>{description}</DialogDescription></DialogHeader>
        {Object.hasOwn(PROVIDER_CAPABILITIES, entry.id) ? <p className="mt-xl text-label text-muted-foreground">{PROVIDER_CAPABILITIES[entry.id]}</p> : null}
        {error ? <Alert className="mt-xl" variant="destructive">{error}</Alert> : null}
        <DialogFooter><Button variant="outline" disabled={pending} aria-busy={disconnecting || undefined} aria-label={disconnecting ? DISCONNECTING_LABEL : `${DISCONNECT_LABEL} ${entry.display_name}`} onClick={() => void disconnect()}>
            {disconnecting ? <Spinner size="sm" /> : null}{disconnecting ? DISCONNECTING_LABEL : DISCONNECT_LABEL}
          </Button><Button ref={done} disabled={pending} onClick={() => setManaging(false)}>Done</Button></DialogFooter>
      </DialogContent>
    </Dialog>
  </div>;
}
