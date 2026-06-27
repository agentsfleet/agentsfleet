"use client";

import type { ComponentType } from "react";
import { useState } from "react";
import {
  Button,
  DashboardRow,
  DashboardRowGroup,
  StatusPill,
  type StatusPillVariant,
} from "@agentsfleet/design-system";
import { BriefcaseIcon, GitPullRequestIcon, HashIcon } from "lucide-react";
import {
  INTEGRATION_CATALOG,
  INTEGRATION_STATUS,
  type Integration,
} from "@/lib/integrations/catalog";
import { EVENTS } from "@/lib/analytics/events";
import { captureProductEvent } from "@/lib/analytics/posthog";

const NOT_CONNECTED_LABEL = "Not connected";
const CONNECTED_LABEL = "Connected";
const TOKEN_STORED_LABEL = "Token stored";
const PLANNED_LABEL = "Planned";
const REQUESTED_LABEL = "Requested";
const CONNECT_GITHUB_LABEL = "Connect GitHub";
const REQUEST_ACCESS_LABEL = "Request access";
const ADD_CUSTOM_SECRET_ID = "#add-custom-secret";
// Planned-connector access requests route to the team inbox; the click also
// fires the EVENTS.integration_requested PostHog event so demand can be
// filtered and studied.
const REQUEST_EMAIL = "agentsfleet@agentmail.to";

function requestMailto(integration: Integration): string {
  const subject = `Integration request: ${integration.name}`;
  const body = `I'd like the ${integration.name} integration for agentsfleet.`;
  return `mailto:${REQUEST_EMAIL}?subject=${encodeURIComponent(subject)}&body=${encodeURIComponent(body)}`;
}

const INTEGRATION_ICON = {
  github: GitPullRequestIcon,
  zoho: BriefcaseIcon,
  slack: HashIcon,
} as const satisfies Record<Integration["id"], ComponentType<{ size?: number }>>;

function integrationStatusLabel({
  isNative,
  isReady,
  requested,
}: {
  isNative: boolean;
  isReady: boolean;
  requested: boolean;
}) {
  if (isReady) return isNative ? CONNECTED_LABEL : TOKEN_STORED_LABEL;
  if (isNative) return NOT_CONNECTED_LABEL;
  return requested ? REQUESTED_LABEL : PLANNED_LABEL;
}

function integrationStatusVariant({
  isReady,
  isNative,
  requested,
}: {
  isReady: boolean;
  isNative: boolean;
  requested: boolean;
}): StatusPillVariant {
  if (isReady) return "success";
  if (isNative || requested) return "warning";
  return "neutral";
}

function IntegrationRow({
  integration,
  storedCredentialNames,
  requested,
  onRequest,
}: {
  integration: Integration;
  storedCredentialNames: ReadonlySet<string>;
  requested: boolean;
  onRequest: (integrationId: Integration["id"]) => void;
}) {
  const Icon = INTEGRATION_ICON[integration.id];
  const isNative = integration.status === INTEGRATION_STATUS.native;
  const isReady = storedCredentialNames.has(integration.requiredSecret);
  const actionLabel = isNative ? CONNECT_GITHUB_LABEL : REQUEST_ACCESS_LABEL;
  const statusLabel = integrationStatusLabel({ isNative, isReady, requested });
  const statusVariant = integrationStatusVariant({ isNative, isReady, requested });
  const description = isNative ? (
    <>
      {integration.description} Store{" "}
      <code className="font-mono">{integration.requiredSecret}</code>.
    </>
  ) : (
    <>
      Planned. Use <code className="font-mono">{integration.requiredSecret}</code> for now.
    </>
  );
  return (
    <DashboardRow
      data-testid={`integration-${integration.id}`}
      icon={<Icon size={16} />}
      title={integration.name}
      description={description}
      action={
        <div className="flex items-center gap-2">
          <StatusPill
            variant={statusVariant}
            dot={isReady || isNative || requested}
          >
            {statusLabel}
          </StatusPill>
          {isReady && isNative ? null : isNative ? (
            <Button asChild variant="outline" size="sm">
              <a href={ADD_CUSTOM_SECRET_ID}>{actionLabel}</a>
            </Button>
          ) : requested ? (
            <Button type="button" variant="outline" size="sm" disabled>
              {REQUESTED_LABEL}
            </Button>
          ) : (
            <Button asChild variant="outline" size="sm">
              <a href={requestMailto(integration)} onClick={() => onRequest(integration.id)}>
                {actionLabel}
              </a>
            </Button>
          )}
        </div>
      }
    />
  );
}

export default function IntegrationsComingSoon({
  credentialNames = [],
}: {
  credentialNames?: readonly string[];
}) {
  const storedCredentialNames = new Set(credentialNames);
  const [requestedIntegrations, setRequestedIntegrations] = useState<ReadonlySet<string>>(
    () => new Set(),
  );

  function requestAccess(integrationId: Integration["id"]) {
    captureProductEvent(EVENTS.integration_requested, { integration_id: integrationId });
    setRequestedIntegrations((prev) => {
      const next = new Set(prev);
      next.add(integrationId);
      return next;
    });
  }

  return (
    <div className="space-y-md" data-testid="integrations-coming-soon">
      <p className="text-body-sm leading-body-sm text-muted-foreground">
        GitHub connects now. Request Zoho or Slack if needed.
      </p>
      <DashboardRowGroup>
        {INTEGRATION_CATALOG.map((integration) => (
          <IntegrationRow
            key={integration.id}
            integration={integration}
            storedCredentialNames={storedCredentialNames}
            requested={requestedIntegrations.has(integration.id)}
            onRequest={requestAccess}
          />
        ))}
      </DashboardRowGroup>
    </div>
  );
}
