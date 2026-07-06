"use client";

import type { ComponentType } from "react";
import { useState } from "react";
import {
  Alert,
  Button,
  DashboardRow,
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
} from "lucide-react";
import {
  CONNECTOR_NOT_CONFIGURED_DOCS_URI,
  CONNECTOR_STATUS,
  type ConnectorCatalogEntry,
  type ConnectorStatus,
} from "@/lib/api/connectors";
import { startConnectAction } from "../connector-actions";
import { presentErrorString } from "@/lib/errors";

const NOT_CONNECTED_LABEL = "Not connected";
const CONNECTED_LABEL = "Connected";
const RECONNECT_LABEL = "Reconnect needed";
const NOT_CONFIGURED_LABEL = "Setup required";
const CONNECTING_LABEL = "Connecting…";
const SETUP_GUIDE_LABEL = "Setup guide";
const CONNECTED_IDENTITY_PREFIX = "Connected: ";

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
  const Icon = providerIcon(entry.id);
  const [connecting, setConnecting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const status =
    override?.status ?? (entry.connected ? CONNECTOR_STATUS.connected : CONNECTOR_STATUS.notConnected);
  const identity = override?.identity ?? null;
  const isConnected = status === CONNECTOR_STATUS.connected;
  const pill = entry.configured
    ? oauthStatusPill(status)
    : { label: NOT_CONFIGURED_LABEL, variant: "neutral" as const };
  const ctaLabel =
    status === CONNECTOR_STATUS.reconnectRequired
      ? `Reconnect ${entry.display_name}`
      : `Connect ${entry.display_name}`;

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
    } finally {
      setConnecting(false);
    }
  }

  const description = !entry.configured
    ? "Not configured on this deployment."
    : isConnected
      ? identity
        ? `${CONNECTED_IDENTITY_PREFIX}${identity}`
        : "Connected."
      : "Connect in one click — no token to paste.";

  return (
    <DashboardRow
      data-testid={`integration-${entry.id}`}
      icon={<Icon size={16} />}
      title={entry.display_name}
      description={
        <>
          {description}
          {error ? (
            <Alert variant="destructive" className="mt-2">
              {error}
            </Alert>
          ) : null}
        </>
      }
      action={
        <div className="flex items-center gap-2">
          <StatusPill variant={pill.variant} dot={pill.variant !== "neutral"}>
            {pill.label}
          </StatusPill>
          {!entry.configured ? (
            <a
              href={CONNECTOR_NOT_CONFIGURED_DOCS_URI}
              target="_blank"
              rel="noreferrer"
              className="rounded-sm text-body-sm text-primary underline underline-offset-2 hover:no-underline focus:outline-none focus-visible:ring-2 focus-visible:ring-ring"
            >
              {SETUP_GUIDE_LABEL}
            </a>
          ) : isConnected ? null : (
            <Button
              type="button"
              variant="outline"
              size="sm"
              onClick={() => void connect()}
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
