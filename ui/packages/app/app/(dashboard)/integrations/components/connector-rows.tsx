"use client";

import type { ComponentType } from "react";
import { useState } from "react";
import { useRouter } from "next/navigation";
import {
  Alert,
  Button,
  DashboardRow,
  Input,
  Label,
  Spinner,
  StatusPill,
  type StatusPillVariant,
} from "@agentsfleet/design-system";
import {
  ActivityIcon,
  BriefcaseIcon,
  GitPullRequestIcon,
  Grid2x2Icon,
  HashIcon,
  LineChartIcon,
  PlaneIcon,
  PlugIcon,
  TicketIcon,
} from "lucide-react";
import {
  CONNECTOR_NOT_CONFIGURED_DOCS_URI,
  CONNECTOR_STATUS,
  type ConnectorCatalogEntry,
  type ConnectorStatus,
} from "@/lib/api/connectors";
import { startConnectAction, submitApiKeyConnectAction } from "../connector-actions";
import { presentErrorString } from "@/lib/errors";

const NOT_CONNECTED_LABEL = "Not connected";
const CONNECTED_LABEL = "Connected";
const RECONNECT_LABEL = "Reconnect needed";
const NOT_CONFIGURED_LABEL = "Setup required";
const CONNECTING_LABEL = "Connecting…";
const CANCEL_LABEL = "Cancel";
const CONNECT_LABEL = "Connect";
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
  datadog: ActivityIcon,
  grafana: LineChartIcon,
  fly: PlaneIcon,
};

// Registry-sourced strings key this lookup, so restrict to OWN keys — a provider
// literally named after an `Object.prototype` member ("constructor", "toString")
// must fall back to the plug, not resolve to the inherited function (which would
// then be rendered as a component).
export function providerIcon(id: string): ComponentType<{ size?: number }> {
  const icon = Object.hasOwn(PROVIDER_ICON, id) ? PROVIDER_ICON[id] : undefined;
  return icon ?? PlugIcon;
}

// Presentation-only: turn a wire field name into an operator label. Acronyms the
// title-caser would mangle are fixed up; everything else is a generic humanize, so
// no provider's fields are enumerated here. Own-key guard for the same reason as
// `providerIcon` — a field segment named "constructor" must not hit the prototype.
const FIELD_ACRONYMS: Record<string, string> = { api: "API", url: "URL", id: "ID" };
function humanizeFieldName(name: string): string {
  return name
    .split("_")
    .filter(Boolean)
    .map((word, i) => {
      const acronym = Object.hasOwn(FIELD_ACRONYMS, word) ? FIELD_ACRONYMS[word] : undefined;
      if (acronym) return acronym;
      return i === 0 ? word.charAt(0).toUpperCase() + word.slice(1) : word;
    })
    .join(" ");
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
  return { label: NOT_CONNECTED_LABEL, variant: "warning" };
}

// oauth2 / app_install connectors: connect is a redirect. The action returns the
// provider authorize/install URL (carrying the signed state); the browser leaves
// and returns via the backend callback, which vaults the credential. No token is
// exchanged client-side. One row serves every OAuth provider — the display name,
// icon, and any status override are all that differ.
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

// api_key connectors: connect is an inline form posting the archetype's declared
// fields (from the catalog entry — never hard-coded) to a backend probe that
// validates the key before vaulting. Secret fields are masked and live only in the
// POST body; the form clears on success and re-fetches so the card flips connected.
export function ApiKeyConnectorRow({
  entry,
  workspaceId,
}: {
  entry: ConnectorCatalogEntry;
  workspaceId: string;
}) {
  const Icon = providerIcon(entry.id);
  const router = useRouter();
  const fields = entry.fields;
  const [open, setOpen] = useState(false);
  const [values, setValues] = useState<Record<string, string>>({});
  const [error, setError] = useState<string | null>(null);
  const [pending, setPending] = useState(false);

  // Own-key read — the value map is keyed by registry field names; a field named
  // after a prototype member must read as empty, not the inherited function.
  const fieldValue = (name: string): string => {
    const v = Object.hasOwn(values, name) ? values[name] : undefined;
    return v ?? "";
  };

  const connected = entry.connected;
  const canSubmit = fields.length > 0 && fields.every((f) => fieldValue(f.name).trim() !== "");

  function close() {
    setOpen(false);
    setError(null);
    setValues({});
  }

  async function submit() {
    if (!canSubmit || pending) return;
    setError(null);
    setPending(true);
    try {
      const body = Object.fromEntries(fields.map((f) => [f.name, fieldValue(f.name).trim()]));
      const result = await submitApiKeyConnectAction(entry.id, workspaceId, body);
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
      close();
      // `connected` flips server-side; re-fetch the catalog so the card reflects it.
      router.refresh();
    } finally {
      setPending(false);
    }
  }

  return (
    <DashboardRow
      data-testid={`integration-${entry.id}`}
      icon={<Icon size={16} />}
      title={entry.display_name}
      description={connected ? "Connected." : "Connect with an API key."}
      meta={
        open && !connected ? (
          <div className="space-y-3" data-testid={`api-key-form-${entry.id}`}>
            {fields.map((field) => {
              const inputId = `${entry.id}-${field.name}`;
              return (
                <div key={field.name} className="space-y-2">
                  <Label htmlFor={inputId}>{humanizeFieldName(field.name)}</Label>
                  <Input
                    id={inputId}
                    name={field.name}
                    type={field.secret ? "password" : "text"}
                    value={fieldValue(field.name)}
                    onChange={(e) => setValues((prev) => ({ ...prev, [field.name]: e.target.value }))}
                    spellCheck={false}
                    autoComplete="off"
                  />
                </div>
              );
            })}
            {error ? (
              <Alert variant="destructive" className="text-xs">
                {error}
              </Alert>
            ) : null}
            <Button
              type="button"
              size="sm"
              onClick={() => void submit()}
              disabled={pending || !canSubmit}
              aria-busy={pending}
            >
              {pending ? <Spinner size="sm" srLabel={CONNECTING_LABEL} /> : null}
              {CONNECT_LABEL}
            </Button>
          </div>
        ) : null
      }
      action={
        <div className="flex items-center gap-2">
          <StatusPill variant={connected ? "success" : "neutral"} dot={connected}>
            {connected ? CONNECTED_LABEL : NOT_CONNECTED_LABEL}
          </StatusPill>
          {connected ? null : (
            <Button
              type="button"
              variant="outline"
              size="sm"
              onClick={() => (open ? close() : setOpen(true))}
              aria-expanded={open}
            >
              {open ? CANCEL_LABEL : CONNECT_LABEL}
            </Button>
          )}
        </div>
      }
    />
  );
}
