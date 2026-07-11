export const CONNECTOR_STATE = {
  connected: "connected",
  notConnected: "not_connected",
  reconnectRequired: "reconnect_required",
  unconfigured: "unconfigured",
} as const;

export type ConnectorState = typeof CONNECTOR_STATE[keyof typeof CONNECTOR_STATE];

export interface ConnectorCatalogEntry {
  readonly id?: string;
  readonly archetype?: string;
  readonly display_name?: string;
  readonly configured?: boolean;
  readonly connected?: boolean;
}

export interface ConnectorSummary {
  readonly provider: string;
  readonly display_name: string;
  readonly archetype: string;
  readonly state: ConnectorState;
  readonly hint?: string;
}

const nextAction = (provider: string, state: ConnectorState): string | undefined => {
  switch (state) {
    case CONNECTOR_STATE.unconfigured:
      return `Ask the platform administrator to configure the ${provider} App keys.`;
    case CONNECTOR_STATE.notConnected:
      return `Connect ${provider} from the workspace connector settings.`;
    case CONNECTOR_STATE.reconnectRequired:
      return `Reconnect ${provider} from the workspace connector settings.`;
    case CONNECTOR_STATE.connected:
      return undefined;
  }
};

export const summarizeConnector = (
  entry: ConnectorCatalogEntry,
): ConnectorSummary => {
  const provider = entry.id ?? "";
  const state: ConnectorState = !entry.configured
    ? CONNECTOR_STATE.unconfigured
    : entry.connected
      ? CONNECTOR_STATE.connected
      : CONNECTOR_STATE.notConnected;
  const hint = nextAction(provider, state);
  return {
    provider,
    display_name: entry.display_name ?? "",
    archetype: entry.archetype ?? "",
    state,
    ...(hint ? { hint } : {}),
  };
};

export const summarizeStatus = (
  entry: ConnectorCatalogEntry,
  status: Readonly<Record<string, unknown>> | null,
): ConnectorSummary & { readonly details: Readonly<Record<string, unknown>> } => {
  const base = summarizeConnector(entry);
  if (base.state === CONNECTOR_STATE.unconfigured) return { ...base, details: {} };
  const rawState = status?.status;
  const state: ConnectorState = rawState === CONNECTOR_STATE.connected
    ? CONNECTOR_STATE.connected
    : rawState === CONNECTOR_STATE.reconnectRequired
      ? CONNECTOR_STATE.reconnectRequired
      : CONNECTOR_STATE.notConnected;
  const hint = nextAction(base.provider, state);
  return {
    provider: base.provider,
    display_name: base.display_name,
    archetype: base.archetype,
    state,
    ...(hint ? { hint } : {}),
    details: status ?? {},
  };
};
